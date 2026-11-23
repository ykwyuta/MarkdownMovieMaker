//! MarkdownMovieMaker のアイコンを作る（yyeditor の `tools/gen-icon` と同じ作り方を Rust で）。
//!
//! ```text
//! cargo run -p mmm-gen-icon
//! ```
//!
//! 角丸の四角（マゼンタのグラデーション）に、白の「M↓」（Markdown のマーク）と、その下にフィルムの帯
//! （映画）を描く。どのサイズ（16 px も）でも同じ図形を描き、色だけの違いにしない。文字は図形で描くので
//! フォントに依存しない。各サイズを 4 倍で描いて縮小する。256 px は PNG、それ以外は 32 bit BMP で ICO に入れる。
use image::{ImageEncoder, RgbaImage, codecs::png::PngEncoder};
use std::path::Path;

const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const SCALE: u32 = 4;
const TOP: [f32; 3] = [0xD9 as f32, 0x46 as f32, 0xEF as f32];
const BOTTOM: [f32; 3] = [0x86 as f32, 0x19 as f32, 0x8F as f32];

/// 図形（0〜1 の座標）。
enum Shape {
    Polygon(Vec<(f32, f32)>),
    RoundRect {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        r: f32,
    },
}

impl Shape {
    fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Self::Polygon(pts) => {
                let mut inside = false;
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
                        inside = !inside;
                    }
                }
                inside
            }
            Self::RoundRect { x0, y0, x1, y1, r } => {
                let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                let (hw, hh) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
                let qx = (x - cx).abs() - (hw - r);
                let qy = (y - cy).abs() - (hh - r);
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r <= 0.0
            }
        }
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Shape {
    Shape::Polygon(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// 白で描く図形と、地の色でくり抜く図形。小さいサイズでは線を太くする。
fn design(size: u32) -> (Vec<Shape>, Vec<Shape>) {
    let small = size <= 24;
    let t = if small { 0.13 } else { 0.105 }; // 線の太さ
    // 「M」
    let (mx0, mx1, my0, my1) = (0.14, 0.56, 0.15, 0.55);
    let mid = (mx0 + mx1) / 2.0;
    // 外側の輪郭を 1 つの多角形で描く（内側の V は外側と平行）
    let (v, d) = (my0 + 0.25, 0.13);
    let mut white = vec![Shape::Polygon(vec![
        (mx0, my1),
        (mx0, my0),
        (mx0 + t * 1.15, my0),
        (mid, v),
        (mx1 - t * 1.15, my0),
        (mx1, my0),
        (mx1, my1),
        (mx1 - t, my1),
        (mx1 - t, my0 + d),
        (mid, v + t * 1.25),
        (mx0 + t, my0 + d),
        (mx0 + t, my1),
    ])];
    // 「↓」
    let (ax, aw) = (0.74, if small { 0.13 } else { 0.1 });
    white.push(rect(ax - aw / 2.0, my0, ax + aw / 2.0, my1 - 0.12));
    white.push(Shape::Polygon(vec![
        (ax - 0.15, my1 - 0.16),
        (ax + 0.15, my1 - 0.16),
        (ax, my1 + 0.01),
    ]));
    // フィルムの帯と穴
    let (fy0, fy1) = (0.66, if small { 0.88 } else { 0.85 });
    white.push(Shape::RoundRect {
        x0: 0.12,
        y0: fy0,
        x1: 0.88,
        y1: fy1,
        r: 0.03,
    });
    let mut holes = Vec::new();
    let n = if small { 3 } else { 5 };
    let (hx0, hx1) = (0.18, 0.82);
    let hw = (hx1 - hx0) / (n as f32 * 2.0 - 1.0);
    for i in 0..n {
        let x = hx0 + hw * 2.0 * i as f32;
        let (hy0, hy1) = (fy0 + (fy1 - fy0) * 0.3, fy1 - (fy1 - fy0) * 0.3);
        holes.push(Shape::RoundRect {
            x0: x,
            y0: hy0,
            x1: x + hw,
            y1: hy1,
            r: hw * 0.2,
        });
    }
    (white, holes)
}

fn draw(size: u32) -> RgbaImage {
    let big = size * SCALE;
    let (white, holes) = design(size);
    let background = Shape::RoundRect {
        x0: 0.03,
        y0: 0.03,
        x1: 0.97,
        y1: 0.97,
        r: 0.19,
    };
    // 4 倍で描いて、4×4 の平均（プリマルチプライド）で縮小する
    let mut acc = vec![[0.0f32; 4]; (size * size) as usize];
    for py in 0..big {
        for px in 0..big {
            let (x, y) = (
                (px as f32 + 0.5) / big as f32,
                (py as f32 + 0.5) / big as f32,
            );
            if !background.contains(x, y) {
                continue;
            }
            let ground = [0, 1, 2].map(|c| TOP[c] + (BOTTOM[c] - TOP[c]) * y);
            let on =
                white.iter().any(|s| s.contains(x, y)) && !holes.iter().any(|s| s.contains(x, y));
            let c = if on { [255.0; 3] } else { ground };
            let a = &mut acc[((py / SCALE) * size + px / SCALE) as usize];
            a[0] += c[0];
            a[1] += c[1];
            a[2] += c[2];
            a[3] += 1.0;
        }
    }
    let n = (SCALE * SCALE) as f32;
    RgbaImage::from_fn(size, size, |x, y| {
        let a = acc[(y * size + x) as usize];
        if a[3] == 0.0 {
            return image::Rgba([0, 0, 0, 0]);
        }
        image::Rgba([
            (a[0] / a[3]).round() as u8,
            (a[1] / a[3]).round() as u8,
            (a[2] / a[3]).round() as u8,
            (a[3] / n * 255.0).round() as u8,
        ])
    })
}

fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("png");
    out
}

/// ICO 用の 32 bit BMP（BITMAPINFOHEADER、下から上、BGRA と 1 bit の AND マスク）。
fn bmp(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    for v in [40u32, w, h * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    let row = w.div_ceil(32) * 4;
    out.extend(std::iter::repeat_n(0u8, (row * h) as usize));
    out
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/markdown-movie-maker/res");
    std::fs::create_dir_all(&dir).expect("res");
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&s| {
            let img = draw(s);
            if s == 256 {
                std::fs::write(dir.join("MarkdownMovieMaker-256.png"), png(&img)).expect("png");
            }
            (s, if s >= 256 { png(&img) } else { bmp(&img) })
        })
        .collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (s, data) in &images {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write(dir.join("MarkdownMovieMaker.ico"), ico).expect("ico");
    println!("{}", dir.join("MarkdownMovieMaker.ico").display());
}
