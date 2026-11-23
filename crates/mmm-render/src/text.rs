//! テキストの描画（字幕・テキストアイテム）。ImageMagick / Pillow を使わず ab_glyph で描く。
//!
//! 多重縁取り・影・背景の箱（Python 版の角丸の枠）・太字・斜体・行揃え・文字間隔・行の高さ・
//! 折り返し（1 文字単位）・縦書き・1 文字ずつ表示に対応する。
use crate::fx;
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::RgbaImage;
use mmm_core::{Align, Rgba, SubtitleStyle, TextBox, TextSpec};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub type TextImage = Arc<RgbaImage>;

/// キャッシュの鍵（毎フレーム呼ばれるので文字列化せずにハッシュする）。
fn spec_key(spec: &TextSpec, k: f32, visible: Option<usize>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let f = |h: &mut std::collections::hash_map::DefaultHasher, v: f32| v.to_bits().hash(h);
    spec.text.hash(&mut h);
    spec.font.hash(&mut h);
    spec.color.0.hash(&mut h);
    for v in [
        spec.size,
        spec.spacing,
        spec.line_height,
        k,
        spec.max_width.unwrap_or(-1.0),
    ] {
        f(&mut h, v);
    }
    for (w, c) in &spec.outlines {
        f(&mut h, *w);
        c.0.hash(&mut h);
    }
    if let Some((a, b, c, col)) = spec.shadow {
        for v in [a, b, c] {
            f(&mut h, v);
        }
        col.0.hash(&mut h);
    }
    if let Some(b) = &spec.boxed {
        for v in [b.padding, b.radius, b.border] {
            f(&mut h, v);
        }
        b.fill.0.hash(&mut h);
        b.border_color.0.hash(&mut h);
    }
    (
        spec.bold,
        spec.italic,
        spec.vertical,
        spec.align as u8,
        spec.typewriter.map(f64::to_bits),
        visible,
    )
        .hash(&mut h);
    h.finish()
}

pub struct TextRenderer {
    default: FontArc,
    base: PathBuf,
    fonts: Mutex<HashMap<String, Option<FontArc>>>,
    cache: Mutex<HashMap<u64, TextImage>>,
}

/// 角丸長方形の符号付き距離（内側が負）。
fn round_rect_sdf(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// ストレートアルファ同士の over 合成（被覆率つき）。
fn over_px(dst: &mut [u8; 4], src: Rgba, coverage: f32) {
    let c = coverage.clamp(0.0, 1.0);
    if c <= 0.0 {
        return;
    }
    let top = [
        src.0[0],
        src.0[1],
        src.0[2],
        (f32::from(src.0[3]) * c).round() as u8,
    ];
    *dst = fx::over(*dst, top);
}

/// 字幕スタイル → テキストの装飾。`k` は基準解像度に対する倍率。
pub fn subtitle_spec(style: &SubtitleStyle, text: &str, max_width: f32) -> TextSpec {
    let mut spec = TextSpec {
        text: text.to_owned(),
        size: style.fontsize,
        color: style.color,
        font: style.font.clone(),
        boxed: (!style.no_box).then(|| TextBox {
            fill: style.fill_color,
            padding: style.padding,
            radius: style.corner_radius.max(style.border_width),
            border: style.border_width,
            border_color: style.border_color,
        }),
        line_height: 1.2,
        typewriter: style.typewriter,
        max_width: Some(max_width),
        ..TextSpec::default()
    };
    let mut attrs = mmm_core::attrs::Attrs::default();
    if let Some(o) = &style.outline {
        attrs.0.insert("outline".into(), o.clone());
    }
    if let Some(s) = &style.shadow {
        attrs.0.insert("shadow".into(), s.clone());
    }
    let _ = mmm_core::parser::take_text_style(&mut spec, &mut attrs);
    spec
}

struct Glyph {
    c: char,
    x: f32,
    y: f32,
}

impl TextRenderer {
    pub fn new(font: Option<Vec<u8>>, base: PathBuf) -> Result<Self, crate::Error> {
        let default = match font {
            Some(bytes) => {
                FontArc::try_from_vec(bytes).map_err(|e| crate::Error::Font(e.to_string()))?
            }
            None => FontArc::try_from_slice(crate::DEFAULT_FONT)
                .map_err(|e| crate::Error::Font(e.to_string()))?,
        };
        Ok(Self {
            default,
            base,
            fonts: Mutex::default(),
            cache: Mutex::default(),
        })
    }

    fn font(&self, path: Option<&str>) -> FontArc {
        let Some(path) = path else {
            return self.default.clone();
        };
        let mut fonts = self.fonts.lock().unwrap();
        fonts
            .entry(path.to_owned())
            .or_insert_with(|| {
                let full = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    self.base.join(path)
                };
                std::fs::read(full)
                    .ok()
                    .and_then(|b| FontArc::try_from_vec(b).ok())
            })
            .clone()
            .unwrap_or_else(|| self.default.clone())
    }

    /// Pillow と同じく `px` を em の大きさとして扱う（ab_glyph の PxScale は行の高さ基準）。
    fn scale(font: &FontArc, px: f32) -> PxScale {
        let em = font.units_per_em().unwrap_or(1000.0);
        PxScale::from(px * font.height_unscaled() / em)
    }

    fn advance(font: &FontArc, px: f32, text: &str, spacing: f32) -> f32 {
        let f = font.as_scaled(Self::scale(font, px));
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                w += f.kern(p, id);
            }
            w += f.h_advance(id) + spacing;
            prev = Some(id);
        }
        (w - spacing).max(0.0)
    }

    /// 1 文字ずつ幅を測って折り返す（日本語向け）。
    pub fn wrap(&self, text: &str, px: f32, max_width: f32) -> Vec<String> {
        self.wrap_with(&self.default, text, px, max_width, 0.0)
    }

    fn wrap_with(
        &self,
        font: &FontArc,
        text: &str,
        px: f32,
        max_width: f32,
        spacing: f32,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut current = String::new();
            for c in para.chars() {
                let mut test = current.clone();
                test.push(c);
                if current.is_empty() || Self::advance(font, px, &test, spacing) <= max_width {
                    current = test;
                    continue;
                }
                // 禁則処理（追い出し）: 行頭に来てはいけない文字（、。」など）は、前の文字と一緒に
                // 次の行へ送る。行末に来てはいけない文字（「（など）も次の行へ送る。
                let mut carry: Vec<char> = Vec::new();
                if is_no_line_start(c) {
                    while let Some(last) = current.pop() {
                        carry.insert(0, last);
                        if !is_no_line_start(last) {
                            break;
                        }
                    }
                }
                while current.chars().last().is_some_and(is_no_line_end) {
                    carry.insert(0, current.pop().unwrap());
                }
                if current.is_empty() {
                    // 送ると行が空になる（禁則文字ばかりの行）ときは、そのまま行末にぶら下げる
                    current = carry.into_iter().collect();
                    current.push(c);
                    continue;
                }
                lines.push(std::mem::take(&mut current));
                current.extend(carry);
                current.push(c);
            }
            lines.push(current);
        }
        lines
    }

    /// テキストを描いた画像（中心がテキストの中心）。`visible` 文字だけを描く（1 文字ずつ表示用）。
    pub fn render(&self, spec: &TextSpec, k: f32, visible: Option<usize>) -> TextImage {
        let key = spec_key(spec, k, visible);
        if let Some(img) = self.cache.lock().unwrap().get(&key) {
            return img.clone();
        }
        let img = Arc::new(self.draw(spec, k, visible));
        let mut cache = self.cache.lock().unwrap();
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, img.clone());
        img
    }

    fn layout(&self, font: &FontArc, spec: &TextSpec, k: f32) -> (Vec<Glyph>, f32, f32) {
        let px = spec.size * k;
        let spacing = spec.spacing * k;
        let scaled = font.as_scaled(Self::scale(font, px));
        let line_h = px * spec.line_height;
        let mut glyphs = Vec::new();
        if spec.vertical {
            // 縦書き: 右の列から左へ、各列は上から下へ
            let cols: Vec<&str> = spec.text.split('\n').collect();
            let col_w = line_h;
            let width = col_w * cols.len() as f32;
            let mut height: f32 = 0.0;
            for (ci, col) in cols.iter().enumerate() {
                let x = width - col_w * (ci as f32 + 0.5);
                let mut y = 0.0;
                for c in col.chars() {
                    glyphs.push(Glyph {
                        c,
                        x: x - scaled.h_advance(scaled.glyph_id(c)) / 2.0,
                        y: y + scaled.ascent(),
                    });
                    y += px + spacing;
                }
                height = height.max(y);
            }
            return (glyphs, width, height);
        }
        let lines = match spec.max_width {
            Some(w) => self.wrap_with(font, &spec.text, px, w * k, spacing),
            None => spec.text.split('\n').map(str::to_owned).collect(),
        };
        let widths: Vec<f32> = lines
            .iter()
            .map(|l| Self::advance(font, px, l, spacing))
            .collect();
        let width = widths.iter().copied().fold(0.0, f32::max);
        let height = px
            + line_h * (lines.len().saturating_sub(1)) as f32
            + (scaled.ascent() - scaled.descent() - px).max(0.0);
        for (li, line) in lines.iter().enumerate() {
            let mut x = match spec.align {
                Align::Left => 0.0,
                Align::Center => (width - widths[li]) / 2.0,
                Align::Right => width - widths[li],
            };
            let y = scaled.ascent() + line_h * li as f32;
            let mut prev = None;
            for c in line.chars() {
                let id = scaled.glyph_id(c);
                if let Some(p) = prev {
                    x += scaled.kern(p, id);
                }
                glyphs.push(Glyph { c, x, y });
                x += scaled.h_advance(id) + spacing;
                prev = Some(id);
            }
        }
        (glyphs, width, height)
    }

    fn draw(&self, spec: &TextSpec, k: f32, visible: Option<usize>) -> RgbaImage {
        let font = self.font(spec.font.as_deref());
        let px = spec.size * k;
        let (glyphs, tw, th) = self.layout(&font, spec, k);
        let outline_total: f32 = spec.outlines.iter().map(|o| o.0 * k).sum();
        let bold = if spec.bold {
            (px * 0.035).max(1.0)
        } else {
            0.0
        };
        let shadow_extent = spec
            .shadow
            .map_or(0.0, |(dx, dy, b, _)| (dx.abs().max(dy.abs()) + b * 1.5) * k);
        let (pad, radius, border) = spec.boxed.as_ref().map_or((0.0, 0.0, 0.0), |b| {
            (b.padding * k, b.radius * k, b.border * k)
        });
        let italic_extra = if spec.italic { px * 0.25 } else { 0.0 };
        let inset = (outline_total + bold + shadow_extent).max(0.0) + pad + border + 2.0;
        let w = (tw + italic_extra + 2.0 * inset).ceil().max(1.0) as u32;
        let h = (th + 2.0 * inset).ceil().max(1.0) as u32;

        // 文字の被覆率マスク
        let mut mask = RgbaImage::new(w, h);
        let scale = Self::scale(&font, px);
        for g in glyphs.iter().take(visible.unwrap_or(usize::MAX)) {
            let glyph = font
                .glyph_id(g.c)
                .with_scale_and_position(scale, ab_glyph::point(g.x + inset, g.y + inset));
            if let Some(outline) = font.outline_glyph(glyph) {
                let b = outline.px_bounds();
                let baseline = g.y + inset;
                outline.draw(|gx, gy, cov| {
                    let fy = b.min.y + gy as f32;
                    let shear = if spec.italic {
                        (baseline - fy) * 0.2
                    } else {
                        0.0
                    };
                    let ix = (b.min.x + gx as f32 + shear).round() as i64;
                    let iy = fy as i64;
                    if ix >= 0 && iy >= 0 && (ix as u32) < w && (iy as u32) < h {
                        let p = mask.get_pixel_mut(ix as u32, iy as u32);
                        p.0[3] = p.0[3].max((cov * 255.0).round() as u8);
                    }
                });
            }
        }
        if bold > 0.0 {
            mask = fx::outline_under(&mask, bold, Rgba([0, 0, 0, 255]));
        }
        // 縁取り（外側から）
        let mut text = RgbaImage::new(w, h);
        let mut acc = outline_total;
        for &(ow, color) in spec.outlines.iter().rev() {
            let layer = fx::outline_under(&mask, acc, color);
            for (d, s) in text.pixels_mut().zip(layer.pixels()) {
                if s.0[3] > 0 {
                    // 縁取りの色だけ（文字部分も一度縁の色で塗り、後で本文を重ねる）
                    d.0 = fx::over(d.0, [color.0[0], color.0[1], color.0[2], s.0[3]]);
                }
            }
            acc -= ow * k;
        }
        for (d, m) in text.pixels_mut().zip(mask.pixels()) {
            let a = f32::from(m.0[3]) / 255.0;
            over_px(&mut d.0, spec.color, a);
        }
        // 影
        if let Some((dx, dy, b, color)) = spec.shadow {
            text = fx::apply(
                &text,
                &[mmm_core::Fx::Shadow {
                    dx,
                    dy,
                    blur: b,
                    color,
                }],
                k,
            );
            text = image::imageops::crop_imm(
                &text,
                (text.width() - w) / 2,
                (text.height() - h) / 2,
                w,
                h,
            )
            .to_image();
        }
        // 背景の箱
        let Some(bx) = &spec.boxed else { return text };
        let mut out = RgbaImage::new(w, h);
        let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
        let box_inset = inset - pad - border;
        let (bw, bh) = (hw - box_inset, hh - box_inset);
        let r = radius.max(border).min(bw.min(bh));
        for y in 0..h {
            for x in 0..w {
                let d = round_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, hw, hh, bw, bh, r);
                let outer = (0.5 - d).clamp(0.0, 1.0);
                if outer <= 0.0 {
                    continue;
                }
                let inner = (0.5 - (d + border)).clamp(0.0, 1.0);
                let p = &mut out.get_pixel_mut(x, y).0;
                over_px(p, bx.fill, inner);
                if border > 0.0 {
                    over_px(p, bx.border_color, outer - inner);
                }
            }
        }
        for (d, s) in out.pixels_mut().zip(text.pixels()) {
            d.0 = fx::over(d.0, s.0);
        }
        // 箱の外側の余白を切り詰める
        let m = box_inset.floor().max(0.0) as u32;
        image::imageops::crop_imm(&out, m, m, w - 2 * m, h - 2 * m).to_image()
    }

    /// 文字数（1 文字ずつ表示の計算用）。
    pub fn char_count(spec: &TextSpec) -> usize {
        spec.text.chars().filter(|c| *c != '\n').count()
    }
}

/// 行頭に置かない文字（句読点・閉じ括弧・小書きの仮名・長音など）。
fn is_no_line_start(c: char) -> bool {
    matches!(
        c,
        '、' | '。'
            | '，'
            | '．'
            | '・'
            | '：'
            | '；'
            | '？'
            | '！'
            | '‼'
            | '⁇'
            | '⁈'
            | '⁉'
            | 'ー'
            | '〜'
            | '～'
            | '…'
            | '‥'
            | '々'
            | 'ゝ'
            | 'ゞ'
            | 'ヽ'
            | 'ヾ'
            | '」'
            | '』'
            | '）'
            | '］'
            | '｝'
            | '〉'
            | '》'
            | '】'
            | '〕'
            | '〙'
            | '〛'
            | '”'
            | '’'
            | 'ぁ'
            | 'ぃ'
            | 'ぅ'
            | 'ぇ'
            | 'ぉ'
            | 'っ'
            | 'ゃ'
            | 'ゅ'
            | 'ょ'
            | 'ゎ'
            | 'ゕ'
            | 'ゖ'
            | 'ァ'
            | 'ィ'
            | 'ゥ'
            | 'ェ'
            | 'ォ'
            | 'ッ'
            | 'ャ'
            | 'ュ'
            | 'ョ'
            | 'ヮ'
            | 'ヵ'
            | 'ヶ'
            | ','
            | '.'
            | ':'
            | ';'
            | '?'
            | '!'
            | ')'
            | ']'
            | '}'
            | '｡'
            | '､'
            | '｣'
            | 'ｰ'
    )
}

/// 行末に置かない文字（開き括弧）。
fn is_no_line_end(c: char) -> bool {
    matches!(
        c,
        '「' | '『'
            | '（'
            | '［'
            | '｛'
            | '〈'
            | '《'
            | '【'
            | '〔'
            | '〘'
            | '〚'
            | '“'
            | '‘'
            | '('
            | '['
            | '{'
            | '｢'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> TextRenderer {
        TextRenderer::new(None, PathBuf::from(".")).unwrap()
    }

    #[test]
    fn punctuation_never_starts_a_line() {
        let r = r();
        // 4 文字で折り返す幅。5 文字目の「、」は前の文字と一緒に次の行へ送る
        let w = 70.0 * 4.5;
        assert_eq!(
            r.wrap("あいうえ、かきく", 70.0, w),
            ["あいう", "え、かき", "く"]
        );
        assert_eq!(r.wrap("あいうえ。", 70.0, w), ["あいう", "え。"]);
        // 「。」」のように続く禁則文字もまとめて送る
        assert_eq!(
            r.wrap("あいう」。かき", 70.0, w),
            ["あい", "う」。か", "き"]
        );
        // 開き括弧は行末に残さない
        assert_eq!(r.wrap("あいう「えおか", 70.0, w), ["あいう", "「えおか"]);
        // 送ると空行になる場合はぶら下げる
        assert_eq!(r.wrap("。。。。。", 70.0, w), ["。。。。。"]);
        for line in r.wrap("ソース管理に切り替えると、ギットのブランチや変更が見えて、コミットやプッシュもできるのだ。", 70.0, 70.0 * 23.5) {
            assert!(!line.starts_with(['、', '。']), "{line}");
        }
    }

    #[test]
    fn wraps_japanese_and_draws_subtitle_box() {
        let r = r();
        let lines = r.wrap("あいうえおかきくけこ", 70.0, 70.0 * 4.5);
        assert_eq!(lines, ["あいうえ", "おかきく", "けこ"]);
        let spec = subtitle_spec(&SubtitleStyle::default(), "こんにちは", 1728.0);
        let img = r.render(&spec, 1.0, None);
        assert!(img.width() > 5 * 60 && img.height() > 70);
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
        let mid_y = img.height() / 2;
        assert_eq!(img.get_pixel(1, mid_y).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(10, mid_y).0, [255, 255, 255, 200]);
        assert!(img.pixels().any(|p| p.0[0] < 50 && p.0[3] > 200));
    }

    #[test]
    fn outlines_shadow_typewriter_and_vertical() {
        let r = r();
        let mut spec = TextSpec {
            text: "あい".into(),
            size: 60.0,
            color: Rgba([255, 255, 0, 255]),
            ..TextSpec::default()
        };
        spec.outlines = vec![
            (4.0, Rgba([0, 0, 0, 255])),
            (4.0, Rgba([255, 255, 255, 255])),
        ];
        spec.shadow = Some((5.0, 5.0, 2.0, Rgba([0, 0, 0, 200])));
        let img = r.render(&spec, 1.0, None);
        let count = |c: [u8; 3]| {
            img.pixels()
                .filter(|p| p.0[..3] == c && p.0[3] == 255)
                .count()
        };
        assert!(count([255, 255, 0]) > 100 && count([0, 0, 0]) > 50 && count([255, 255, 255]) > 50);
        let one = r.render(&spec, 1.0, Some(1));
        let ink = |i: &RgbaImage| i.pixels().filter(|p| p.0[3] > 0).count();
        assert!(ink(&one) < ink(&img));
        let v = r.render(
            &TextSpec {
                text: "あいう".into(),
                vertical: true,
                ..TextSpec::default()
            },
            1.0,
            None,
        );
        assert!(v.height() > v.width());
    }
}
