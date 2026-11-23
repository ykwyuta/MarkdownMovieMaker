//! 口パク（あいうえお）と目パチ。
//!
//! 音声を 50ms ごとに解析し、音量で有声区間を、LPC で求めた第 1・第 2 フォルマントで母音を判定する
//! （YMM4 の「音声データを解析して口パク」と同じ考え方。音声合成エンジンに依存しない）。
//! 開・閉の 2 枚しかない立ち絵は、声が出ている間 10Hz で開閉する。
use std::collections::HashMap;

pub const WINDOW: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mouth {
    Closed,
    A,
    I,
    U,
    E,
    O,
}

impl Mouth {
    pub fn key(self) -> &'static str {
        match self {
            Self::Closed => "n",
            Self::A => "a",
            Self::I => "i",
            Self::U => "u",
            Self::E => "e",
            Self::O => "o",
        }
    }

    /// 画像がない母音の代わり（例: `e` がなければ `i`、`o` がなければ `a`）。
    pub fn fallbacks(self) -> &'static [&'static str] {
        match self {
            Self::Closed => &["n", "u"],
            Self::A => &["a", "o", "e"],
            Self::I => &["i", "e", "u", "a"],
            Self::U => &["u", "o", "i", "a"],
            Self::E => &["e", "i", "a"],
            Self::O => &["o", "a", "u"],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LipSync {
    /// (シーン, セリフ) → 50ms ごとの口の形。
    tracks: HashMap<(usize, usize), Vec<Mouth>>,
}

/// Levinson-Durbin で LPC 係数（a[0] = 1）を求める。
fn lpc(frame: &[f32], order: usize) -> Option<Vec<f64>> {
    let r: Vec<f64> = (0..=order)
        .map(|k| {
            frame
                .iter()
                .zip(&frame[k..])
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum()
        })
        .collect();
    if r[0] <= 1e-9 {
        return None;
    }
    let mut a = vec![0.0; order + 1];
    a[0] = 1.0;
    let mut e = r[0];
    for i in 1..=order {
        let mut acc = r[i];
        for j in 1..i {
            acc += a[j] * r[i - j];
        }
        let k = -acc / e;
        let prev = a.clone();
        for j in 1..i {
            a[j] = prev[j] + k * prev[i - j];
        }
        a[i] = k;
        e *= 1.0 - k * k;
        if e <= 0.0 {
            return None;
        }
    }
    Some(a)
}

/// LPC スペクトル包絡の山から第 1・第 2 フォルマント（Hz）を求める。
fn formants(frame: &[f32], rate: f64) -> Option<(f64, f64)> {
    let a = lpc(frame, 10)?;
    let step = 20.0;
    let n = (4000.0 / step) as usize;
    let spec: Vec<f64> = (0..n)
        .map(|i| {
            let w = std::f64::consts::TAU * i as f64 * step / rate;
            let (mut re, mut im) = (0.0, 0.0);
            for (k, c) in a.iter().enumerate() {
                re += c * (w * k as f64).cos();
                im -= c * (w * k as f64).sin();
            }
            -(re * re + im * im).ln()
        })
        .collect();
    let peaks: Vec<f64> = (1..n - 1)
        .filter(|&i| spec[i] > spec[i - 1] && spec[i] >= spec[i + 1] && i as f64 * step > 200.0)
        .map(|i| i as f64 * step)
        .collect();
    match peaks.as_slice() {
        [f1, f2, ..] => Some((*f1, *f2)),
        _ => None,
    }
}

/// 日本語の母音の平均的なフォルマント（成人）に最も近いもの。
fn classify(f1: f64, f2: f64) -> Mouth {
    const TABLE: [(Mouth, f64, f64); 5] = [
        (Mouth::A, 800.0, 1300.0),
        (Mouth::I, 320.0, 2400.0),
        (Mouth::U, 360.0, 1400.0),
        (Mouth::E, 500.0, 2000.0),
        (Mouth::O, 500.0, 900.0),
    ];
    TABLE
        .iter()
        .min_by(|a, b| {
            let d = |&(_, x, y): &(Mouth, f64, f64)| {
                (f1.ln() - x.ln()).powi(2) * 2.0 + (f2.ln() - y.ln()).powi(2)
            };
            d(a).total_cmp(&d(b))
        })
        .map(|t| t.0)
        .unwrap_or(Mouth::A)
}

/// モノラル音声から 50ms ごとの口の形を求める。
pub fn analyze(samples: &[f32], rate: u32) -> Vec<Mouth> {
    // 約 12kHz へ間引いて解析する（フォルマントは 4kHz 以下）
    let decim = (rate / 12_000).max(1) as usize;
    let low: Vec<f32> = samples
        .chunks(decim)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();
    let r = f64::from(rate) / decim as f64;
    let n = ((WINDOW * r) as usize).max(16);
    let rms: Vec<f32> = low
        .chunks(n)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = rms.iter().copied().fold(0.0, f32::max);
    let threshold = (peak * 0.12).max(0.005);
    low.chunks(n)
        .zip(&rms)
        .map(|(c, &level)| {
            if level <= threshold || c.len() < 32 {
                return Mouth::Closed;
            }
            // プリエンファシス + ハミング窓
            let len = c.len();
            let frame: Vec<f32> = (0..len)
                .map(|i| {
                    let x = c[i] - if i > 0 { 0.95 * c[i - 1] } else { 0.0 };
                    x * (0.54 - 0.46 * (std::f32::consts::TAU * i as f32 / (len - 1) as f32).cos())
                })
                .collect();
            formants(&frame, r).map_or(Mouth::A, |(f1, f2)| classify(f1, f2))
        })
        .collect()
}

impl LipSync {
    /// セリフの音声（モノラル）を解析して登録する。
    pub fn insert(&mut self, scene: usize, line: usize, samples: &[f32], rate: u32) {
        self.tracks.insert((scene, line), analyze(samples, rate));
    }

    /// 口の形。音声がない（推定の長さでプレビュー中）なら 10Hz で「あ」と閉じを繰り返す。
    pub fn mouth(&self, scene: usize, line: usize, t: f64, duration: f64) -> Mouth {
        match self.tracks.get(&(scene, line)) {
            Some(v) => v
                .get((t / WINDOW) as usize)
                .copied()
                .unwrap_or(Mouth::Closed),
            None if (t * 10.0) as i64 % 2 == 1 && t < duration - 0.1 => Mouth::A,
            None => Mouth::Closed,
        }
    }

    /// 開・閉 2 枚の立ち絵用（Python 版と同じく 10Hz で開閉、ただし無音の間は閉じる）。
    pub fn mouth_open(&self, scene: usize, line: usize, t: f64, duration: f64) -> bool {
        let flap = (t * 10.0).floor() as i64 % 2 == 1;
        match self.tracks.get(&(scene, line)) {
            Some(v) => {
                flap && v
                    .get((t / WINDOW) as usize)
                    .is_some_and(|m| *m != Mouth::Closed)
            }
            None => flap && t < duration - 0.1,
        }
    }
}

/// 目パチ。`frames` 枚の目の画像（開→閉）のうち、時刻 `t` で使う番号を返す。
/// キャラクターごとに決まった間隔（平均 `interval` 秒、±50%）でまばたきする。
pub fn blink_frame(t: f64, frames: usize, interval: f64, seed: u64) -> usize {
    if frames < 2 || interval <= 0.0 {
        return 0;
    }
    let per = 0.04; // 1 枚あたりの秒
    let blink = per * (2 * frames - 1) as f64;
    let mut at = interval * (0.3 + crate::fx::hash01(0, seed) as f64);
    let mut n = 1;
    while at + blink < t {
        at += interval * (0.5 + crate::fx::hash01(n, seed) as f64);
        n += 1;
    }
    if t < at {
        return 0;
    }
    // 開 → 閉 → 開
    let step = ((t - at) / per) as usize;
    let last = frames - 1;
    if step <= last {
        step
    } else {
        (2 * last).saturating_sub(step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2 つのフォルマントを持つ合成母音（パルス列をレゾネータに通す）。
    fn vowel(f1: f64, f2: f64, secs: f64, rate: u32) -> Vec<f32> {
        let n = (secs * f64::from(rate)) as usize;
        let mut out = vec![0.0f64; n];
        let period = (f64::from(rate) / 120.0) as usize;
        for (i, o) in out.iter_mut().enumerate() {
            *o = if i % period == 0 { 1.0 } else { 0.0 };
        }
        for (f, bw) in [(f1, 80.0), (f2, 120.0)] {
            let r = (-std::f64::consts::PI * bw / f64::from(rate)).exp();
            let c = 2.0 * r * (std::f64::consts::TAU * f / f64::from(rate)).cos();
            let (mut y1, mut y2) = (0.0, 0.0);
            for o in out.iter_mut() {
                let y = *o + c * y1 - r * r * y2;
                y2 = y1;
                y1 = y;
                *o = y;
            }
        }
        let peak = out.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        out.iter().map(|v| (v / peak * 0.5) as f32).collect()
    }

    #[test]
    fn recognizes_vowels_and_silence() {
        for (f1, f2, expect) in [
            (800.0, 1250.0, Mouth::A),
            (300.0, 2350.0, Mouth::I),
            (500.0, 850.0, Mouth::O),
        ] {
            let mut s = vec![0.0f32; 4800];
            s.extend(vowel(f1, f2, 0.5, 48_000));
            let track = analyze(&s, 48_000);
            assert_eq!(track[0], Mouth::Closed);
            let mid = &track[4..track.len() - 1];
            let hits = mid.iter().filter(|m| **m == expect).count();
            assert!(hits * 10 >= mid.len() * 7, "{expect:?}: {track:?}");
        }
    }

    #[test]
    fn blink_cycles_open_closed_open() {
        let seq: Vec<usize> = (0..400)
            .map(|i| blink_frame(i as f64 * 0.01, 3, 1.0, 7))
            .collect();
        assert!(seq.contains(&2));
        let first = seq.iter().position(|&f| f != 0).unwrap();
        assert_eq!(
            &seq[first..first + 20]
                .iter()
                .copied()
                .filter(|f| *f > 0)
                .max(),
            &Some(2)
        );
        assert_eq!(blink_frame(1.0, 1, 1.0, 0), 0);
    }
}
