//! タイムラインの音声（セリフ・効果音・BGM）を 48kHz ステレオへミックスする。
//!
//! 任意の区間だけを描けるので、GUI のプレビュー再生は全体を作らずに逐次ミックスできる。
use crate::{AudioBuffer, SAMPLE_RATE, decode, resample::resample};
use mmm_core::Timeline;
use std::{collections::HashMap, path::Path, sync::Arc};

/// 1 つのセリフの音声。
#[derive(Clone, Debug)]
pub struct VoiceClip {
    pub start: f64,
    /// 48kHz ステレオ。
    pub audio: Arc<AudioBuffer>,
    pub gain: f32,
}

pub struct MixInput<'a> {
    pub timeline: &'a Timeline,
    pub voices: Vec<VoiceClip>,
    /// 効果音・BGM のパスを解決する基準ディレクトリ。
    pub base_dir: &'a Path,
}

struct Placed {
    start: usize,
    len: usize,
    audio: Arc<AudioBuffer>,
    gain: f32,
    fade_out: usize,
    looped: Option<Looped>,
}

struct Looped {
    offset: usize,
    fade_in: usize,
}

/// デコード済みの効果音・BGM のキャッシュ（GUI で Mixer を作り直すときに再デコードしない）。
pub type SourceCache = std::sync::Mutex<HashMap<String, Option<Arc<AudioBuffer>>>>;

/// 再生できる音源（GUI のプレビュー再生用）。
pub trait Source: Send + Sync {
    fn frames(&self) -> usize;
    /// 48kHz ステレオのインターリーブで `[start, start + count)` を返す。
    fn render(&self, start: usize, count: usize) -> Vec<f32>;
}

impl Source for Mixer {
    fn frames(&self) -> usize {
        self.frames
    }
    fn render(&self, start: usize, count: usize) -> Vec<f32> {
        Mixer::render(self, start, count)
    }
}

/// 1 つの音声をそのまま再生する音源（48kHz ステレオ）。
impl Source for AudioBuffer {
    fn frames(&self) -> usize {
        AudioBuffer::frames(self)
    }
    fn render(&self, start: usize, count: usize) -> Vec<f32> {
        let mut out = vec![0.0; count * 2];
        let a = (start * 2).min(self.samples.len());
        let b = ((start + count) * 2).min(self.samples.len());
        out[..b - a].copy_from_slice(&self.samples[a..b]);
        out
    }
}

pub struct Mixer {
    items: Vec<Placed>,
    pub frames: usize,
    /// 読めなかったファイル（警告として表示する）。
    pub warnings: Vec<String>,
}

fn frames(secs: f64) -> usize {
    (secs.max(0.0) * f64::from(SAMPLE_RATE)).round() as usize
}

/// 48kHz ステレオへ揃える。
pub fn normalize(buf: &AudioBuffer) -> AudioBuffer {
    resample(&buf.to_stereo(), SAMPLE_RATE)
}

impl Mixer {
    pub fn new(input: MixInput<'_>) -> Self {
        Self::with_cache(input, &SourceCache::default())
    }

    pub fn with_cache(input: MixInput<'_>, cache: &SourceCache) -> Self {
        let mut warnings = Vec::new();
        let mut load = |path: &str| -> Option<Arc<AudioBuffer>> {
            if let Some(hit) = cache.lock().unwrap().get(path) {
                if hit.is_none() {
                    warnings.push(format!("{path}: 読めません"));
                }
                return hit.clone();
            }
            let full = input.base_dir.join(path);
            let loaded = match decode::decode_file(&full) {
                Ok(b) => Some(Arc::new(normalize(&b))),
                Err(e) => {
                    warnings.push(e.to_string());
                    None
                }
            };
            cache
                .lock()
                .unwrap()
                .insert(path.to_owned(), loaded.clone());
            loaded
        };
        let mut items = Vec::new();
        for v in &input.voices {
            items.push(Placed {
                start: frames(v.start),
                len: v.audio.frames(),
                audio: v.audio.clone(),
                gain: v.gain,
                fade_out: frames(0.01), // Python 版の audio_fadeout(0.01)
                looped: None,
            });
        }
        for s in &input.timeline.sounds {
            if let Some(audio) = load(&s.path) {
                // ファイル内の開始位置から切り出す（動画の音声のトリミング）
                let audio = if s.offset > 0.0 {
                    let skip = frames(s.offset).min(audio.frames());
                    Arc::new(AudioBuffer::new(
                        audio.rate,
                        audio.channels,
                        audio.samples[skip * 2..].to_vec(),
                    ))
                } else {
                    audio
                };
                let len = s
                    .duration
                    .map_or(audio.frames(), |d| audio.frames().min(frames(d)));
                items.push(Placed {
                    start: frames(s.start),
                    len,
                    audio,
                    gain: s.gain as f32,
                    fade_out: frames(0.02).min(len / 2),
                    looped: None,
                });
            }
        }
        for b in &input.timeline.bgm {
            if let Some(audio) = load(&b.path).filter(|a| a.frames() > 0) {
                items.push(Placed {
                    start: frames(b.start),
                    len: frames(b.end - b.start),
                    audio,
                    gain: b.gain as f32,
                    fade_out: frames(b.fade_out),
                    looped: Some(Looped {
                        offset: frames(b.offset),
                        fade_in: frames(b.fade_in),
                    }),
                });
            }
        }
        Self {
            items,
            frames: frames(input.timeline.duration),
            warnings,
        }
    }

    /// `[start, start + count)` フレームをミックスしてインターリーブのステレオで返す。
    pub fn render(&self, start: usize, count: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; count * 2];
        let end = start + count;
        for item in &self.items {
            let (a, b) = (item.start, item.start + item.len);
            if b <= start || a >= end {
                continue;
            }
            let src = &item.audio.samples;
            let src_frames = item.audio.frames();
            for t in a.max(start)..b.min(end) {
                let local = t - a;
                let mut g = item.gain;
                let remaining = item.len - local;
                if remaining < item.fade_out {
                    g *= remaining as f32 / item.fade_out as f32;
                }
                let idx = match &item.looped {
                    Some(l) => {
                        if local < l.fade_in {
                            g *= local as f32 / l.fade_in as f32;
                        }
                        (l.offset + local) % src_frames
                    }
                    None if local < src_frames => local,
                    None => continue,
                };
                let o = (t - start) * 2;
                out[o] += src[idx * 2] * g;
                out[o + 1] += src[idx * 2 + 1] * g;
            }
        }
        for s in &mut out {
            *s = soft_clip(*s);
        }
        out
    }

    pub fn render_all(&self) -> AudioBuffer {
        AudioBuffer::new(SAMPLE_RATE, 2, self.render(0, self.frames))
    }
}

/// 0.9 を超える部分だけを滑らかに圧縮して ±1 に収める。
fn soft_clip(s: f32) -> f32 {
    let a = s.abs();
    if a <= 0.9 {
        s
    } else {
        s.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmm_core::{Segment, SegmentKind, SoundCue};

    fn tone(secs: f64, amp: f32) -> Arc<AudioBuffer> {
        Arc::new(AudioBuffer::new(
            SAMPLE_RATE,
            2,
            vec![amp; frames(secs) * 2],
        ))
    }

    #[test]
    fn mixes_voices_loops_bgm_and_reports_missing_files() {
        let tl = Timeline {
            segments: vec![Segment {
                scene: 0,
                kind: SegmentKind::Line(0),
                start: 0.0,
                duration: 3.0,
            }],
            sounds: vec![SoundCue {
                path: "missing.mp3".into(),
                start: 0.0,
                duration: Some(1.0),
                gain: 1.0,
                offset: 0.0,
            }],
            bgm: vec![],
            scenes: vec![],
            duration: 3.0,
        };
        let mut m = Mixer::new(MixInput {
            timeline: &tl,
            voices: vec![VoiceClip {
                start: 1.0,
                audio: tone(0.5, 0.5),
                gain: 1.0,
            }],
            base_dir: Path::new("/nonexistent"),
        });
        assert_eq!(m.warnings.len(), 1);
        // BGM はテスト用に直接差し込む（0.25 秒の素材を 3 秒ループ）
        m.items.push(Placed {
            start: 0,
            len: frames(3.0),
            audio: tone(0.25, 0.2),
            gain: 1.0,
            fade_out: frames(1.0),
            looped: Some(Looped {
                offset: 0,
                fade_in: 0,
            }),
        });
        let all = m.render_all();
        assert_eq!(all.frames(), frames(3.0));
        let at = |s: f64| all.samples[frames(s) * 2];
        assert!((at(0.5) - 0.2).abs() < 1e-6, "bgm loops");
        assert!((at(1.2) - 0.7).abs() < 1e-6, "voice + bgm");
        assert!(at(2.5) < 0.11 && at(2.5) > 0.09, "fade out half way");
        // 区間描画は全体描画と一致する
        let part = m.render(frames(1.1), 100);
        assert_eq!(
            part[..],
            all.samples[frames(1.1) * 2..frames(1.1) * 2 + 200]
        );
    }
}
