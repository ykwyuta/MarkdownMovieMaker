//! 映像エフェクト。
//!
//! - 画像を作り替えるもの（ぼかし・影・発光・縁取り・モザイク・色調補正・クロマキー）: [`apply`]
//! - 位置・角度・大きさ・不透明度を時間で動かすもの（振動・跳ねる・浮遊など）: [`motion`]
//!
//! 影・発光・縁取りは画像の周りに余白を足すが、中心は変えない（配置の計算をそのまま使える）。
use image::RgbaImage;
use mmm_core::{Fx, Rgba};

/// 余白を足す（中心は変わらない）。
fn pad(img: &RgbaImage, m: u32) -> RgbaImage {
    if m == 0 {
        return img.clone();
    }
    let mut out = RgbaImage::new(img.width() + 2 * m, img.height() + 2 * m);
    image::imageops::replace(&mut out, img, i64::from(m), i64::from(m));
    out
}

/// プリマルチプライドの箱型ぼかしを 3 回（ガウスに近い）。
pub fn blur(img: &RgbaImage, radius: f32) -> RgbaImage {
    let r = (radius / 1.7).round() as usize;
    if r == 0 {
        return img.clone();
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut buf: Vec<[f32; 4]> = img
        .pixels()
        .map(|p| {
            let a = p.0[3] as f32 / 255.0;
            [
                p.0[0] as f32 * a,
                p.0[1] as f32 * a,
                p.0[2] as f32 * a,
                p.0[3] as f32,
            ]
        })
        .collect();
    let mut tmp = vec![[0.0f32; 4]; w * h];
    for _ in 0..3 {
        box_pass(&buf, &mut tmp, w, h, r, true);
        box_pass(&tmp, &mut buf, w, h, r, false);
    }
    let mut out = RgbaImage::new(w as u32, h as u32);
    for (p, v) in out.pixels_mut().zip(&buf) {
        let a = v[3];
        if a > 0.01 {
            let k = 255.0 / a;
            p.0 = [
                (v[0] * k).min(255.0) as u8,
                (v[1] * k).min(255.0) as u8,
                (v[2] * k).min(255.0) as u8,
                a.round().min(255.0) as u8,
            ];
        }
    }
    out
}

fn box_pass(
    src: &[[f32; 4]],
    dst: &mut [[f32; 4]],
    w: usize,
    h: usize,
    r: usize,
    horizontal: bool,
) {
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut acc = [0.0f32; 4];
        let get = |i: isize| -> [f32; 4] {
            if i < 0 || i >= n as isize {
                [0.0; 4]
            } else {
                src[idx(line, i as usize)]
            }
        };
        for i in -(r as isize)..=(r as isize) {
            let v = get(i);
            for c in 0..4 {
                acc[c] += v[c];
            }
        }
        for i in 0..n {
            dst[idx(line, i)] = [acc[0] * norm, acc[1] * norm, acc[2] * norm, acc[3] * norm];
            let (add, sub) = (
                get(i as isize + r as isize + 1),
                get(i as isize - r as isize),
            );
            for c in 0..4 {
                acc[c] += add[c] - sub[c];
            }
        }
    }
}

/// 不透明な部分からのユークリッド距離（Felzenszwalb の 2 乗距離変換）。
fn distance_field(img: &RgbaImage) -> Vec<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    const INF: f32 = 1e20;
    let mut f: Vec<f32> = img
        .pixels()
        .map(|p| if p.0[3] >= 128 { 0.0 } else { INF })
        .collect();
    let mut col = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    let mut v = vec![0usize; w.max(h)];
    let mut z = vec![0.0f32; w.max(h) + 1];
    let mut edt1 = |data: &mut [f32], n: usize, stride: usize, offset: usize| {
        for i in 0..n {
            col[i] = data[offset + i * stride];
        }
        let mut k = 0;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        for q in 1..n {
            let mut s;
            loop {
                let p = v[k];
                s = ((col[q] + (q * q) as f32) - (col[p] + (p * p) as f32))
                    / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                } else {
                    break;
                }
            }
            if s <= z[k] {
                // k == 0 で前の放物線が完全に隠れる
                v[0] = q;
                z[0] = -INF;
                z[1] = INF;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate().take(n) {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let d = q as f32 - v[k] as f32;
            *o = d * d + col[v[k]];
        }
        for i in 0..n {
            data[offset + i * stride] = out[i];
        }
    };
    for x in 0..w {
        edt1(&mut f, h, w, x);
    }
    for y in 0..h {
        edt1(&mut f, w, 1, y * w);
    }
    f.into_iter().map(f32::sqrt).collect()
}

/// 縁取り（文字・立ち絵の外側に `width` の線）。`img` は既に余白を持っていること。
pub fn outline_under(img: &RgbaImage, width: f32, color: Rgba) -> RgbaImage {
    let dist = distance_field(img);
    let mut out = RgbaImage::new(img.width(), img.height());
    for (i, (p, s)) in out.pixels_mut().zip(img.pixels()).enumerate() {
        let cov = (width - dist[i] + 0.5).clamp(0.0, 1.0);
        let base = [
            color.0[0],
            color.0[1],
            color.0[2],
            (f32::from(color.0[3]) * cov) as u8,
        ];
        p.0 = over(base, s.0);
    }
    out
}

/// ストレートアルファ同士の over（`top` を `bottom` の上に）。
pub fn over(bottom: [u8; 4], top: [u8; 4]) -> [u8; 4] {
    let ta = f32::from(top[3]) / 255.0;
    if ta >= 1.0 {
        return top;
    }
    let ba = f32::from(bottom[3]) / 255.0;
    let oa = ta + ba * (1.0 - ta);
    if oa <= 0.0 {
        return [0; 4];
    }
    let mut out = [0u8; 4];
    for c in 0..3 {
        out[c] =
            ((f32::from(top[c]) * ta + f32::from(bottom[c]) * ba * (1.0 - ta)) / oa).round() as u8;
    }
    out[3] = (oa * 255.0).round() as u8;
    out
}

/// `shape` の形の影（色 `color`、ぼかし `radius`、ずれ `dx, dy`）を `img` の下に敷く。
fn shadow(img: &RgbaImage, dx: f32, dy: f32, radius: f32, color: Rgba) -> RgbaImage {
    let mut sil = RgbaImage::new(img.width(), img.height());
    for (d, s) in sil.pixels_mut().zip(img.pixels()) {
        d.0 = [
            color.0[0],
            color.0[1],
            color.0[2],
            ((u32::from(s.0[3]) * u32::from(color.0[3])) / 255) as u8,
        ];
    }
    let sil = blur(&sil, radius);
    let mut out = RgbaImage::new(img.width(), img.height());
    image::imageops::overlay(&mut out, &sil, dx.round() as i64, dy.round() as i64);
    for (d, s) in out.pixels_mut().zip(img.pixels()) {
        d.0 = over(d.0, s.0);
    }
    out
}

fn color_filter(img: &mut RgbaImage, f: impl Fn([f32; 3]) -> [f32; 3]) {
    for p in img.pixels_mut() {
        let c = f([
            f32::from(p.0[0]) / 255.0,
            f32::from(p.0[1]) / 255.0,
            f32::from(p.0[2]) / 255.0,
        ]);
        for (dst, v) in p.0.iter_mut().zip(c) {
            *dst = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

fn hue_rotate([r, g, b]: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    let m = [
        [
            0.213 + c * 0.787 - s * 0.213,
            0.715 - c * 0.715 - s * 0.715,
            0.072 - c * 0.072 + s * 0.928,
        ],
        [
            0.213 - c * 0.213 + s * 0.143,
            0.715 + c * 0.285 + s * 0.140,
            0.072 - c * 0.072 - s * 0.283,
        ],
        [
            0.213 - c * 0.213 - s * 0.787,
            0.715 - c * 0.715 + s * 0.715,
            0.072 + c * 0.928 + s * 0.072,
        ],
    ];
    [0, 1, 2].map(|i| m[i][0] * r + m[i][1] * g + m[i][2] * b)
}

/// 静的なエフェクトを順に適用する。`k` は基準解像度に対する倍率（ぼかし幅などに掛ける）。
pub fn apply(img: &RgbaImage, list: &[Fx], k: f32) -> RgbaImage {
    let mut img = img.clone();
    for fx in list {
        img = match *fx {
            Fx::Blur(r) => {
                let m = (r * k * 1.5).ceil() as u32;
                blur(&pad(&img, m), r * k)
            }
            Fx::Shadow {
                dx,
                dy,
                blur: r,
                color,
            } => {
                let m = ((dx.abs().max(dy.abs()) + r * 1.5) * k).ceil() as u32;
                shadow(&pad(&img, m), dx * k, dy * k, r * k, color)
            }
            Fx::Glow { radius, color } => {
                let m = (radius * k * 1.5).ceil() as u32;
                let padded = pad(&img, m);
                let mut glow = shadow(&padded, 0.0, 0.0, radius * k, color);
                // 2 回重ねて光を強める
                let again = shadow(&padded, 0.0, 0.0, radius * k * 0.5, color);
                for (d, s) in glow.pixels_mut().zip(again.pixels()) {
                    d.0 = over(d.0, s.0);
                }
                glow
            }
            Fx::Outline { width, color } => {
                let m = (width * k).ceil() as u32 + 2;
                outline_under(&pad(&img, m), width * k, color)
            }
            Fx::Mosaic(size) => {
                let b = ((size as f32 * k).round() as u32).max(1);
                let (w, h) = img.dimensions();
                let mut out = img.clone();
                for by in (0..h).step_by(b as usize) {
                    for bx in (0..w).step_by(b as usize) {
                        let p = *img.get_pixel((bx + b / 2).min(w - 1), (by + b / 2).min(h - 1));
                        for y in by..(by + b).min(h) {
                            for x in bx..(bx + b).min(w) {
                                out.put_pixel(x, y, p);
                            }
                        }
                    }
                }
                out
            }
            Fx::Grayscale => {
                color_filter(&mut img, |[r, g, b]| [0.299 * r + 0.587 * g + 0.114 * b; 3]);
                img
            }
            Fx::Sepia => {
                color_filter(&mut img, |[r, g, b]| {
                    [
                        0.393 * r + 0.769 * g + 0.189 * b,
                        0.349 * r + 0.686 * g + 0.168 * b,
                        0.272 * r + 0.534 * g + 0.131 * b,
                    ]
                });
                img
            }
            Fx::Negative => {
                color_filter(&mut img, |c| c.map(|v| 1.0 - v));
                img
            }
            Fx::Brightness(v) => {
                color_filter(&mut img, |c| c.map(|x| x * v));
                img
            }
            Fx::Contrast(v) => {
                color_filter(&mut img, |c| c.map(|x| (x - 0.5) * v + 0.5));
                img
            }
            Fx::Saturation(v) => {
                color_filter(&mut img, |[r, g, b]| {
                    let l = 0.299 * r + 0.587 * g + 0.114 * b;
                    [l + (r - l) * v, l + (g - l) * v, l + (b - l) * v]
                });
                img
            }
            Fx::Hue(deg) => {
                color_filter(&mut img, |c| hue_rotate(c, deg));
                img
            }
            Fx::Tint(c) => {
                let t = [c.0[0], c.0[1], c.0[2]].map(|v| f32::from(v) / 255.0);
                color_filter(&mut img, |p| [p[0] * t[0], p[1] * t[1], p[2] * t[2]]);
                img
            }
            Fx::Chroma { color, tolerance } => {
                let key = [color.0[0], color.0[1], color.0[2]].map(f32::from);
                let tol = tolerance.clamp(0.0, 1.0) * 441.7; // RGB 空間の対角
                let soft = tol * 0.25 + 1.0;
                for p in img.pixels_mut() {
                    let d = ((f32::from(p.0[0]) - key[0]).powi(2)
                        + (f32::from(p.0[1]) - key[1]).powi(2)
                        + (f32::from(p.0[2]) - key[2]).powi(2))
                    .sqrt();
                    let keep = ((d - tol) / soft).clamp(0.0, 1.0);
                    p.0[3] = (f32::from(p.0[3]) * keep) as u8;
                }
                img
            }
            _ => img, // 時間で動くエフェクトは motion で扱う
        };
    }
    img
}

/// 時間で動くエフェクトによる変形の差分。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub dx: f32,
    pub dy: f32,
    pub rotate: f32,
    pub scale: f32,
    pub opacity: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            rotate: 0.0,
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

/// 決まった乱数（フレームと種から）。
pub fn hash01(a: u64, b: u64) -> f32 {
    let mut h =
        0xcbf2_9ce4_8422_2325u64 ^ a.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ b.rotate_left(17);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    (h & 0xffff) as f32 / 65535.0
}

/// 動きのエフェクトを時刻 `t`（アイテム内の秒）で評価する（基準解像度のピクセル）。
pub fn motion(list: &[Fx], t: f64, seed: u64) -> Motion {
    let mut m = Motion::default();
    let tau = std::f64::consts::TAU;
    for fx in list {
        match *fx {
            Fx::Shake { amount, speed } => {
                let step = (t * f64::from(speed.max(1.0))) as u64;
                m.dx += (hash01(step, seed) * 2.0 - 1.0) * amount;
                m.dy += (hash01(step, seed ^ 0x5555) * 2.0 - 1.0) * amount;
            }
            Fx::Bounce { height, speed } => {
                m.dy -= height * (t * f64::from(speed)).sin().abs() as f32
            }
            Fx::Float { amount, period } => {
                m.dy += amount * (t * tau / f64::from(period.max(0.01))).sin() as f32
            }
            Fx::Sway { degrees, period } => {
                m.rotate += degrees * (t * tau / f64::from(period.max(0.01))).sin() as f32
            }
            Fx::Spin(dps) => m.rotate += (t * f64::from(dps)) as f32,
            Fx::Pulse { amount, period } => {
                m.scale *= 1.0
                    + amount * (0.5 - 0.5 * (t * tau / f64::from(period.max(0.01))).cos()) as f32
            }
            Fx::Blink { period } if (t / f64::from(period.max(0.01))) as u64 % 2 == 1 => {
                m.opacity = 0.0;
            }
            _ => {}
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba as Px;

    #[test]
    fn blur_spreads_and_outline_surrounds() {
        let mut img = RgbaImage::new(21, 21);
        img.put_pixel(10, 10, Px([255, 255, 255, 255]));
        let b = blur(&img, 4.0);
        assert!(b.get_pixel(12, 10).0[3] > 0 && b.get_pixel(10, 10).0[3] < 255);
        let mut dot = RgbaImage::new(21, 21);
        for y in 9..12 {
            for x in 9..12 {
                dot.put_pixel(x, y, Px([255, 0, 0, 255]));
            }
        }
        let o = apply(
            &dot,
            &[Fx::Outline {
                width: 3.0,
                color: Rgba([0, 0, 255, 255]),
            }],
            1.0,
        );
        let c = o.width() / 2;
        assert_eq!(o.get_pixel(c, c).0, [255, 0, 0, 255]);
        assert_eq!(o.get_pixel(c + 3, c).0, [0, 0, 255, 255]);
        assert_eq!(o.get_pixel(c + 6, c).0[3], 0);
        let s = apply(
            &dot,
            &[Fx::Shadow {
                dx: 4.0,
                dy: 4.0,
                blur: 0.0,
                color: Rgba([0, 0, 0, 255]),
            }],
            1.0,
        );
        let c = s.width() / 2;
        assert_eq!(s.get_pixel(c + 3, c + 3).0, [0, 0, 0, 255]);
    }

    #[test]
    fn color_filters_and_chroma_key() {
        let img = RgbaImage::from_pixel(2, 1, Px([0, 255, 0, 255]));
        let k = apply(
            &img,
            &[Fx::Chroma {
                color: Rgba([0, 255, 0, 255]),
                tolerance: 0.2,
            }],
            1.0,
        );
        assert_eq!(k.get_pixel(0, 0).0[3], 0);
        let g = apply(
            &RgbaImage::from_pixel(1, 1, Px([255, 0, 0, 255])),
            &[Fx::Grayscale],
            1.0,
        );
        assert_eq!(g.get_pixel(0, 0).0[0], g.get_pixel(0, 0).0[1]);
        let n = apply(
            &RgbaImage::from_pixel(1, 1, Px([0, 0, 0, 255])),
            &[Fx::Negative],
            1.0,
        );
        assert_eq!(n.get_pixel(0, 0).0, [255, 255, 255, 255]);
    }

    #[test]
    fn motions_are_deterministic() {
        let m = motion(
            &[Fx::Bounce {
                height: 20.0,
                speed: 5.0,
            }],
            std::f64::consts::PI / 10.0,
            0,
        );
        assert!((m.dy + 20.0).abs() < 1e-3);
        let s1 = motion(
            &[Fx::Shake {
                amount: 8.0,
                speed: 30.0,
            }],
            1.0,
            7,
        );
        assert_eq!(
            s1,
            motion(
                &[Fx::Shake {
                    amount: 8.0,
                    speed: 30.0
                }],
                1.0,
                7
            )
        );
        assert_eq!(motion(&[Fx::Blink { period: 0.5 }], 0.6, 0).opacity, 0.0);
    }
}
