//! 台本 → 音声合成 → タイムライン → 動画（MP4）/ 音声（MP3）/ 静止画（PNG）。
//!
//! CLI（`mmm`）と GUI（`markdown-movie-maker`）で共有する。
use image::RgbaImage;
use mmm_audio::{AudioBuffer, MixInput, Mixer, SourceCache, VoiceClip, mix::normalize};
use mmm_core::{
    CharacterMaster, Parsed, SegmentKind, Timeline, TimelineInput, character::load_master,
    estimate_duration, parse_script,
};
use mmm_render::{LipSync, Renderer};
use mmm_tts::{EngineKind, EngineSettings, Synthesizer, VoiceRequest, request_for_line};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub use mmm_audio::{Source, SourceCache as AudioCache};
pub use mmm_encode::Backend;
pub use mmm_tts::{Dictionary, available_engines};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] mmm_core::Error),
    #[error(transparent)]
    Render(#[from] mmm_render::Error),
    #[error(transparent)]
    Encode(#[from] mmm_encode::Error),
    #[error(transparent)]
    Audio(#[from] mmm_audio::Error),
    #[error(transparent)]
    Tts(#[from] mmm_tts::Error),
    #[error("画像を保存できません: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("中止しました")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 書き出し・プレビューの設定。
#[derive(Clone, Debug)]
pub struct Options {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// すべてのキャラクターに使うエンジン（`None` ならキャラクター定義に従う）。
    pub force_engine: Option<EngineKind>,
    /// キャラクター定義でエンジン未指定のときのエンジン。
    pub default_engine: EngineKind,
    pub engine: EngineSettings,
    /// 全体 BGM（基準ディレクトリからの相対パスか絶対パス）。
    pub bgm: Option<String>,
    pub bgm_gain: f64,
    /// 字幕フォント（`None` は埋め込みの M+ 1 Bold）。
    pub font: Option<PathBuf>,
    pub backend: Backend,
    /// 純 Rust エンコーダを速さ優先（下書き向け。ファイルは 1〜4 割大きくなる）にする。
    pub fast_encode: bool,
    /// 合成音声のキャッシュ先。
    pub cache_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            force_engine: None,
            default_engine: EngineKind::Voicevox,
            engine: EngineSettings::default(),
            bgm: None,
            bgm_gain: 0.1,
            font: None,
            backend: Backend::Auto,
            fast_encode: false,
            cache_dir: default_cache_dir(),
        }
    }
}

/// OS のキャッシュディレクトリ配下の `MarkdownMovieMaker/voices`。
pub fn default_cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("MMM_CACHE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("MarkdownMovieMaker").join("voices"))
}

/// 台本中の相対パスの基準。`data/character_master.json` を持つ祖先ディレクトリ、なければ台本の場所。
pub fn find_project_root(script: &Path) -> PathBuf {
    let dir = script
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = dir.canonicalize().unwrap_or(dir);
    dir.ancestors()
        .find(|d| d.join("data/character_master.json").exists())
        .map(Path::to_path_buf)
        .unwrap_or(dir)
}

/// 台本・キャラクター定義・基準ディレクトリ。
#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub parsed: Parsed,
    pub master: CharacterMaster,
    pub dictionary: Dictionary,
}

impl Project {
    pub fn load(script: &Path, master: Option<&Path>, root: Option<&Path>) -> Result<Self> {
        let text = std::fs::read_to_string(script)
            .map_err(|e| Error::Other(format!("{}: {e}", script.display())))?;
        let root = root
            .map(Path::to_path_buf)
            .unwrap_or_else(|| find_project_root(script));
        Self::from_text(&text, root, master)
    }

    /// 読み込み済みのキャラクター定義・辞書から作る（GUI で入力のたびにディスクを読まない）。
    pub fn from_parts(
        text: &str,
        root: PathBuf,
        master: CharacterMaster,
        dictionary: Dictionary,
    ) -> Self {
        Self {
            parsed: parse_script(text),
            root,
            master,
            dictionary,
        }
    }

    pub fn from_text(text: &str, root: PathBuf, master: Option<&Path>) -> Result<Self> {
        let master_path = master
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.join("data/character_master.json"));
        let master = load_master(&master_path)?;
        let dictionary = Dictionary::load(&root.join("data/voice_dictionary.csv"))?;
        Ok(Self {
            parsed: parse_script(text),
            root,
            master,
            dictionary,
        })
    }

    pub fn requests(&self, opts: &Options) -> Vec<((usize, usize), VoiceRequest)> {
        let mut out = Vec::new();
        for (si, scene) in self.parsed.script.scenes.iter().enumerate() {
            for (li, line) in scene.lines.iter().enumerate() {
                if let Some(r) = request_for_line(
                    &self.master,
                    scene,
                    line,
                    opts.force_engine,
                    opts.default_engine,
                ) {
                    out.push(((si, li), r));
                }
            }
        }
        out
    }

    pub fn synthesizer(&self, opts: &Options) -> Synthesizer {
        Synthesizer::new(
            opts.engine.clone(),
            self.dictionary.clone(),
            opts.cache_dir.clone(),
        )
    }

    pub fn renderer(&self, opts: &Options) -> Result<Renderer> {
        let font = match &opts.font {
            Some(p) => {
                Some(std::fs::read(p).map_err(|e| Error::Other(format!("{}: {e}", p.display())))?)
            }
            None => None,
        };
        Ok(Renderer::new(
            self.master.clone(),
            self.root.clone(),
            font,
            opts.width,
            opts.height,
        )?)
    }
}

/// 進捗の通知。
#[derive(Clone, Debug)]
pub enum Progress {
    Synth {
        done: usize,
        total: usize,
        text: String,
    },
    Frames {
        done: u64,
        total: u64,
    },
    Message(String),
}

/// 合成済みの音声（48kHz ステレオ）。
#[derive(Clone, Debug, Default)]
pub struct Voices {
    pub clips: HashMap<(usize, usize), Arc<AudioBuffer>>,
    /// 合成に失敗したセリフとエラー。
    pub failures: Vec<((usize, usize), String)>,
}

/// すべてのセリフを合成する。失敗したセリフは `failures` に入れて続ける。
pub fn synthesize_all(
    project: &Project,
    synth: &Synthesizer,
    opts: &Options,
    progress: &(dyn Fn(Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Voices> {
    let requests = project.requests(opts);
    let total = requests.len();
    let mut voices = Voices::default();
    for (i, (key, req)) in requests.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        progress(Progress::Synth {
            done: i,
            total,
            text: req.text.clone(),
        });
        match synth.synthesize(&req) {
            Ok(buf) => {
                voices.clips.insert(key, Arc::new(normalize(&buf)));
            }
            Err(e) => voices.failures.push((key, e.to_string())),
        }
    }
    progress(Progress::Synth {
        done: total,
        total,
        text: String::new(),
    });
    Ok(voices)
}

/// タイムラインと口パクを作る。音声のないセリフは推定の長さを使う（プレビュー用）。
pub fn build_timeline(
    project: &Project,
    voices: &Voices,
    opts: &Options,
    estimate_missing: bool,
) -> (Timeline, LipSync) {
    let script = &project.parsed.script;
    let speakers: HashMap<(usize, usize), f64> = project
        .requests(opts)
        .into_iter()
        .map(|(k, r)| (k, r.speed))
        .collect();
    let duration = |s: usize, l: usize| -> Option<f64> {
        let speed = *speakers.get(&(s, l))?;
        match voices.clips.get(&(s, l)) {
            Some(v) => Some(v.duration()),
            None if estimate_missing => {
                Some(estimate_duration(&script.scenes[s].lines[l].text, speed))
            }
            None => None,
        }
    };
    let timeline = Timeline::build(&TimelineInput {
        script,
        line_duration: &duration,
        global_bgm: opts.bgm.as_deref(),
        bgm_gain: opts.bgm_gain,
    });
    let mut lipsync = LipSync::default();
    for (&(s, l), v) in &voices.clips {
        lipsync.insert(s, l, &v.to_mono().samples, v.rate);
    }
    (timeline, lipsync)
}

pub fn mixer(project: &Project, timeline: &Timeline, voices: &Voices) -> Mixer {
    mixer_cached(project, timeline, voices, &SourceCache::default())
}

pub fn mixer_cached(
    project: &Project,
    timeline: &Timeline,
    voices: &Voices,
    cache: &SourceCache,
) -> Mixer {
    let clips = timeline
        .segments
        .iter()
        .filter_map(|seg| match seg.kind {
            SegmentKind::Line(l) => voices.clips.get(&(seg.scene, l)).map(|a| VoiceClip {
                start: seg.start,
                audio: a.clone(),
                gain: 1.0,
            }),
            _ => None,
        })
        .collect();
    Mixer::with_cache(
        MixInput {
            timeline,
            voices: clips,
            base_dir: &project.root,
        },
        cache,
    )
}

/// MP4 を書き出す。フレームは並列に描画し、順番にエンコーダへ渡す。
#[allow(clippy::too_many_arguments)]
pub fn render_video(
    project: &Project,
    timeline: &Timeline,
    lipsync: &LipSync,
    mixer: &Mixer,
    renderer: &Renderer,
    opts: &Options,
    out: &Path,
    progress: &(dyn Fn(Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<()> {
    if timeline.duration <= 0.0 {
        return Err(Error::Other(
            "動画にするシーンがありません（セリフか間が必要です）".into(),
        ));
    }
    let mut settings = mmm_encode::Settings::new(opts.width, opts.height, opts.fps);
    settings.fast = opts.fast_encode;
    renderer.set_reuse_frames(true);
    let fps = f64::from(settings.fps);
    let total = (timeline.duration * fps).ceil() as u64;
    let script = &project.parsed.script;
    let render = |f: u64| -> Arc<RgbaImage> {
        let img = renderer.frame_shared(script, timeline, lipsync, f as f64 / fps);
        if img.dimensions() == (settings.width, settings.height) {
            img
        } else {
            Arc::new(fit((*img).clone(), settings.width, settings.height))
        }
    };
    let audio = |start: usize, count: usize| -> Vec<f32> {
        let end = (start + count).min(mixer.frames.max(start));
        let mut v = mixer.render(start, end - start);
        v.resize(count * 2, 0.0);
        v
    };

    // 純 Rust エンコーダ: 描画と H.264 符号化を GOP 単位で全コア並列に行う
    #[cfg(feature = "software")]
    if opts.backend.resolve() == Backend::Software {
        mmm_encode::encode_parallel(
            out,
            &settings,
            total,
            &render,
            &audio,
            &|done| progress(Progress::Frames { done, total }),
            cancel,
        )?;
        return Ok(());
    }

    // Media Foundation（GPU）: 描画だけ並列にして順にエンコーダへ渡す
    let mut writer = mmm_encode::create(out, &settings, opts.backend)?;
    let audio_per_frame = 48_000.0 / fps;
    let batch = (rayon::current_num_threads() * 2).max(4) as u64;
    let mut frame = 0;
    while frame < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let end = (frame + batch).min(total);
        let frames: Vec<Arc<RgbaImage>> = (frame..end).into_par_iter().map(render).collect();
        for (i, img) in frames.iter().enumerate() {
            let f = frame + i as u64;
            writer.push_frame(img)?;
            let a = (f as f64 * audio_per_frame).round() as usize;
            let b = ((f + 1) as f64 * audio_per_frame).round() as usize;
            writer.push_audio(&audio(a, b - a))?;
        }
        frame = end;
        progress(Progress::Frames { done: frame, total });
    }
    writer.finish()?;
    Ok(())
}

/// レンダラーの出力を偶数サイズのエンコーダへ合わせる。
fn fit(img: RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.dimensions() == (w, h) {
        img
    } else {
        image::imageops::crop_imm(&img, 0, 0, w, h).to_image()
    }
}

/// 音声だけを MP3 で書き出す（Python 版の `--audio-only`）。
pub fn render_audio(mixer: &Mixer, out: &Path) -> Result<()> {
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let audio = mixer.render_all();
    match out.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("wav") => mmm_audio::wav::write_wav(out, &audio)?,
        _ => mmm_audio::mp3::write_mp3(out, &audio)?,
    }
    Ok(())
}

/// セリフごと（セリフのないシーンは 1 枚）の静止画を書き出す（Python 版の `--image-only`）。
pub fn render_images(
    project: &Project,
    renderer: &Renderer,
    timeline: &Timeline,
    lipsync: &LipSync,
    out: &Path,
) -> Result<Vec<PathBuf>> {
    let dir = out
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let script = &project.parsed.script;
    let mut shots = Vec::new();
    for (si, scene) in script.scenes.iter().enumerate() {
        if timeline.scenes.get(si).is_none_or(|s| s.end <= s.start) {
            continue;
        }
        if scene.lines.is_empty() {
            shots.push((si, None));
        }
        for li in 0..scene.lines.len() {
            if timeline.line_start(si, li).is_some() {
                shots.push((si, Some(li)));
            }
        }
    }
    shots
        .par_iter()
        .enumerate()
        .map(|(i, &(si, li))| {
            let path = dir.join(format!("{stem}_{:03}.png", i + 1));
            renderer
                .still(script, timeline, lipsync, si, li)
                .save(&path)?;
            Ok(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// リポジトリの素材と無音エンジンで、短い台本を MP4・MP3・PNG まで通す。
    #[test]
    fn end_to_end_with_silent_engine() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if !root.join("data/character_master.json").exists() {
            return;
        }
        let text = "# Scene@テスト\n- bg: data/bg/Blackboard.png\n- before_pause: 0.3\n- id: zundamon@default_left.bouncing\n- id: metan@default.shaking\n- sound: data/sound/correct1.mp3 0.5\n- bgm: data/bgm/001.mp3\n\n## Lines\nzundamon: こんにちは\nmetan: Hello\nnobody: 飛ばされる\n";
        let project = Project::from_text(text, root.canonicalize().unwrap(), None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let opts = Options {
            width: 320,
            height: 180,
            fps: 8,
            force_engine: Some(EngineKind::Silent),
            cache_dir: Some(dir.path().join("cache")),
            backend: Backend::Software,
            ..Options::default()
        };
        let synth = project.synthesizer(&opts);
        let cancel = AtomicBool::new(false);
        let voices = synthesize_all(&project, &synth, &opts, &|_| {}, &cancel).unwrap();
        assert_eq!(voices.clips.len(), 2);
        let (timeline, lipsync) = build_timeline(&project, &voices, &opts, false);
        assert_eq!(timeline.segments.len(), 3);
        let mixer = mixer(&project, &timeline, &voices);
        assert!(mixer.warnings.is_empty(), "{:?}", mixer.warnings);
        let renderer = project.renderer(&opts).unwrap();
        let mp4 = dir.path().join("out.mp4");
        render_video(
            &project,
            &timeline,
            &lipsync,
            &mixer,
            &renderer,
            &opts,
            &mp4,
            &|_| {},
            &cancel,
        )
        .unwrap();
        assert!(std::fs::metadata(&mp4).unwrap().len() > 1000);
        assert!(
            renderer.assets.missing().is_empty(),
            "{:?}",
            renderer.assets.missing()
        );
        let mp3 = dir.path().join("out.mp3");
        render_audio(&mixer, &mp3).unwrap();
        let decoded = mmm_audio::decode::decode_file(&mp3).unwrap();
        assert!((decoded.duration() - timeline.duration).abs() < 0.2);
        let pngs = render_images(
            &project,
            &renderer,
            &timeline,
            &lipsync,
            &dir.path().join("still.png"),
        )
        .unwrap();
        assert_eq!(pngs.len(), 2); // 未定義のキャラクターのセリフは飛ばす
    }

    /// 動画アイテム: 自前で書き出した MP4（H.264）を台本に貼り、正しいフレームが描かれるか。
    #[test]
    fn video_item_decodes_frames_at_the_right_time() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let mut s = mmm_encode::Settings::new(160, 90, 10);
        s.audio = false;
        s.gop = 4;
        // 0〜1 秒は赤、1〜2 秒は青
        let frame = |i: u64| {
            let c = if i < 10 {
                [230, 20, 20, 255]
            } else {
                [20, 20, 230, 255]
            };
            RgbaImage::from_pixel(160, 90, image::Rgba(c))
        };
        mmm_encode::encode_parallel(
            &clip,
            &s,
            20,
            &|i| Arc::new(frame(i)),
            &|_, n| vec![0.0; n * 2],
            &|_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        let text = "# Scene\n- length: 3\n- video: clip.mp4 {scale: 12, mute}\n";
        std::fs::write(dir.path().join("master.json"), "{}").unwrap();
        let project = Project::from_text(
            text,
            dir.path().to_path_buf(),
            Some(&dir.path().join("master.json")),
        )
        .unwrap();
        assert!(
            project.parsed.diagnostics.is_empty(),
            "{:?}",
            project.parsed.diagnostics
        );
        let opts = Options {
            width: 192,
            height: 108,
            ..Options::default()
        };
        let (timeline, lipsync) = build_timeline(&project, &Voices::default(), &opts, false);
        let renderer = project.renderer(&opts).unwrap();
        let script = &project.parsed.script;
        let at = |t: f64| {
            renderer
                .frame(script, &timeline, &lipsync, t)
                .get_pixel(96, 54)
                .0
        };
        let (red, blue) = (at(0.5), at(1.5));
        assert!(red[0] > 180 && red[2] < 60, "{red:?}");
        assert!(blue[2] > 180 && blue[0] < 60, "{blue:?}");
        assert!(
            renderer.assets.missing().is_empty(),
            "{:?}",
            renderer.assets.missing()
        );
    }

    /// フレームの使い回しは、毎回描いた結果と 1 画素も違わない。
    #[test]
    fn frame_reuse_is_pixel_identical() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let script = root.join("scripts/ymm4_features_demo.md");
        if !script.exists() {
            return;
        }
        let project = Project::load(&script, None, Some(&root.canonicalize().unwrap())).unwrap();
        let opts = Options {
            width: 320,
            height: 180,
            ..Options::default()
        };
        let (timeline, lipsync) = build_timeline(&project, &Voices::default(), &opts, true);
        let plain = project.renderer(&opts).unwrap();
        let reuse = project.renderer(&opts).unwrap();
        reuse.set_reuse_frames(true);
        let s = &project.parsed.script;
        let mut reused = 0;
        let mut f = 0;
        while f as f64 / 30.0 < timeline.duration {
            let t = f as f64 / 30.0;
            let a = plain.frame(s, &timeline, &lipsync, t);
            let before = std::time::Instant::now();
            let b = reuse.frame(s, &timeline, &lipsync, t);
            if before.elapsed().as_micros() < 300 {
                reused += 1;
            }
            assert!(a == b, "フレーム {f}（{t:.2} 秒）が違う");
            f += 1;
        }
        assert!(reused > 0);
    }
}
