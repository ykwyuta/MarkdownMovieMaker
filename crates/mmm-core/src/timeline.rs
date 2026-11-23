//! 台本と各セリフの音声の長さから、動画全体のタイムラインを組み立てる。
//!
//! シーンごとに「開始前の間 → (セリフ前の間 → セリフ → セリフ後の間)… → 終了後の間」を並べる。
//! セリフの長さは音声の長さで決まる（音声が未合成のプレビューでは推定値を使う）。
//! アイテムの開始・終了（`at: L2` など）は [`Timeline::resolve`] で絶対時刻にする。
use crate::{
    attrs::{Props, SceneTime},
    model::{ItemKind, Script},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    BeforePause,
    /// セリフの前の間。
    Pre(usize),
    Line(usize),
    /// セリフの後の間。
    Wait(usize),
    AfterPause,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    /// `Script::scenes` の添字。
    pub scene: usize,
    pub kind: SegmentKind,
    pub start: f64,
    pub duration: f64,
}

impl Segment {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

/// シーンの時間範囲と、各セリフ（声の部分）の時間範囲。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneSpan {
    pub start: f64,
    pub end: f64,
    pub lines: Vec<Option<(f64, f64)>>,
}

/// 効果音（動画アイテムの音声もここに入る）。
#[derive(Clone, Debug, PartialEq)]
pub struct SoundCue {
    pub path: String,
    pub start: f64,
    /// 最大の再生時間（`None` はファイルの最後まで）。
    pub duration: Option<f64>,
    pub gain: f64,
    /// ファイル内の再生開始位置（秒）。
    pub offset: f64,
}

/// BGM。区間内でループし、終わりでフェードアウトする。
#[derive(Clone, Debug, PartialEq)]
pub struct BgmCue {
    pub path: String,
    pub start: f64,
    pub end: f64,
    /// BGM ファイル内の再生開始位置（ループ前の秒）。区間をまたいで続きから流す。
    pub offset: f64,
    pub gain: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

pub struct TimelineInput<'a> {
    pub script: &'a Script,
    /// (シーン添字, セリフ添字) → 音声の長さ。`None` のセリフは飛ばす。
    pub line_duration: &'a dyn Fn(usize, usize) -> Option<f64>,
    /// 動画全体に流す BGM（`--bgm`）。`bgm:` のあるシーンではそちらが優先される。
    pub global_bgm: Option<&'a str>,
    pub bgm_gain: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    pub segments: Vec<Segment>,
    pub scenes: Vec<SceneSpan>,
    pub sounds: Vec<SoundCue>,
    pub bgm: Vec<BgmCue>,
    pub duration: f64,
}

impl Timeline {
    pub fn build(input: &TimelineInput<'_>) -> Self {
        let mut t = Self::default();
        let mut now = 0.0;
        for (si, scene) in input.script.scenes.iter().enumerate() {
            let scene_start = now;
            let mut span = SceneSpan {
                start: now,
                end: now,
                lines: vec![None; scene.lines.len()],
            };
            let mut push = |kind, duration: f64, now: &mut f64| {
                if duration > 0.0 {
                    t.segments.push(Segment {
                        scene: si,
                        kind,
                        start: *now,
                        duration,
                    });
                    *now += duration;
                }
            };
            push(SegmentKind::BeforePause, scene.before_pause, &mut now);
            for (li, line) in scene.lines.iter().enumerate() {
                if let Some(d) = (input.line_duration)(si, li) {
                    push(SegmentKind::Pre(li), line.pre, &mut now);
                    span.lines[li] = Some((now, now + d));
                    push(SegmentKind::Line(li), d, &mut now);
                    push(SegmentKind::Wait(li), line.wait, &mut now);
                }
            }
            let mut after = scene.after_pause;
            if let Some(min) = scene.length {
                after = after.max(min - (now - scene_start));
            }
            push(SegmentKind::AfterPause, after, &mut now);
            span.end = now;
            t.scenes.push(span);
        }
        t.duration = now;

        for (si, scene) in input.script.scenes.iter().enumerate() {
            let span = &t.scenes[si];
            if span.end <= span.start {
                continue; // 何も表示しないシーン（Python 版と同じく動画に含めない）
            }
            for s in &scene.sounds {
                let start = t.resolve(si, s.at);
                if start < span.end {
                    t.sounds.push(SoundCue {
                        path: s.path.clone(),
                        start,
                        duration: s.duration,
                        gain: s.volume,
                        offset: 0.0,
                    });
                }
            }
            for (li, line) in scene.lines.iter().enumerate() {
                if let (Some(se), Some((a, _))) = (&line.se, span.lines[li]) {
                    t.sounds.push(SoundCue {
                        path: se.clone(),
                        start: a,
                        duration: None,
                        gain: 1.0,
                        offset: 0.0,
                    });
                }
            }
            for item in &scene.items {
                if let ItemKind::Video(v) = &item.kind {
                    // 速度を変えた動画の音声は使わない（音程が変わるため）
                    if v.volume <= 0.0 || (v.speed - 1.0).abs() > 1e-6 {
                        continue;
                    }
                    let (a, b) = t.item_span(si, &item.props);
                    if v.looped {
                        t.bgm.push(BgmCue {
                            path: v.path.clone(),
                            start: a,
                            end: b,
                            offset: v.trim,
                            gain: v.volume,
                            fade_in: 0.0,
                            fade_out: 0.0,
                        });
                    } else {
                        t.sounds.push(SoundCue {
                            path: v.path.clone(),
                            start: a,
                            duration: Some(b - a),
                            gain: v.volume,
                            offset: v.trim,
                        });
                    }
                }
            }
        }
        t.place_bgm(input);
        t
    }

    /// シーン BGM と全体 BGM を配置する。シーン BGM がある区間では全体 BGM を止める。
    fn place_bgm(&mut self, input: &TimelineInput<'_>) {
        let scenes = &input.script.scenes;
        let mut covered: Vec<(f64, f64)> = Vec::new();
        let mut si = 0;
        while si < scenes.len() {
            let scene = &scenes[si];
            let span = self.scenes[si].clone();
            for b in &scene.bgm {
                // `continue` は次に bgm を持つシーンの手前まで流し続ける
                let mut end_scene = si;
                if b.keep {
                    while end_scene + 1 < scenes.len() && scenes[end_scene + 1].bgm.is_empty() {
                        end_scene += 1;
                    }
                }
                let start = b.at.map_or(span.start, |at| self.resolve(si, at));
                let end = match b.until {
                    Some(u) => self.resolve(si, u),
                    None => self.scenes[end_scene].end,
                };
                if end <= start {
                    continue;
                }
                covered.push((start, end));
                let len = end - start;
                self.bgm.push(BgmCue {
                    path: b.path.clone(),
                    start,
                    end,
                    offset: 0.0,
                    gain: b.volume.unwrap_or(input.bgm_gain),
                    fade_in: b.fade_in.min(len / 2.0),
                    fade_out: b.fade_out.unwrap_or(1.0).min(len / 2.0),
                });
            }
            si += 1;
        }
        let Some(path) = input.global_bgm else { return };
        covered.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cursor = 0.0;
        let mut gaps = Vec::new();
        for (a, b) in covered {
            if a > cursor {
                gaps.push((cursor, a));
            }
            cursor = f64::max(cursor, b);
        }
        if self.duration > cursor {
            gaps.push((cursor, self.duration));
        }
        for (a, b) in gaps {
            let fade = ((b - a) / 2.0).min(0.5);
            self.bgm.push(BgmCue {
                path: path.to_owned(),
                start: a,
                end: b,
                offset: a,
                gain: input.bgm_gain,
                fade_in: if a > 0.0 { fade } else { 0.0 },
                fade_out: if b < self.duration {
                    fade
                } else {
                    ((b - a) / 2.0).min(1.0)
                },
            });
        }
    }

    /// シーン内の時刻を絶対時刻へ。範囲外のセリフ番号はシーンの終わりになる。
    pub fn resolve(&self, scene: usize, t: SceneTime) -> f64 {
        let Some(span) = self.scenes.get(scene) else {
            return 0.0;
        };
        let line = |i: usize| span.lines.get(i).copied().flatten();
        let v = match t {
            SceneTime::Secs(s) => span.start + s,
            SceneTime::LineStart(i, o) => line(i).map_or(span.end, |(a, _)| a) + o,
            SceneTime::LineEnd(i, o) => line(i).map_or(span.end, |(_, b)| b) + o,
            SceneTime::End(o) => span.end + o,
        };
        v.clamp(span.start, span.end)
    }

    /// アイテムの開始・終了（絶対時刻）。既定はシーン全体。
    pub fn item_span(&self, scene: usize, props: &Props) -> (f64, f64) {
        let Some(span) = self.scenes.get(scene) else {
            return (0.0, 0.0);
        };
        let start = props.at.map_or(span.start, |at| self.resolve(scene, at));
        let end = match (props.until, props.duration) {
            (Some(u), _) => self.resolve(scene, u),
            (None, Some(d)) => (start + d).min(span.end),
            (None, None) => span.end,
        };
        (start, end.max(start))
    }

    /// 時刻 `time` を含む区間と、その区間内の経過時間。
    pub fn at(&self, time: f64) -> Option<(&Segment, f64)> {
        let last = self.segments.last()?;
        let time = time.clamp(0.0, last.end());
        let i = self
            .segments
            .partition_point(|s| s.end() <= time)
            .min(self.segments.len() - 1);
        let seg = &self.segments[i];
        Some((seg, (time - seg.start).max(0.0)))
    }

    /// シーンの最初の区間の開始時刻。
    pub fn scene_start(&self, scene: usize) -> Option<f64> {
        self.scenes
            .get(scene)
            .filter(|s| s.end > s.start)
            .map(|s| s.start)
    }

    /// セリフの（声の）開始時刻。
    pub fn line_start(&self, scene: usize, line: usize) -> Option<f64> {
        self.scenes
            .get(scene)?
            .lines
            .get(line)
            .copied()
            .flatten()
            .map(|(a, _)| a)
    }
}

/// 音声未合成時の長さの推定（日本語は約 7 文字/秒、英語は約 15 文字/秒）。
pub fn estimate_duration(text: &str, speed: f64) -> f64 {
    let mut secs = 0.3;
    for c in text.chars() {
        secs += if c.is_ascii() { 0.065 } else { 0.14 };
    }
    secs / speed.max(0.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_script;

    fn build(text: &str, skip: Option<(usize, usize)>) -> Timeline {
        let p = parse_script(text);
        let dur = move |s: usize, l: usize| {
            if Some((s, l)) == skip {
                None
            } else {
                Some(2.0)
            }
        };
        Timeline::build(&TimelineInput {
            script: &p.script,
            line_duration: &dur,
            global_bgm: Some("g.mp3"),
            bgm_gain: 0.1,
        })
    }

    #[test]
    fn builds_segments_cues_and_bgm() {
        let t = build(
            "# Scene\n- after_pause: 1\n# Scene\n- before_pause: 0.5\n- bgm: b.mp3\n- sound: s.mp3 1 0.2\n## Lines\na: x\nb: y\n# Scene\n# Scene\n## Lines\na: z\n",
            Some((1, 1)),
        );
        let kinds: Vec<_> = t.segments.iter().map(|s| (s.scene, s.kind)).collect();
        assert_eq!(
            kinds,
            [
                (0, SegmentKind::AfterPause),
                (1, SegmentKind::BeforePause),
                (1, SegmentKind::Line(0)),
                (3, SegmentKind::Line(0)),
            ]
        );
        assert_eq!(t.duration, 5.5);
        assert_eq!(t.sounds[0].start, 1.2);
        let g: Vec<_> = t
            .bgm
            .iter()
            .filter(|b| b.path == "g.mp3")
            .map(|b| (b.start, b.end))
            .collect();
        assert_eq!(g, [(0.0, 1.0), (3.5, 5.5)]);
        assert_eq!(t.at(3.6).unwrap().0.scene, 3);
        assert_eq!(t.at(99.0).unwrap().0.scene, 3);
        assert_eq!(t.line_start(3, 0), Some(3.5));
        assert_eq!(t.scene_start(2), None);
    }

    #[test]
    fn pre_wait_length_item_spans_and_continued_bgm() {
        let t = build(
            concat!(
                "# Scene\n- length: 10\n- bgm: b.mp3 {continue, volume: 0.3}\n",
                "- text: hi {at: L2, until: L2.end+0.5}\n- video: v.mp4 {at: 1, dur: 3}\n",
                "## Lines\na: x {pre: 0.5, wait: 1, se: pop.mp3}\nb: y\n",
                "# Scene\n## Lines\na: z\n# Scene\n- bgm: c.mp3\n## Lines\na: w\n"
            ),
            None,
        );
        let s0 = &t.scenes[0];
        assert_eq!(s0.lines[0], Some((0.5, 2.5)));
        assert_eq!(s0.lines[1], Some((3.5, 5.5)));
        assert_eq!(s0.end, 10.0); // length で延びる
        let p = parse_script("# Scene\n- text: hi {at: L2, until: L2.end+0.5}\n");
        assert_eq!(
            t.item_span(0, &p.script.scenes[0].items[0].props),
            (3.5, 6.0)
        );
        assert!(
            t.sounds
                .iter()
                .any(|s| s.path == "pop.mp3" && s.start == 0.5)
        );
        assert!(
            t.sounds
                .iter()
                .any(|s| s.path == "v.mp4" && s.start == 1.0 && s.duration == Some(3.0))
        );
        // continue した BGM は次の bgm を持つシーンの手前まで続き、全体 BGM はその後だけ
        let b = t.bgm.iter().find(|b| b.path == "b.mp3").unwrap();
        assert_eq!((b.start, b.end, b.gain), (0.0, 12.0, 0.3));
        assert!(t.bgm.iter().all(|b| b.path != "g.mp3"));
    }
}
