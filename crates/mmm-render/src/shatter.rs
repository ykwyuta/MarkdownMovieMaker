//! ガラスのように割れて飛び散る演出（`out: shatter`・`- transition: shatter`）。
//!
//! 画像を揺らした格子で三角形の破片に分け、まずひびが入り（破片の間に隙間が開く）、
//! 次に中心から外へ飛び散りながら回転し、重力で落ちて消える。破片の形と動きは
//! 種（アイテムごとに固定）で決まるので、プレビューと書き出しで同じ絵になる。
use crate::{fx::hash01, raster::Placement};
use image::RgbaImage;

const COLS: usize = 7;
const ROWS: usize = 5;

struct Shard {
    /// 元画像のピクセル座標の三角形。
    tri: [(f32, f32); 3],
    centroid: (f32, f32),
    /// 飛ぶ向き（単位ベクトル）と速さ（画像の対角線に対する割合）。
    dir: (f32, f32),
    speed: f32,
    spin: f32,
    /// 飛び始めの遅れ（中心ほど早く割れる）。
    delay: f32,
}

fn shards(w: f32, h: f32, seed: u64) -> Vec<Shard> {
    // 内側の格子点を揺らす（外周は画像の縁に揃える）
    let point = |i: usize, j: usize| -> (f32, f32) {
        let (mut x, mut y) = (w * i as f32 / COLS as f32, h * j as f32 / ROWS as f32);
        if i > 0 && i < COLS {
            x += (hash01((i * 31 + j) as u64, seed) - 0.5) * w / COLS as f32 * 0.8;
        }
        if j > 0 && j < ROWS {
            y += (hash01((i * 17 + j * 7 + 999) as u64, seed) - 0.5) * h / ROWS as f32 * 0.8;
        }
        (x, y)
    };
    let (cx, cy) = (w / 2.0, h / 2.0);
    let diag = (w * w + h * h).sqrt().max(1.0);
    let mut out = Vec::with_capacity(COLS * ROWS * 2);
    for j in 0..ROWS {
        for i in 0..COLS {
            let (a, b, c, d) = (
                point(i, j),
                point(i + 1, j),
                point(i + 1, j + 1),
                point(i, j + 1),
            );
            // 対角線の向きをばらつかせる
            let flip = hash01((i * 7 + j * 13) as u64, seed ^ 0xabc) > 0.5;
            let tris = if flip {
                [[a, b, c], [a, c, d]]
            } else {
                [[a, b, d], [b, c, d]]
            };
            for (n, tri) in tris.into_iter().enumerate() {
                let centroid = (
                    (tri[0].0 + tri[1].0 + tri[2].0) / 3.0,
                    (tri[0].1 + tri[1].1 + tri[2].1) / 3.0,
                );
                let (dx, dy) = (centroid.0 - cx, centroid.1 - cy);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                let k = (out.len() * 3 + n) as u64;
                let wobble = (hash01(k, seed ^ 0x51) - 0.5) * 0.8;
                let (s, c) = wobble.sin_cos();
                let dir = (
                    (dx / len) * c - (dy / len) * s,
                    (dx / len) * s + (dy / len) * c,
                );
                out.push(Shard {
                    tri,
                    centroid,
                    dir,
                    speed: 0.35 + hash01(k, seed ^ 0x77) * 0.5,
                    spin: (hash01(k, seed ^ 0x99) - 0.5) * 540.0,
                    delay: 0.15 * len / (diag / 2.0),
                });
            }
        }
    }
    out
}

fn sample(src: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let (w, h) = (src.width() as i64, src.height() as i64);
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
    let (ax, ay) = (fx - ix as f32, fy - iy as f32);
    let mut acc = [0.0f32; 4];
    for (oy, wy) in [(0, 1.0 - ay), (1, ay)] {
        for (ox, wx) in [(0, 1.0 - ax), (1, ax)] {
            let p = src
                .get_pixel(
                    (ix + ox).clamp(0, w - 1) as u32,
                    (iy + oy).clamp(0, h - 1) as u32,
                )
                .0;
            let a = f32::from(p[3]) * wx * wy;
            acc[0] += f32::from(p[0]) * a;
            acc[1] += f32::from(p[1]) * a;
            acc[2] += f32::from(p[2]) * a;
            acc[3] += a;
        }
    }
    if acc[3] > 0.0 {
        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]]
    } else {
        [0.0; 4]
    }
}

fn inside(p: (f32, f32), t: &[(f32, f32); 3]) -> bool {
    let s = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
    let (d0, d1, d2) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
    !((d0 < 0.0 || d1 < 0.0 || d2 < 0.0) && (d0 > 0.0 || d1 > 0.0 || d2 > 0.0))
}

/// `src` を `base` の配置で、割れ具合 `r`（0 で無傷、1 で飛び散って消える）で描く。
pub fn draw(canvas: &mut RgbaImage, src: &RgbaImage, base: &Placement, r: f32, seed: u64) {
    let r = r.clamp(0.0, 1.0);
    if r >= 1.0 || base.opacity <= 0.0 {
        return;
    }
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let (cw, ch) = (canvas.width() as i64, canvas.height() as i64);
    let diag_out = ((sw * base.sx).powi(2) + (sh * base.sy).powi(2)).sqrt();
    let (bs, bc) = base.rotate.to_radians().sin_cos();
    // 出力座標 = 中心 + 回転(拡大(元画像の中心からのずれ))
    let forward = |x: f32, y: f32| -> (f32, f32) {
        let (dx, dy) = ((x - sw / 2.0) * base.sx, (y - sh / 2.0) * base.sy);
        (base.cx + dx * bc - dy * bs, base.cy + dx * bs + dy * bc)
    };
    // 最初の 15% はひびが入るだけ（破片の間に細い隙間）
    let crack = (r / 0.15).min(1.0);
    let fly = ((r - 0.15) / 0.85).max(0.0);
    let out: &mut [u8] = canvas;
    for s in shards(sw, sh, seed) {
        let t = ((fly - s.delay) / (1.0 - s.delay)).clamp(0.0, 1.0);
        let shrink = 1.0 - 0.06 * crack;
        let (ox, oy) = forward(s.centroid.0, s.centroid.1);
        let dist = s.speed * diag_out * t;
        let gravity = 0.9 * diag_out * t * t;
        let (px, py) = (ox + s.dir.0 * dist, oy + s.dir.1 * dist + gravity);
        let spin = (s.spin * t).to_radians();
        let (ss, sc) = spin.sin_cos();
        let alpha = base.opacity * (1.0 - ((t - 0.55) / 0.45).clamp(0.0, 1.0));
        if alpha <= 0.0 {
            continue;
        }
        // 破片の頂点を出力へ（破片の重心まわりに縮小・回転してから全体の配置へ）
        let place = |x: f32, y: f32| -> (f32, f32) {
            let (lx, ly) = (
                (x - s.centroid.0) * shrink * base.sx,
                (y - s.centroid.1) * shrink * base.sy,
            );
            let (rx, ry) = (lx * sc - ly * ss, lx * ss + ly * sc);
            (px + rx * bc - ry * bs, py + rx * bs + ry * bc)
        };
        let v: Vec<(f32, f32)> = s.tri.iter().map(|p| place(p.0, p.1)).collect();
        let x0 = v
            .iter()
            .map(|p| p.0)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as i64;
        let x1 = (v.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i64).min(cw);
        let y0 = v
            .iter()
            .map(|p| p.1)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as i64;
        let y1 = (v.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i64).min(ch);
        let tri = [v[0], v[1], v[2]];
        for y in y0..y1 {
            for x in x0..x1 {
                let q = (x as f32 + 0.5, y as f32 + 0.5);
                if !inside(q, &tri) {
                    continue;
                }
                // 逆変換で元画像の座標へ
                let (dx, dy) = (q.0 - px, q.1 - py);
                let (ux, uy) = (dx * bc + dy * bs, -dx * bs + dy * bc);
                let (lx, ly) = (ux * sc + uy * ss, -ux * ss + uy * sc);
                let sx = s.centroid.0 + lx / (shrink * base.sx);
                let sy = s.centroid.1 + ly / (shrink * base.sy);
                let p = sample(src, sx, sy);
                let a = p[3] / 255.0 * alpha;
                if a <= 0.0 {
                    continue;
                }
                let o = (y * cw + x) as usize * 4;
                for c in 0..3 {
                    let d = f32::from(out[o + c]);
                    out[o + c] = (p[c] * a + d * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shards_tile_the_image_and_fly_away() {
        let s = shards(140.0, 100.0, 1);
        let area: f32 = s
            .iter()
            .map(|s| {
                let [a, b, c] = s.tri;
                ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() / 2.0
            })
            .sum();
        assert!((area - 14_000.0).abs() < 1.0, "{area}");

        let src = RgbaImage::from_pixel(100, 60, image::Rgba([255, 0, 0, 255]));
        let count = |r: f32| {
            let mut c = RgbaImage::from_pixel(300, 200, image::Rgba([0, 0, 0, 255]));
            draw(&mut c, &src, &Placement::at(150.0, 100.0), r, 3);
            c.pixels().filter(|p| p.0[0] > 128).count()
        };
        let intact = count(0.0);
        assert!(intact > 5500, "{intact}");
        // ひびの段階では少し減る（隙間）だけ、飛び散ると減り、最後は消える
        let cracked = count(0.14);
        assert!(cracked < intact && cracked > intact * 8 / 10, "{cracked}");
        assert!(count(0.97) < intact / 3);
        assert_eq!(count(1.0), 0);
    }
}
