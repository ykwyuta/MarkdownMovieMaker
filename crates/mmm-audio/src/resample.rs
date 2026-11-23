//! 窓付き sinc による帯域制限リサンプラー（フィルタ係数は表引き）。
use crate::AudioBuffer;
use std::f64::consts::PI;

const ZEROS: usize = 16; // 片側のゼロ交差数
const PHASES: usize = 256; // ゼロ交差あたりの表の分解能

struct Kernel {
    table: Vec<f32>,
    cutoff: f64,
}

impl Kernel {
    fn new(cutoff: f64) -> Self {
        let len = ZEROS * PHASES + 1;
        let table = (0..len)
            .map(|i| {
                let x = i as f64 / PHASES as f64; // 0..=ZEROS（ゼロ交差単位）
                let sinc = if x == 0.0 {
                    1.0
                } else {
                    (PI * x).sin() / (PI * x)
                };
                // Blackman 窓
                let w = 0.42
                    + 0.5 * (PI * x / ZEROS as f64).cos()
                    + 0.08 * (2.0 * PI * x / ZEROS as f64).cos();
                (sinc * w * cutoff) as f32
            })
            .collect();
        Self { table, cutoff }
    }

    /// 入力サンプル間隔で測った距離 `d` の係数。
    fn at(&self, d: f64) -> f32 {
        let x = d.abs() * self.cutoff * PHASES as f64;
        let i = x as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let f = (x - i as f64) as f32;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }
}

pub fn resample(buf: &AudioBuffer, rate: u32) -> AudioBuffer {
    if buf.rate == rate || buf.samples.is_empty() {
        let mut out = buf.clone();
        out.rate = rate;
        return out;
    }
    let ch = buf.channels.max(1) as usize;
    let ratio = f64::from(buf.rate) / f64::from(rate);
    let cutoff = (1.0 / ratio).min(1.0);
    let kernel = Kernel::new(cutoff);
    let reach = (ZEROS as f64 / cutoff).ceil() as isize;
    let in_frames = buf.frames() as isize;
    let out_frames = ((buf.frames() as f64) / ratio).round() as usize;
    let mut out = vec![0.0f32; out_frames * ch];
    // BGM など長い音声は区間ごとに並列に計算する（結果は逐次と同じ）
    use rayon::prelude::*;
    out.par_chunks_mut(ch * 4096)
        .enumerate()
        .for_each(|(block, chunk)| {
            for (i, frame) in chunk.chunks_exact_mut(ch).enumerate() {
                let o = block * 4096 + i;
                let p = o as f64 * ratio;
                let center = p.floor() as isize;
                for j in (center - reach + 1).max(0)..=(center + reach).min(in_frames - 1) {
                    let w = kernel.at(p - j as f64);
                    if w != 0.0 {
                        let src = &buf.samples[j as usize * ch..j as usize * ch + ch];
                        for (d, s) in frame.iter_mut().zip(src) {
                            *d += s * w;
                        }
                    }
                }
            }
        });
    AudioBuffer::new(rate, buf.channels, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f64, secs: f64) -> AudioBuffer {
        let n = (f64::from(rate) * secs) as usize;
        AudioBuffer::new(
            rate,
            1,
            (0..n)
                .map(|i| (0.5 * (2.0 * PI * hz * i as f64 / f64::from(rate)).sin()) as f32)
                .collect(),
        )
    }

    #[test]
    fn preserves_tone_between_rates() {
        let src = sine(44_100, 1000.0, 0.5);
        let out = resample(&src, 48_000);
        assert_eq!(out.frames(), 24_000);
        let expect = sine(48_000, 1000.0, 0.5);
        // 端の過渡部を除いて比較
        let err: f32 = out.samples[1000..23_000]
            .iter()
            .zip(&expect.samples[1000..23_000])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(err < 0.01, "max error {err}");
    }

    #[test]
    fn downsampling_removes_content_above_nyquist() {
        let src = sine(48_000, 15_000.0, 0.2);
        let out = resample(&src, 24_000);
        let rms = (out.samples[500..4000].iter().map(|s| s * s).sum::<f32>() / 3500.0).sqrt();
        assert!(rms < 0.01, "aliasing rms {rms}");
    }
}
