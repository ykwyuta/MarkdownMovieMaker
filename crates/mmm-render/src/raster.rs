//! アフィン変形つきの描画（拡大・回転・反転・不透明度・合成モード・ワイプの切り抜き）。
use image::RgbaImage;
use mmm_core::Blend;

/// 描画する位置と変形。
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// 出力画像上の中心座標（ピクセル）。
    pub cx: f32,
    pub cy: f32,
    /// 元画像 1 ピクセルあたりの出力ピクセル数。
    pub sx: f32,
    pub sy: f32,
    /// 回転（度、時計回り）。
    pub rotate: f32,
    pub opacity: f32,
    pub blend: Blend,
    /// 元画像の正規化座標（0〜1）で見える範囲 `[x0, y0, x1, y1]`（ワイプ用）。
    pub clip: [f32; 4],
}

impl Placement {
    pub fn at(cx: f32, cy: f32) -> Self {
        Self {
            cx,
            cy,
            sx: 1.0,
            sy: 1.0,
            rotate: 0.0,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

#[inline]
fn blend_channel(mode: Blend, s: f32, d: f32) -> f32 {
    match mode {
        Blend::Normal => s,
        Blend::Add => (s + d).min(1.0),
        Blend::Multiply => s * d,
        Blend::Screen => 1.0 - (1.0 - s) * (1.0 - d),
        Blend::Overlay => {
            if d < 0.5 {
                2.0 * s * d
            } else {
                1.0 - 2.0 * (1.0 - s) * (1.0 - d)
            }
        }
        Blend::Lighten => s.max(d),
        Blend::Darken => s.min(d),
    }
}

/// 等倍の重ね合わせ（左右反転・不透明度・切り抜きに対応）。中心は最も近い画素に揃える。
fn blit(canvas: &mut RgbaImage, src: &RgbaImage, p: &Placement) {
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let (cw, ch) = (canvas.width() as i64, canvas.height() as i64);
    let left = (p.cx - sw as f32 / 2.0).round() as i64;
    let top = (p.cy - sh as f32 / 2.0).round() as i64;
    // 切り抜き（元画像の画素の範囲）
    let [u0, v0, u1, v1] = p.clip;
    let (cu0, cu1) = (
        (u0 * sw as f32).ceil() as i64,
        (u1 * sw as f32).floor() as i64,
    );
    let (cv0, cv1) = (
        (v0 * sh as f32).ceil() as i64,
        (v1 * sh as f32).floor() as i64,
    );
    let x0 = left.max(0).max(left + cu0);
    let x1 = (left + sw).min(cw).min(left + cu1);
    let y0 = top.max(0).max(top + cv0);
    let y1 = (top + sh).min(ch).min(top + cv1);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let flip = p.sx < 0.0;
    let opacity = (p.opacity.clamp(0.0, 1.0) * 256.0).round() as u32;
    let buf = src.as_raw();
    let out: &mut [u8] = canvas;
    let n = (x1 - x0) as usize;
    for y in y0..y1 {
        let srow = ((y - top) * sw) as usize * 4;
        let drow = (y * cw + x0) as usize * 4;
        let dst = &mut out[drow..drow + n * 4];
        if !flip && opacity >= 256 {
            let sx0 = (x0 - left) as usize;
            let row = &buf[srow + sx0 * 4..srow + (sx0 + n) * 4];
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(row.as_chunks::<4>().0)
            {
                let a = u32::from(s[3]);
                if a == 255 {
                    d[..3].copy_from_slice(&s[..3]);
                } else if a != 0 {
                    for c in 0..3 {
                        d[c] =
                            (mul255(u32::from(s[c]), a) + mul255(u32::from(d[c]), 255 - a)) as u8;
                    }
                }
            }
            continue;
        }
        for (i, d) in dst.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let sx = (x0 - left) as usize + i;
            let sx = if flip { sw as usize - 1 - sx } else { sx };
            let s = &buf[srow + sx * 4..srow + sx * 4 + 4];
            let a = (u32::from(s[3]) * opacity) >> 8;
            if a >= 255 {
                d[..3].copy_from_slice(&s[..3]);
            } else if a != 0 {
                for c in 0..3 {
                    d[c] = (mul255(u32::from(s[c]), a) + mul255(u32::from(d[c]), 255 - a)) as u8;
                }
            }
        }
    }
}

/// 8bit 同士の掛け算を 255 で割る（丸めつき）。
#[inline]
fn mul255(a: u32, b: u32) -> u32 {
    let x = a * b + 128;
    (x + (x >> 8)) >> 8
}

/// `src` をバイリニア補間で `p` のとおりに不透明なキャンバスへ重ねる。
///
/// 逆変換の座標は行ごとに増分で求め、16.16 の固定小数点で補間する。4 画素とも不透明な
/// ところ（画像の大半）はアルファを扱わない近道を通る。
pub fn draw(canvas: &mut RgbaImage, src: &RgbaImage, p: &Placement) {
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    if sw == 0.0 || sh == 0.0 || p.opacity <= 0.0 || p.sx == 0.0 || p.sy == 0.0 {
        return;
    }
    // 等倍・回転なし（立ち絵・字幕・背景の大半）は補間せずに行ごとに重ねる
    if p.rotate == 0.0 && p.sx.abs() == 1.0 && p.sy == 1.0 && p.blend == Blend::Normal {
        blit(canvas, src, p);
        return;
    }
    let (cw, ch) = (canvas.width() as i64, canvas.height() as i64);
    let (sin, cos) = p.rotate.to_radians().sin_cos();
    let hw = sw * p.sx.abs() / 2.0;
    let hh = sh * p.sy.abs() / 2.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
        let rx = p.cx + x * cos - y * sin;
        let ry = p.cy + x * sin + y * cos;
        x0 = x0.min(rx);
        y0 = y0.min(ry);
        x1 = x1.max(rx);
        y1 = y1.max(ry);
    }
    let (x0, y0) = ((x0.floor() as i64).max(0), (y0.floor() as i64).max(0));
    let (x1, y1) = ((x1.ceil() as i64).min(cw), (y1.ceil() as i64).min(ch));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let buf = src.as_raw();
    let stride = src.width() as usize * 4;
    let (iw, ih) = (src.width() as i64, src.height() as i64);
    let opacity = (p.opacity.clamp(0.0, 1.0) * 256.0).round() as u32; // 0..=256
    let [u0, v0, u1, v1] = p.clip;
    // 切り抜き（元画像のピクセル座標、16.16）
    const ONE: i64 = 1 << 16;
    let (cu0, cv0) = ((u0 * sw * ONE as f32) as i64, (v0 * sh * ONE as f32) as i64);
    let (cu1, cv1) = ((u1 * sw * ONE as f32) as i64, (v1 * sh * ONE as f32) as i64);
    // 出力 1 ピクセル右へ進んだときの元画像座標の増分
    let dux = (cos / p.sx * ONE as f32) as i64;
    let dvx = (-sin / p.sy * ONE as f32) as i64;
    let normal = p.blend == Blend::Normal;
    let out: &mut [u8] = canvas;
    let rows = &mut out[(y0 * cw) as usize * 4..(y1 * cw) as usize * 4];
    let line = |(i, out): (usize, &mut [u8])| {
        let y = y0 + i as i64;
        let dy = y as f32 + 0.5 - p.cy;
        let dx = x0 as f32 + 0.5 - p.cx;
        // 元画像の座標（画素の中心が整数になるよう 0.5 引く）
        let mut u = (((dx * cos + dy * sin) / p.sx + sw / 2.0) * ONE as f32) as i64;
        let mut v = (((-dx * sin + dy * cos) / p.sy + sh / 2.0) * ONE as f32) as i64;
        for x in x0..x1 {
            let (cu, cv) = (u, v);
            u += dux;
            v += dvx;
            if cu < cu0.max(0)
                || cv < cv0.max(0)
                || cu >= cu1.min(iw * ONE)
                || cv >= cv1.min(ih * ONE)
            {
                continue;
            }
            let fu = cu - ONE / 2;
            let fv = cv - ONE / 2;
            let ix = fu >> 16;
            let iy = fv >> 16;
            let ax = ((fu & 0xffff) >> 8) as u32; // 0..255
            let ay = ((fv & 0xffff) >> 8) as u32;
            let xa = ix.clamp(0, iw - 1) as usize * 4;
            let xb = (ix + 1).clamp(0, iw - 1) as usize * 4;
            let ya = iy.clamp(0, ih - 1) as usize * stride;
            let yb = (iy + 1).clamp(0, ih - 1) as usize * stride;
            let (p00, p10, p01, p11) = (ya + xa, ya + xb, yb + xa, yb + xb);
            let w00 = (256 - ax) * (256 - ay);
            let w10 = ax * (256 - ay);
            let w01 = (256 - ax) * ay;
            let w11 = ax * ay; // 合計 65536
            let (a00, a10, a01, a11) = (
                buf[p00 + 3] as u32,
                buf[p10 + 3] as u32,
                buf[p01 + 3] as u32,
                buf[p11 + 3] as u32,
            );
            let (mut rgb, alpha) = if a00 & a10 & a01 & a11 == 255 {
                let c = |o: usize| {
                    (buf[p00 + o] as u32 * w00
                        + buf[p10 + o] as u32 * w10
                        + buf[p01 + o] as u32 * w01
                        + buf[p11 + o] as u32 * w11
                        + 32768)
                        >> 16
                };
                ([c(0), c(1), c(2)], 255)
            } else {
                // プリマルチプライドで補間して縁のにじみを防ぐ
                let aw = [a00 * w00, a10 * w10, a01 * w01, a11 * w11];
                let at = aw[0] + aw[1] + aw[2] + aw[3];
                if at == 0 {
                    continue;
                }
                let c = |o: usize| {
                    let s = buf[p00 + o] as u64 * aw[0] as u64
                        + buf[p10 + o] as u64 * aw[1] as u64
                        + buf[p01 + o] as u64 * aw[2] as u64
                        + buf[p11 + o] as u64 * aw[3] as u64;
                    (s / at as u64) as u32
                };
                ([c(0), c(1), c(2)], (at + 32768) >> 16)
            };
            let a = (alpha * opacity) >> 8;
            if a == 0 {
                continue;
            }
            let o = x as usize * 4;
            if !normal {
                for (c, s) in rgb.iter_mut().enumerate() {
                    let d = out[o + c] as f32 / 255.0;
                    *s = (blend_channel(p.blend, *s as f32 / 255.0, d) * 255.0).round() as u32;
                }
            }
            if a >= 255 {
                out[o] = rgb[0] as u8;
                out[o + 1] = rgb[1] as u8;
                out[o + 2] = rgb[2] as u8;
            } else {
                for c in 0..3 {
                    out[o + c] = (mul255(rgb[c], a) + mul255(out[o + c] as u32, 255 - a)) as u8;
                }
            }
        }
    };
    // 全画面に近い大きさ（カメラ・背景のアニメーション）は行ごとに並列に描く
    let stride_out = cw as usize * 4;
    if (x1 - x0) * (y1 - y0) >= 400_000 {
        use rayon::prelude::*;
        rows.par_chunks_mut(stride_out).enumerate().for_each(line);
    } else {
        rows.chunks_mut(stride_out).enumerate().for_each(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn scales_rotates_and_blends() {
        let src = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let mut c = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 255, 255]));
        draw(
            &mut c,
            &src,
            &Placement {
                sx: 2.0,
                sy: 2.0,
                ..Placement::at(20.0, 20.0)
            },
        );
        assert_eq!(c.get_pixel(11, 20).0, [255, 0, 0, 255]);
        assert_eq!(c.get_pixel(8, 20).0, [0, 0, 255, 255]);
        // 45 度回すと角が外へ出る
        let mut r = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        draw(
            &mut r,
            &src,
            &Placement {
                rotate: 45.0,
                ..Placement::at(20.0, 20.0)
            },
        );
        assert_eq!(r.get_pixel(20, 13).0[0], 255);
        assert_eq!(r.get_pixel(14, 14).0[0], 0);
        // 加算
        let mut a = RgbaImage::from_pixel(10, 10, Rgba([100, 100, 100, 255]));
        let gray = RgbaImage::from_pixel(10, 10, Rgba([100, 100, 100, 255]));
        draw(
            &mut a,
            &gray,
            &Placement {
                blend: Blend::Add,
                ..Placement::at(5.0, 5.0)
            },
        );
        assert_eq!(a.get_pixel(5, 5).0[0], 200);
        // 等倍の近道: 反転・半透明・はみ出し
        let mut f = RgbaImage::from_pixel(4, 1, Rgba([0, 0, 0, 255]));
        let mut grad = RgbaImage::new(2, 1);
        grad.put_pixel(0, 0, Rgba([10, 0, 0, 255]));
        grad.put_pixel(1, 0, Rgba([20, 0, 0, 255]));
        draw(
            &mut f,
            &grad,
            &Placement {
                sx: -1.0,
                ..Placement::at(1.0, 0.5)
            },
        );
        assert_eq!((f.get_pixel(0, 0).0[0], f.get_pixel(1, 0).0[0]), (20, 10));
        draw(
            &mut f,
            &grad,
            &Placement {
                opacity: 0.5,
                ..Placement::at(4.0, 0.5)
            },
        );
        assert_eq!(f.get_pixel(3, 0).0[0], 5);
        // 左半分だけ見せる（ワイプ）
        let mut w = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255]));
        draw(
            &mut w,
            &src,
            &Placement {
                clip: [0.0, 0.0, 0.5, 1.0],
                ..Placement::at(5.0, 5.0)
            },
        );
        assert_eq!((w.get_pixel(2, 5).0[0], w.get_pixel(8, 5).0[0]), (255, 0));
    }
}
