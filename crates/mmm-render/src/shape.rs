//! 図形アイテム（四角形・円・三角形・星・線）。符号付き距離でアンチエイリアスして描く。
use crate::fx::over;
use image::RgbaImage;
use mmm_core::{Rgba, ShapeKind, ShapeSpec};

fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay, bax, bay) = (px - ax, py - ay, bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay).max(1e-6)).clamp(0.0, 1.0);
    ((pax - bax * h).powi(2) + (pay - bay * h).powi(2)).sqrt()
}

/// 多角形（頂点は時計回りでも反時計回りでもよい）の符号付き距離。
fn sd_polygon(px: f32, py: f32, pts: &[(f32, f32)]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        d = d.min(sd_segment(px, py, a.0, a.1, b.0, b.1));
        if (a.1 > py) != (b.1 > py) && px < (b.0 - a.0) * (py - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    if inside { -d } else { d }
}

/// 図形を描いた画像（中心が図形の中心）。`k` は基準解像度に対する倍率。
pub fn render(spec: &ShapeSpec, k: f32) -> RgbaImage {
    let stroke = spec.stroke.map_or(0.0, |(w, _)| w * k);
    let (sw, sh) = (spec.width * k, spec.height * k);
    let margin = stroke + 2.0;
    let w = (sw + 2.0 * margin).ceil().max(1.0) as u32;
    let h = (sh + 2.0 * margin).ceil().max(1.0) as u32;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (hw, hh) = (sw / 2.0, sh / 2.0);
    let r = (spec.radius * k).min(hw.min(hh));
    let poly: Vec<(f32, f32)> = match spec.shape {
        ShapeKind::Triangle => vec![(cx, cy - hh), (cx + hw, cy + hh), (cx - hw, cy + hh)],
        ShapeKind::Star => (0..10)
            .map(|i| {
                let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
                let rad = if i % 2 == 0 { 1.0 } else { 0.42 };
                (cx + a.cos() * hw * rad, cy + a.sin() * hh * rad)
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = match spec.shape {
                ShapeKind::Rect => {
                    let qx = (px - cx).abs() - (hw - r);
                    let qy = (py - cy).abs() - (hh - r);
                    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
                }
                ShapeKind::Ellipse => {
                    // 楕円の近似距離（単位円へ正規化した距離に短径を掛ける）
                    let nx = (px - cx) / hw.max(1e-3);
                    let ny = (py - cy) / hh.max(1e-3);
                    ((nx * nx + ny * ny).sqrt() - 1.0) * hw.min(hh)
                }
                ShapeKind::Line => {
                    sd_segment(px, py, cx - hw, cy, cx + hw, cy) - (sh / 2.0).max(1.0)
                }
                ShapeKind::Triangle | ShapeKind::Star => sd_polygon(px, py, &poly),
            };
            let fill_cov = (0.5 - d).clamp(0.0, 1.0);
            let p = img.get_pixel_mut(x, y);
            if let Some(fill) = spec.fill {
                p.0 = over(p.0, with_alpha(fill, fill_cov));
            }
            if let Some((_, color)) = spec.stroke {
                // 輪郭の外側に stroke の太さ
                let cov = ((0.5 - (d - stroke)).clamp(0.0, 1.0) - fill_cov).max(0.0);
                p.0 = over(p.0, with_alpha(color, cov));
            }
        }
    }
    img
}

fn with_alpha(c: Rgba, cov: f32) -> [u8; 4] {
    [
        c.0[0],
        c.0[1],
        c.0[2],
        (f32::from(c.0[3]) * cov).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_fill_and_stroke() {
        let spec = ShapeSpec {
            shape: ShapeKind::Rect,
            width: 100.0,
            height: 50.0,
            fill: Some(Rgba([255, 0, 0, 255])),
            stroke: Some((4.0, Rgba([0, 0, 255, 255]))),
            radius: 10.0,
        };
        let img = render(&spec, 1.0);
        let (cx, cy) = (img.width() / 2, img.height() / 2);
        assert_eq!(img.get_pixel(cx, cy).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(cx + 52, cy).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(1, 1).0[3], 0);
        for shape in [
            ShapeKind::Ellipse,
            ShapeKind::Triangle,
            ShapeKind::Star,
            ShapeKind::Line,
        ] {
            let img = render(
                &ShapeSpec {
                    shape,
                    stroke: None,
                    ..spec.clone()
                },
                1.0,
            );
            assert!(img.pixels().any(|p| p.0[3] == 255), "{shape:?}");
        }
    }
}
