//! WSOLA による音程を変えない速度変更（速度指定のないエンジン用）。
use crate::AudioBuffer;

/// `speed` 倍の速さにする（2.0 で長さ半分）。モノラル化して処理する。
pub fn time_stretch(buf: &AudioBuffer, speed: f64) -> AudioBuffer {
    if (speed - 1.0).abs() < 1e-3 || buf.samples.is_empty() {
        return buf.clone();
    }
    let speed = speed.clamp(0.25, 4.0);
    let x = buf.to_mono().samples;
    let rate = f64::from(buf.rate);
    let n = ((0.040 * rate) as usize).max(64); // 窓長 40ms
    let hs = n / 2; // 合成ホップ
    let ha = hs as f64 * speed; // 分析ホップ
    let tol = (0.010 * rate) as isize; // 探索幅 ±10ms
    let window: Vec<f32> = (0..n)
        .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()) as f32)
        .collect();
    let out_len = (x.len() as f64 / speed) as usize + n;
    let mut out = vec![0.0f32; out_len];
    let mut norm = vec![0.0f32; out_len];
    let get = |i: isize| {
        if i >= 0 && (i as usize) < x.len() {
            x[i as usize]
        } else {
            0.0
        }
    };
    let mut prev: isize = 0; // 直前に使った入力位置
    let mut k = 0usize;
    loop {
        let out_pos = k * hs;
        if out_pos >= out_len.saturating_sub(n) {
            break;
        }
        let ideal = (k as f64 * ha) as isize;
        if ideal as usize >= x.len() + n {
            break;
        }
        let pos = if k == 0 {
            0
        } else {
            // 自然な続き（prev + hs）と最も相関の高い位置を探す
            let natural = prev + hs as isize;
            let mut best = (f32::MIN, ideal);
            for delta in (-tol..=tol).step_by(2) {
                let cand = ideal + delta;
                let mut c = 0.0f32;
                for i in (0..hs as isize).step_by(4) {
                    c += get(cand + i) * get(natural + i);
                }
                if c > best.0 {
                    best = (c, cand);
                }
            }
            best.1
        };
        for (i, w) in window.iter().enumerate() {
            out[out_pos + i] += get(pos + i as isize) * w;
            norm[out_pos + i] += w;
        }
        prev = pos;
        k += 1;
    }
    let target = (x.len() as f64 / speed) as usize;
    out.truncate(target);
    for (o, w) in out.iter_mut().zip(&norm) {
        if *w > 1e-3 {
            *o /= w;
        }
    }
    AudioBuffer::new(buf.rate, 1, out)
}

/// 長さを変えずに声の高さを `ratio` 倍にする（1.059 で半音上げ）。WSOLA で伸ばしてから再標本化する。
pub fn pitch_shift(buf: &AudioBuffer, ratio: f64) -> AudioBuffer {
    if (ratio - 1.0).abs() < 1e-3 || buf.samples.is_empty() {
        return buf.clone();
    }
    let ratio = ratio.clamp(0.5, 2.0);
    // 長さを ratio 倍にしてから、ratio 倍の速さで再生する（= 元の長さで高さだけ変わる）
    let longer = time_stretch(buf, 1.0 / ratio);
    let virtual_rate = (f64::from(buf.rate) * ratio).round() as u32;
    let mut out =
        crate::resample::resample(&AudioBuffer::new(virtual_rate, 1, longer.samples), buf.rate);
    out.rate = buf.rate;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_length_but_keeps_pitch() {
        let rate = 24_000;
        let hz = 220.0;
        let src = AudioBuffer::new(
            rate,
            1,
            (0..rate as usize)
                .map(|i| {
                    (0.5 * (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin()) as f32
                })
                .collect(),
        );
        let out = time_stretch(&src, 1.25);
        assert_eq!(out.frames(), 19_200);
        // ゼロ交差の数で周波数を推定
        let mid = &out.samples[2000..17_000];
        let crossings = mid.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let est = crossings as f64 / (mid.len() as f64 / f64::from(rate));
        assert!((est - hz).abs() < 8.0, "pitch {est}");
        // 高さだけ 1.5 倍
        let up = pitch_shift(&src, 1.5);
        assert!(
            (up.frames() as i64 - src.frames() as i64).abs() < 600,
            "{}",
            up.frames()
        );
        let mid = &up.samples[2000..20_000];
        let crossings = mid.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let est = crossings as f64 / (mid.len() as f64 / f64::from(rate));
        assert!((est - hz * 1.5).abs() < 12.0, "pitch {est}");
    }
}
