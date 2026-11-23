//! 場面切り替え（YMM4 の「場面切り替え」相当）。前のシーンの最後の絵と新しいシーンを混ぜる。
use image::RgbaImage;
use mmm_core::TransitionKind;

fn mix(a: &RgbaImage, b: &RgbaImage, f: impl Fn(u32, u32) -> f32) -> RgbaImage {
    let mut out = b.clone();
    let w = out.width() as usize;
    let (src, dst): (&[u8], &mut [u8]) = (a.as_raw(), &mut out);
    for (i, (o, s)) in dst
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
        .enumerate()
    {
        let t = (f((i % w) as u32, (i / w) as u32).clamp(0.0, 1.0) * 256.0) as u32;
        if t >= 256 {
            continue;
        }
        for c in 0..3 {
            o[c] = ((u32::from(s[c]) * (256 - t) + u32::from(o[c]) * t) >> 8) as u8;
        }
    }
    out
}

fn shifted(img: &RgbaImage, dx: i64, dy: i64) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(img.width(), img.height(), image::Rgba([0, 0, 0, 255]));
    image::imageops::replace(&mut out, img, dx, dy);
    out
}

fn mosaic(img: &RgbaImage, block: u32) -> RgbaImage {
    crate::fx::apply(img, &[mmm_core::Fx::Mosaic(block.max(1))], 1.0)
}

/// `p` は 0（前のシーン）→ 1（新しいシーン）。
pub fn blend(prev: &RgbaImage, cur: &RgbaImage, kind: TransitionKind, p: f32, k: f32) -> RgbaImage {
    let (w, h) = (cur.width() as f32, cur.height() as f32);
    let soft = (24.0 * k).max(1.0); // ワイプの境目のぼかし幅
    // 境目が画面の外から外へ動くよう、ぼかし幅のぶん広げる
    let sweep = |len: f32| p * (len + 2.0 * soft) - soft;
    match kind {
        TransitionKind::Fade => mix(prev, cur, |_, _| p),
        TransitionKind::Black | TransitionKind::White => {
            let v = if kind == TransitionKind::Black {
                0
            } else {
                255
            };
            let solid =
                RgbaImage::from_pixel(cur.width(), cur.height(), image::Rgba([v, v, v, 255]));
            if p < 0.5 {
                mix(prev, &solid, |_, _| p * 2.0)
            } else {
                mix(&solid, cur, |_, _| p * 2.0 - 1.0)
            }
        }
        TransitionKind::WipeLeft => mix(prev, cur, |x, _| (sweep(w) - x as f32) / soft + 0.5),
        TransitionKind::WipeRight => {
            mix(prev, cur, |x, _| (x as f32 - (w - sweep(w))) / soft + 0.5)
        }
        TransitionKind::WipeUp => mix(prev, cur, |_, y| (y as f32 - (h - sweep(h))) / soft + 0.5),
        TransitionKind::WipeDown => mix(prev, cur, |_, y| (sweep(h) - y as f32) / soft + 0.5),
        TransitionKind::Circle => {
            let r = sweep((w * w + h * h).sqrt() / 2.0);
            mix(prev, cur, |x, y| {
                let d = ((x as f32 - w / 2.0).powi(2) + (y as f32 - h / 2.0).powi(2)).sqrt();
                (r - d) / soft + 0.5
            })
        }
        TransitionKind::SlideLeft
        | TransitionKind::SlideRight
        | TransitionKind::SlideUp
        | TransitionKind::SlideDown => {
            let (dx, dy) = match kind {
                TransitionKind::SlideLeft => (-(w * p) as i64, 0),
                TransitionKind::SlideRight => ((w * p) as i64, 0),
                TransitionKind::SlideUp => (0, -(h * p) as i64),
                _ => (0, (h * p) as i64),
            };
            // 前の絵が押し出され、新しい絵が反対側から入る
            let mut out = shifted(prev, dx, dy);
            let (nx, ny) = match kind {
                TransitionKind::SlideLeft => (dx + w as i64, 0),
                TransitionKind::SlideRight => (dx - w as i64, 0),
                TransitionKind::SlideUp => (0, dy + h as i64),
                _ => (0, dy - h as i64),
            };
            image::imageops::replace(&mut out, cur, nx, ny);
            out
        }
        TransitionKind::Zoom => {
            // 前の絵を拡大しながら消し、新しい絵を重ねる
            let s = 1.0 + p;
            let mut zoomed =
                RgbaImage::from_pixel(cur.width(), cur.height(), image::Rgba([0, 0, 0, 255]));
            let pl = crate::raster::Placement {
                sx: s,
                sy: s,
                ..crate::raster::Placement::at(w / 2.0, h / 2.0)
            };
            crate::raster::draw(&mut zoomed, prev, &pl);
            mix(&zoomed, cur, |_, _| p)
        }
        TransitionKind::Shatter => {
            // 前の絵が割れて飛び散り、後ろから新しいシーンが現れる
            let mut out = cur.clone();
            let base = crate::raster::Placement::at(w / 2.0, h / 2.0);
            crate::shatter::draw(&mut out, prev, &base, p, 0x5ea7);
            out
        }
        TransitionKind::Mosaic => {
            let peak = (40.0 * k) as u32;
            let block = ((1.0 - (2.0 * p - 1.0).abs()) * peak as f32) as u32;
            if p < 0.5 {
                mosaic(prev, block)
            } else {
                mosaic(cur, block)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_start_at_prev_and_end_at_current() {
        let a = RgbaImage::from_pixel(32, 18, image::Rgba([255, 0, 0, 255]));
        let b = RgbaImage::from_pixel(32, 18, image::Rgba([0, 0, 255, 255]));
        use TransitionKind::*;
        for kind in [
            Fade, Black, White, WipeLeft, WipeRight, WipeUp, WipeDown, SlideLeft, SlideRight,
            SlideUp, SlideDown, Circle, Zoom, Mosaic,
        ] {
            let end = blend(&a, &b, kind, 1.0, 0.1);
            assert_eq!(end.get_pixel(16, 9).0, [0, 0, 255, 255], "{kind:?} end");
            if kind != Zoom {
                let start = blend(&a, &b, kind, 0.0, 0.1);
                assert_eq!(start.get_pixel(16, 9).0, [255, 0, 0, 255], "{kind:?} start");
            }
        }
    }
}
