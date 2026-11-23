//! Studio の状態と処理（OS に依存しない部分）。Win32 の画面はこれを表示・操作するだけにする。
//!
//! - 台本の再解析とタイムラインの作り直し（未合成のセリフは推定の長さ）
//! - 音声合成・書き出しのバックグラウンド処理と、その通知
//! - シーン一覧・タイムライン表示に使う行データ
use mmm_audio::{AudioBuffer, mix::normalize};
use mmm_core::{ItemKind, SegmentKind, Timeline};
use mmm_pipeline::{
    AudioCache, Backend, Options, Progress, Project, Voices, build_timeline, mixer_cached,
    render_audio, render_images, render_video,
};
use mmm_render::LipSync;
use mmm_tts::{EngineKind, Synthesizer, VoiceRequest, request_for_line};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub const TEMPLATE: &str = "# Scene@はじめに\n\n- bg: data/bg/Blackboard.png\n- before_pause: 0.5\n- id: zundamon@default_left\n- id: metan@default\n\n## Lines\nzundamon: こんにちは、ずんだもんなのだ。\nmetan: 今日はよろしくお願いしますね。\n";

/// バックグラウンド処理からの通知。
pub enum Event {
    Progress(String, f32),
    Voice(String, Arc<AudioBuffer>),
    VoiceFailed(String, String),
    /// 再生用の音源ができた（開始位置）。
    Mixer(Arc<dyn mmm_pipeline::Source>, f64),
    /// 1 つの音声だけを再生する（試聴）。
    Audition(Arc<AudioBuffer>),
    Done {
        label: String,
        result: Result<String, String>,
        play: Option<(usize, usize)>,
    },
}

/// 実行中の処理。
pub struct Job {
    pub label: String,
    pub cancel: Arc<AtomicBool>,
    pub progress: f32,
    pub status: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Video,
    Audio,
    Images,
}

impl ExportKind {
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("mp3" | "wav") => Self::Audio,
            Some("png") => Self::Images,
            _ => Self::Video,
        }
    }
}

/// シーン一覧の 1 行。
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub scene: usize,
    pub line: Option<usize>,
    /// エディタで移動する行（0 始まり）。
    pub source_line: usize,
}

/// スライド（シーン）一覧の 1 枚。
#[derive(Clone, Debug, PartialEq)]
pub struct Slide {
    pub scene: usize,
    pub title: String,
    /// 動画に含まれないシーン（セリフも間もない）なら `None`。
    pub span: Option<(f64, f64)>,
    pub lines: usize,
    pub synthesized: usize,
    pub source_line: usize,
}

impl Slide {
    pub fn subtitle(&self) -> String {
        match self.span {
            Some((a, b)) => format!(
                "{}〜  {:.1} 秒  セリフ {}（合成済み {}）",
                fmt_time(a),
                b - a,
                self.lines,
                self.synthesized
            ),
            None => "（動画に含まれません）".to_owned(),
        }
    }
}

/// タイムライン表示の 1 本の帯。
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub track: usize,
    pub start: f64,
    pub end: f64,
    pub label: String,
    /// 色の種（話者名など）。
    pub color_key: String,
    pub source_line: usize,
}

/// タイムラインのトラック名（上から）。
pub const TRACKS: [&str; 5] = ["シーン", "セリフ", "立ち絵", "アイテム", "音声"];

/// キャラクター定義・読み辞書と、読んだときの更新時刻。
struct AssetsCache {
    root: PathBuf,
    stamps: [Option<std::time::SystemTime>; 2],
    master: mmm_core::CharacterMaster,
    dictionary: mmm_pipeline::Dictionary,
}

fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub fn voice_key(r: &VoiceRequest) -> String {
    format!("{r:?}")
}

pub struct Studio {
    pub path: Option<PathBuf>,
    pub text: String,
    /// 最後に開いた・保存した内容（未保存の変更の確認に使う）。
    pub saved: String,
    pub root: PathBuf,
    pub project: Option<Arc<Project>>,
    pub load_error: Option<String>,
    pub opts: Options,
    pub export_opts: Options,
    pub synth: Arc<Synthesizer>,
    synth_key: String,
    /// 合成済みの音声（要求の内容 → 48kHz ステレオ）。台本を編集しても使い回せる。
    pub voices: HashMap<String, Arc<AudioBuffer>>,
    pub voice_errors: HashMap<String, String>,
    pub audio_cache: Arc<AudioCache>,
    pub timeline: Arc<Timeline>,
    pub lipsync: Arc<LipSync>,
    /// タイムラインを作り直すたびに増える（サムネイルの作り直しに使う）。
    pub generation: usize,
    /// セリフごとの合成要求の鍵（タイムラインを作り直すときに 1 回だけ求める）。
    voice_keys: HashMap<(usize, usize), String>,
    /// 読み込み済みのキャラクター定義と読み辞書（ファイルの更新時刻が変わるまで使い回す）。
    assets_cache: Option<AssetsCache>,
    pub time: f64,
    pub job: Option<Job>,
    pub log: Vec<String>,
    tx: mpsc::Sender<Event>,
    /// 通知を受け取ったら画面側で呼ぶ（Win32 では PostMessage）。
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl Studio {
    pub fn new(tx: mpsc::Sender<Event>, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let opts = Options::default();
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut s = Self {
            path: None,
            text: TEMPLATE.to_owned(),
            saved: String::new(),
            root,
            project: None,
            load_error: None,
            synth: Arc::new(Synthesizer::new(
                opts.engine.clone(),
                Default::default(),
                opts.cache_dir.clone(),
            )),
            synth_key: String::new(),
            export_opts: opts.clone(),
            opts,
            voices: HashMap::new(),
            voice_errors: HashMap::new(),
            audio_cache: Arc::default(),
            timeline: Arc::default(),
            lipsync: Arc::default(),
            generation: 0,
            voice_keys: HashMap::new(),
            assets_cache: None,
            time: 0.0,
            job: None,
            log: Vec::new(),
            tx,
            notify,
        };
        s.reload();
        s
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        if self.log.len() > 300 {
            self.log.remove(0);
        }
    }

    // ---- ファイル --------------------------------------------------------

    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.saved = text.clone();
        self.text = text;
        self.path = Some(path.to_path_buf());
        self.root = mmm_pipeline::find_project_root(path);
        self.time = 0.0;
        self.reload();
        Ok(())
    }

    pub fn save_as(&mut self, path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.text = text.to_owned();
        if self.path.as_deref() != Some(path) {
            self.path = Some(path.to_path_buf());
            self.root = mmm_pipeline::find_project_root(path);
        }
        self.reload();
        Ok(())
    }

    pub fn master_path(&self) -> PathBuf {
        self.root.join("data/character_master.json")
    }

    /// 台本・キャラクター定義を読み直してタイムラインを作り直す。
    /// キャラクター定義・辞書を次の読み込みで必ず読み直す。
    pub fn forget_assets(&mut self) {
        self.assets_cache = None;
    }

    /// キャラクター定義・辞書（ファイルが変わっていなければ前回の結果）。
    fn load_assets(
        &mut self,
    ) -> Result<(mmm_core::CharacterMaster, mmm_pipeline::Dictionary), String> {
        let master_path = self.master_path();
        let dict_path = self.root.join("data/voice_dictionary.csv");
        let stamps = [mtime(&master_path), mtime(&dict_path)];
        if let Some(c) = &self.assets_cache
            && c.root == self.root
            && c.stamps == stamps
        {
            return Ok((c.master.clone(), c.dictionary.clone()));
        }
        let master = mmm_core::character::load_master(&master_path).map_err(|e| e.to_string())?;
        let dictionary = mmm_pipeline::Dictionary::load(&dict_path).map_err(|e| e.to_string())?;
        self.assets_cache = Some(AssetsCache {
            root: self.root.clone(),
            stamps,
            master: master.clone(),
            dictionary: dictionary.clone(),
        });
        Ok((master, dictionary))
    }

    pub fn reload(&mut self) {
        let project = self.load_assets().map(|(master, dictionary)| {
            Project::from_parts(&self.text, self.root.clone(), master, dictionary)
        });
        match project {
            Ok(p) => {
                self.load_error = None;
                let key = format!(
                    "{:?}|{}|{}",
                    self.opts.engine,
                    p.dictionary.entries.len(),
                    self.root.display()
                );
                if key != self.synth_key {
                    self.synth = Arc::new(p.synthesizer(&self.opts));
                    self.synth_key = key;
                }
                self.project = Some(Arc::new(p));
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
                self.project = None;
            }
        }
        self.rebuild_timeline();
    }

    /// 編集中の文を反映する（変わっていなければ何もしない）。
    pub fn set_text(&mut self, text: String) -> bool {
        if text == self.text {
            return false;
        }
        self.text = text;
        self.reload();
        true
    }

    pub fn current_voices(&self, project: &Project) -> Voices {
        let mut v = Voices::default();
        for (key, req) in project.requests(&self.opts) {
            if let Some(a) = self.voices.get(&voice_key(&req)) {
                v.clips.insert(key, a.clone());
            }
        }
        v
    }

    pub fn rebuild_timeline(&mut self) {
        self.generation += 1;
        let Some(project) = self.project.clone() else {
            self.timeline = Arc::default();
            return;
        };
        self.voice_keys = project
            .requests(&self.opts)
            .into_iter()
            .map(|(k, r)| (k, voice_key(&r)))
            .collect();
        let (t, l) = build_timeline(&project, &self.current_voices(&project), &self.opts, true);
        self.timeline = Arc::new(t);
        self.lipsync = Arc::new(l);
        self.time = self.time.clamp(0.0, self.timeline.duration);
    }

    pub fn set_engine(&mut self, engine: Option<EngineKind>) {
        self.opts.force_engine = engine;
        self.export_opts.force_engine = engine;
        self.rebuild_timeline();
    }

    pub fn set_device(&mut self, device: &str) {
        self.opts.engine.device = device.to_owned();
        self.export_opts.engine.device = device.to_owned();
        self.reload();
    }

    // ---- 位置 ------------------------------------------------------------

    /// エディタの行に対応する (シーン, セリフ)。
    pub fn item_at_line(&self, line: usize) -> Option<(usize, Option<usize>)> {
        let script = &self.project.as_ref()?.parsed.script;
        let si = script.scenes.iter().rposition(|s| s.source_line <= line)?;
        let li = script.scenes[si]
            .lines
            .iter()
            .rposition(|l| l.source_line <= line);
        Some((si, li))
    }

    /// エディタの行に対応する時刻。
    pub fn time_for_line(&self, line: usize) -> Option<f64> {
        let (s, l) = self.item_at_line(line)?;
        l.and_then(|l| self.timeline.line_start(s, l))
            .or_else(|| self.timeline.scene_start(s))
    }

    /// 現在の (シーン, セリフ)。
    pub fn current(&self) -> Option<(usize, Option<usize>)> {
        let (seg, _) = self.timeline.at(self.time)?;
        Some((
            seg.scene,
            match seg.kind {
                SegmentKind::Line(l) => Some(l),
                _ => None,
            },
        ))
    }

    /// 現在の場所の説明（ステータスバー）。
    pub fn describe_current(&self) -> String {
        let (Some(project), Some((s, l))) = (&self.project, self.current()) else {
            return String::new();
        };
        let scene = &project.parsed.script.scenes[s];
        match l.and_then(|l| scene.lines.get(l)) {
            Some(line) => format!("シーン {} ／ {}: {}", s + 1, line.speaker, line.text),
            None => format!("シーン {}", s + 1),
        }
    }

    // ---- 一覧・タイムライン ----------------------------------------------

    pub fn rows(&self) -> Vec<Row> {
        self.rows_for(None)
    }

    /// `only` のシーンだけ（`None` は全部）の一覧。
    fn rows_for(&self, only: Option<usize>) -> Vec<Row> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let requests = &self.voice_keys;
        let mut rows = Vec::new();
        for (si, scene) in project.parsed.script.scenes.iter().enumerate() {
            if only.is_some_and(|o| o != si) {
                continue;
            }
            let start = self
                .timeline
                .scene_start(si)
                .map_or("（表示なし）".to_owned(), fmt_time);
            rows.push(Row {
                label: format!(
                    "{:>2}. {}  {start}",
                    si + 1,
                    scene.name.as_deref().unwrap_or("（名前なし）")
                ),
                scene: si,
                line: None,
                source_line: scene.source_line,
            });
            for (li, line) in scene.lines.iter().enumerate() {
                let mark = match requests.get(&(si, li)) {
                    None => "[未定義]",
                    Some(k) if self.voices.contains_key(k) => "[済]",
                    Some(k) if self.voice_errors.contains_key(k) => "[失敗]",
                    Some(_) => "[推定]",
                };
                let text: String = line.text.chars().take(28).collect();
                rows.push(Row {
                    label: format!("    {mark} {}: {text}", line.speaker),
                    scene: si,
                    line: Some(li),
                    source_line: line.source_line,
                });
            }
        }
        rows
    }

    /// スライド（シーン）一覧。
    pub fn slides(&self) -> Vec<Slide> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let done: std::collections::HashSet<(usize, usize)> = self
            .voice_keys
            .iter()
            .filter(|(_, k)| self.voices.contains_key(*k))
            .map(|(k, _)| *k)
            .collect();
        project
            .parsed
            .script
            .scenes
            .iter()
            .enumerate()
            .map(|(si, scene)| Slide {
                scene: si,
                title: format!(
                    "{}. {}",
                    si + 1,
                    scene.name.as_deref().unwrap_or("（名前なし）")
                ),
                span: self
                    .timeline
                    .scenes
                    .get(si)
                    .filter(|s| s.end > s.start)
                    .map(|s| (s.start, s.end)),
                lines: scene.lines.len(),
                synthesized: (0..scene.lines.len())
                    .filter(|l| done.contains(&(si, *l)))
                    .count(),
                source_line: scene.source_line,
            })
            .collect()
    }

    /// シーンのセリフ一覧（ノート）。
    pub fn scene_rows(&self, scene: usize) -> Vec<Row> {
        self.rows_for(Some(scene))
            .into_iter()
            .filter(|r| r.line.is_some())
            .collect()
    }

    /// シーンごとのサムネイルの鍵。台本のそのシーンの部分・長さ・合成済みの音声・
    /// キャラクター定義が変わらなければ同じ値になり、描き直さずに済む。
    pub fn scene_keys(&self) -> Vec<u64> {
        use std::hash::{Hash, Hasher};
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let lines: Vec<&str> = self.text.lines().collect();
        let scenes = &project.parsed.script.scenes;
        let mut master = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", project.master).hash(&mut master);
        let master = master.finish();
        scenes
            .iter()
            .enumerate()
            .map(|(si, scene)| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                // 背景は前のシーンから引き継ぐので、前のシーンの背景も鍵に入れる
                (master, &scene.background, si).hash(&mut h);
                let end = scenes.get(si + 1).map_or(lines.len(), |n| n.source_line);
                lines
                    .get(scene.source_line..end.min(lines.len()))
                    .hash(&mut h);
                if let Some(span) = self.timeline.scenes.get(si) {
                    for (a, b) in span.lines.iter().flatten() {
                        (b - a).to_bits().hash(&mut h);
                    }
                    (span.end - span.start).to_bits().hash(&mut h);
                }
                for li in 0..scene.lines.len() {
                    self.voice_keys
                        .get(&(si, li))
                        .filter(|k| self.voices.contains_key(*k))
                        .hash(&mut h);
                }
                h.finish()
            })
            .collect()
    }

    /// シーンの時間範囲。
    pub fn scene_span(&self, scene: usize) -> Option<(f64, f64)> {
        self.timeline
            .scenes
            .get(scene)
            .filter(|s| s.end > s.start)
            .map(|s| (s.start, s.end))
    }

    /// 動画に含まれる、`scene` の前（`dir < 0`）・後（`dir > 0`）のシーン。
    pub fn neighbor_scene(&self, scene: usize, dir: i32) -> Option<usize> {
        let n = self.timeline.scenes.len();
        let mut i = scene as i64;
        loop {
            i += i64::from(dir.signum());
            if i < 0 || i >= n as i64 {
                return None;
            }
            if self.scene_span(i as usize).is_some() {
                return Some(i as usize);
            }
        }
    }

    pub fn bars(&self) -> Vec<Bar> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let tl = &self.timeline;
        let mut bars = Vec::new();
        for (si, scene) in project.parsed.script.scenes.iter().enumerate() {
            let span = &tl.scenes[si];
            if span.end <= span.start {
                continue;
            }
            bars.push(Bar {
                track: 0,
                start: span.start,
                end: span.end,
                label: format!("{} {}", si + 1, scene.name.as_deref().unwrap_or("")),
                color_key: format!("scene{}", si % 2),
                source_line: scene.source_line,
            });
            for (li, line) in scene.lines.iter().enumerate() {
                if let Some((a, b)) = span.lines[li] {
                    bars.push(Bar {
                        track: 1,
                        start: a,
                        end: b,
                        label: format!("{}: {}", line.speaker, line.text),
                        color_key: line.speaker.clone(),
                        source_line: line.source_line,
                    });
                }
            }
            for c in &scene.characters {
                let (a, b) = tl.item_span(si, &c.props);
                bars.push(Bar {
                    track: 2,
                    start: a,
                    end: b,
                    label: format!("{}@{}", c.id, c.variation),
                    color_key: c.id.clone(),
                    source_line: scene.source_line,
                });
            }
            for item in &scene.items {
                let (a, b) = tl.item_span(si, &item.props);
                let label = match &item.kind {
                    ItemKind::Text(t) => format!("T {}", t.text.replace('\n', " ")),
                    ItemKind::Image(p) => format!("画像 {}", file_name(p)),
                    ItemKind::Video(v) => format!("動画 {}", file_name(&v.path)),
                    ItemKind::Shape(_) => "図形".to_owned(),
                };
                bars.push(Bar {
                    track: 3,
                    start: a,
                    end: b,
                    label,
                    color_key: "item".into(),
                    source_line: item.source_line,
                });
            }
        }
        for b in &tl.bgm {
            bars.push(Bar {
                track: 4,
                start: b.start,
                end: b.end,
                label: format!("BGM {}", file_name(&b.path)),
                color_key: "bgm".into(),
                source_line: 0,
            });
        }
        for s in &tl.sounds {
            let end = s.start + s.duration.unwrap_or(0.5).min(3.0);
            bars.push(Bar {
                track: 4,
                start: s.start,
                end,
                label: file_name(&s.path),
                color_key: "se".into(),
                source_line: 0,
            });
        }
        bars
    }

    // ---- 再生 ------------------------------------------------------------

    /// 現在の時刻から再生する音源を別スレッドで作る（できたら `Event::Mixer`）。
    pub fn prepare_playback(&self) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let voices = self.current_voices(&project);
        let timeline = self.timeline.clone();
        let cache = self.audio_cache.clone();
        let tx = self.tx.clone();
        let notify = self.notify.clone();
        let from = self.time;
        thread::spawn(move || {
            let m = mixer_cached(&project, &timeline, &voices, &cache);
            let _ = tx.send(Event::Mixer(Arc::new(m), from));
            notify();
        });
    }

    // ---- 音声合成 --------------------------------------------------------

    /// 未合成のセリフ（`only` なら 1 つ）を合成する。`play` なら終わったらそのセリフを再生する。
    pub fn synthesize(&mut self, only: Option<(usize, usize)>, play: bool) {
        let Some(project) = self.project.clone() else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        let pending: Vec<(String, VoiceRequest)> = project
            .requests(&self.opts)
            .into_iter()
            .filter(|(k, _)| only.is_none_or(|o| o == *k))
            .map(|(_, r)| (voice_key(&r), r))
            .filter(|(k, _)| only.is_some() || !self.voices.contains_key(k))
            .collect();
        if pending.is_empty() {
            self.log("合成が必要なセリフはありません");
            if play {
                let _ = self.tx.send(Event::Done {
                    label: "音声合成".into(),
                    result: Ok(String::new()),
                    play: only,
                });
                (self.notify)();
            }
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job {
            label: "音声合成".into(),
            cancel: cancel.clone(),
            progress: 0.0,
            status: "モデルを準備しています（初回はダウンロードに時間がかかります）".into(),
        });
        let synth = self.synth.clone();
        let tx = self.tx.clone();
        let notify = self.notify.clone();
        thread::spawn(move || {
            let total = pending.len();
            for (i, (key, req)) in pending.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(Event::Done {
                        label: "音声合成".into(),
                        result: Err("中止しました".into()),
                        play: None,
                    });
                    notify();
                    return;
                }
                let _ = tx.send(Event::Progress(
                    format!("[{}/{total}] {}", i + 1, req.text),
                    i as f32 / total as f32,
                ));
                notify();
                let ev = match synth.synthesize(&req) {
                    Ok(buf) => Event::Voice(key, Arc::new(normalize(&buf))),
                    Err(e) => Event::VoiceFailed(key, e.to_string()),
                };
                let _ = tx.send(ev);
                notify();
            }
            let _ = tx.send(Event::Done {
                label: "音声合成".into(),
                result: Ok(format!("{total} 件の音声を合成しました")),
                play: only,
            });
            notify();
        });
    }

    /// キャラクター `id` の声を 1 文だけ合成して試聴する。
    pub fn audition(&mut self, id: &str) {
        let Some(project) = self.project.clone() else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        let name = project
            .master
            .get(id)
            .map_or(id.to_owned(), |c| c.name.clone());
        let parsed = mmm_core::parse_script(&format!(
            "# Scene\n## Lines\n{id}: こんにちは、{name}です。よろしくお願いします。\n"
        ));
        let scene = &parsed.script.scenes[0];
        let Some(req) = request_for_line(
            &project.master,
            scene,
            &scene.lines[0],
            self.opts.force_engine,
            self.opts.default_engine,
        ) else {
            self.log(format!("キャラクター {id} が定義にありません"));
            return;
        };
        self.job = Some(Job {
            label: "試聴".into(),
            cancel: Arc::default(),
            progress: 0.0,
            status: format!("{} で合成中", req.engine.label()),
        });
        let synth = self.synth.clone();
        let tx = self.tx.clone();
        let notify = self.notify.clone();
        thread::spawn(move || {
            let result = match synth.synthesize(&req) {
                Ok(b) => {
                    let _ = tx.send(Event::Audition(Arc::new(normalize(&b))));
                    Ok("試聴を再生します".to_owned())
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(Event::Done {
                label: "試聴".into(),
                result,
                play: None,
            });
            notify();
        });
    }

    // ---- 書き出し --------------------------------------------------------

    pub fn export(&mut self, out: PathBuf) {
        let Some(project) = self.project.clone() else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        let kind = ExportKind::from_path(&out);
        let opts = self.export_opts.clone();
        let synth = self.synth.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job {
            label: "書き出し".into(),
            cancel: cancel.clone(),
            progress: 0.0,
            status: "準備中".into(),
        });
        let tx = self.tx.clone();
        let notify = self.notify.clone();
        let known = self.voices.clone();
        thread::spawn(move || {
            let report = |p: Progress| {
                let (msg, f) = match p {
                    Progress::Synth { done, total, text } => (
                        format!("音声 {done}/{total} {text}"),
                        0.3 * done as f32 / total.max(1) as f32,
                    ),
                    Progress::Frames { done, total } => (
                        format!("映像 {done}/{total}"),
                        0.3 + 0.7 * done as f32 / total.max(1) as f32,
                    ),
                    Progress::Message(m) => (m, 0.0),
                };
                let _ = tx.send(Event::Progress(msg, f));
                notify();
            };
            let result = (|| -> Result<String, String> {
                let renderer = project.renderer(&opts).map_err(|e| e.to_string())?;
                let mut voices = Voices::default();
                if kind != ExportKind::Images {
                    let requests = project.requests(&opts);
                    let total = requests.len();
                    for (i, (key, req)) in requests.into_iter().enumerate() {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("中止しました".into());
                        }
                        let vk = voice_key(&req);
                        if let Some(a) = known.get(&vk) {
                            voices.clips.insert(key, a.clone());
                            continue;
                        }
                        report(Progress::Synth {
                            done: i,
                            total,
                            text: req.text.clone(),
                        });
                        let b = synth
                            .synthesize(&req)
                            .map_err(|e| format!("「{}」を合成できません: {e}", req.text))?;
                        let a = Arc::new(normalize(&b));
                        let _ = tx.send(Event::Voice(vk, a.clone()));
                        voices.clips.insert(key, a);
                    }
                }
                let (timeline, lipsync) =
                    build_timeline(&project, &voices, &opts, kind == ExportKind::Images);
                match kind {
                    ExportKind::Images => {
                        let files = render_images(&project, &renderer, &timeline, &lipsync, &out)
                            .map_err(|e| e.to_string())?;
                        return Ok(format!("{} 枚の画像を書き出しました", files.len()));
                    }
                    ExportKind::Audio => {
                        let mix = mmm_pipeline::mixer(&project, &timeline, &voices);
                        render_audio(&mix, &out).map_err(|e| e.to_string())?;
                    }
                    ExportKind::Video => {
                        let mix = mmm_pipeline::mixer(&project, &timeline, &voices);
                        render_video(
                            &project, &timeline, &lipsync, &mix, &renderer, &opts, &out, &report,
                            &cancel,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                Ok(format!("書き出しました: {}", out.display()))
            })();
            let _ = tx.send(Event::Done {
                label: "書き出し".into(),
                result,
                play: None,
            });
            notify();
        });
    }

    /// 通知を反映する。戻り値は（タイムラインが変わったか, 再生を始めるセリフ）。
    pub fn apply(&mut self, ev: Event) -> (bool, Option<Option<(usize, usize)>>) {
        match ev {
            Event::Progress(s, f) => {
                if let Some(j) = &mut self.job {
                    j.status = s;
                    j.progress = f;
                }
                (false, None)
            }
            Event::Voice(k, a) => {
                self.voice_errors.remove(&k);
                self.voices.insert(k, a);
                (true, None)
            }
            Event::VoiceFailed(k, e) => {
                self.log(format!("合成エラー: {e}"));
                self.voice_errors.insert(k, e);
                (false, None)
            }
            Event::Done {
                label,
                result,
                play,
            } => {
                self.job = None;
                match result {
                    Ok(s) if !s.is_empty() => self.log(s),
                    Ok(_) => {}
                    Err(e) => self.log(format!("{label}: {e}")),
                }
                (false, play.map(Some))
            }
            Event::Mixer(..) | Event::Audition(_) => (false, None),
        }
    }

    pub fn cancel_job(&self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// 書き出しの設定（解像度・フレームレート・エンコーダ）。
    pub fn set_export(
        &mut self,
        size: Option<(u32, u32)>,
        fps: Option<u32>,
        backend: Option<Backend>,
    ) {
        if let Some((w, h)) = size {
            self.export_opts.width = w;
            self.export_opts.height = h;
        }
        if let Some(f) = fps {
            self.export_opts.fps = f;
        }
        if let Some(b) = backend {
            self.export_opts.backend = b;
        }
    }
}

pub fn fmt_time(t: f64) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0) as u64, t % 60.0)
}

fn file_name(p: &str) -> String {
    Path::new(p)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(p)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> (Studio, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let mut s = Studio::new(tx, Arc::new(|| {}));
        s.root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        s.opts.force_engine = Some(EngineKind::Silent);
        s.opts.cache_dir = None;
        s.reload();
        (s, rx)
    }

    #[test]
    fn rows_bars_and_line_lookup_follow_the_script() {
        let (mut s, _rx) = studio();
        assert!(s.load_error.is_none(), "{:?}", s.load_error);
        s.set_text("# Scene@A\n- text: hi {at: L2}\n- bgm: data/bgm/001.mp3\n## Lines\nzundamon: one\nmetan: two\n".into());
        let rows = s.rows();
        assert_eq!(rows.len(), 3);
        assert!(rows[1].label.contains("[推定]"));
        assert_eq!(s.item_at_line(5), Some((0, Some(1))));
        assert_eq!(s.time_for_line(5), s.timeline.line_start(0, 1));
        let bars = s.bars();
        assert!(bars.iter().any(|b| b.track == 3
            && b.label == "T hi"
            && Some(b.start) == s.timeline.line_start(0, 1)));
        assert!(
            bars.iter()
                .any(|b| b.track == 4 && b.label.starts_with("BGM"))
        );
        assert!(!s.set_text(s.text.clone()));
    }

    #[test]
    fn slides_notes_and_scene_navigation() {
        let (mut s, _rx) = studio();
        s.set_text(
            "# Scene@A\n## Lines\nzundamon: one\nmetan: two\n# Scene@空\n# Scene@C\n- length: 2\n"
                .into(),
        );
        let slides = s.slides();
        assert_eq!(slides.len(), 3);
        assert_eq!(slides[0].title, "1. A");
        assert_eq!((slides[0].lines, slides[0].synthesized), (2, 0));
        assert!(slides[1].span.is_none());
        assert_eq!(slides[2].span.map(|(a, b)| b - a), Some(2.0));
        assert_eq!(s.scene_rows(0).len(), 2);
        assert_eq!(s.neighbor_scene(0, 1), Some(2)); // 空のシーンは飛ばす
        assert_eq!(s.neighbor_scene(2, -1), Some(0));
        assert_eq!(s.neighbor_scene(2, 1), None);
        let g = s.generation;
        s.rebuild_timeline();
        assert_eq!(s.generation, g + 1);
        // サムネイルの鍵: 2 つ目のシーンだけを書き換えると、そのシーン（と以降の背景を引き継ぐシーン）だけ変わる
        let before = s.scene_keys();
        s.set_text(
            s.text
                .replace("# Scene@C\n- length: 2", "# Scene@C\n- length: 3"),
        );
        let after = s.scene_keys();
        assert_eq!(before[0], after[0]);
        assert_ne!(before[2], after[2]);
    }

    #[test]
    fn synthesize_updates_voices_and_timeline() {
        let (mut s, rx) = studio();
        s.set_text("# Scene\n## Lines\nzundamon: こんにちは\n".into());
        s.synthesize(None, false);
        assert!(s.job.is_some());
        let mut changed = false;
        while let Ok(ev) = rx.recv_timeout(std::time::Duration::from_secs(30)) {
            let done = matches!(ev, Event::Done { .. });
            changed |= s.apply(ev).0;
            if done {
                break;
            }
        }
        assert!(changed && s.job.is_none());
        // キャラクター定義は更新時刻が変わらない限り読み直さない
        assert!(s.assets_cache.is_some());
        assert!(s.rows()[1].label.contains("[済]"));
        s.rebuild_timeline();
        assert!(s.timeline.duration > 0.5);
    }

    #[test]
    fn export_kind_from_extension() {
        assert_eq!(ExportKind::from_path(Path::new("a.MP3")), ExportKind::Audio);
        assert_eq!(
            ExportKind::from_path(Path::new("a.png")),
            ExportKind::Images
        );
        assert_eq!(ExportKind::from_path(Path::new("a.mp4")), ExportKind::Video);
    }

    /// UI の処理の時間（`cargo test --release -- --ignored --nocapture ui_costs`）。
    #[test]
    #[ignore]
    fn ui_costs() {
        let (mut s, _rx) = studio();
        let script = s
            .root
            .join("scripts/zundamon_metan_talk_civil_low_fraud.md");
        let text = std::fs::read_to_string(script).unwrap();
        let time = |label: &str, n: u32, f: &mut dyn FnMut()| {
            let t = std::time::Instant::now();
            for _ in 0..n {
                f();
            }
            println!(
                "{label:40} {:8.3} ms",
                t.elapsed().as_secs_f64() * 1000.0 / f64::from(n)
            );
        };
        let root = s.root.clone();
        time(
            "従来: Project::from_text（毎回ディスク）",
            20,
            &mut || {
                let _ = Project::from_text(&text, root.clone(), None).unwrap();
            },
        );
        let mut i = 0;
        time("改善: set_text（キャッシュ）", 20, &mut || {
            i += 1;
            s.set_text(format!("{text}\n<!-- {i} -->"));
        });
        let p = s.project.clone().unwrap();
        time(
            "従来: requests を 3 回（一覧の作り直し）",
            20,
            &mut || {
                for _ in 0..3 {
                    let _: Vec<_> = p
                        .requests(&s.opts)
                        .into_iter()
                        .map(|(k, r)| (k, voice_key(&r)))
                        .collect();
                }
            },
        );
        time("改善: rows + slides + scene_rows", 20, &mut || {
            let _ = (s.rows(), s.slides(), s.scene_rows(3));
        });
        time("scene_keys（サムネイルの鍵）", 20, &mut || {
            let _ = s.scene_keys();
        });
        // プレビュー再生（1280×720、30fps で 10 秒分を連続で描く）
        let opts = mmm_pipeline::Options {
            width: 1280,
            height: 720,
            ..mmm_pipeline::Options::default()
        };
        let script = &p.parsed.script;
        for reuse in [false, true] {
            let r = p.renderer(&opts).unwrap();
            r.set_reuse_frames(reuse);
            let t = std::time::Instant::now();
            for f in 0..300 {
                let _ = r.frame_shared(script, &s.timeline, &s.lipsync, 20.0 + f64::from(f) / 30.0);
            }
            println!(
                "プレビュー再生 1 フレーム（使い回し {reuse:5}）   {:8.3} ms",
                t.elapsed().as_secs_f64() * 1000.0 / 300.0
            );
        }
        // サムネイル: 全シーンを描く場合と、1 シーンだけ描き直す場合
        let r = p
            .renderer(&mmm_pipeline::Options {
                width: 240,
                height: 134,
                ..mmm_pipeline::Options::default()
            })
            .unwrap();
        let n = s.timeline.scenes.iter().filter(|x| x.end > x.start).count();
        let t = std::time::Instant::now();
        for si in 0..s.timeline.scenes.len() {
            if s.timeline.scenes[si].end > s.timeline.scenes[si].start {
                let _ = r.still(script, &s.timeline, &s.lipsync, si, None);
            }
        }
        println!(
            "サムネイル: 全 {n} シーン                       {:8.3} ms（変更後は変わったシーンだけ）",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
}
