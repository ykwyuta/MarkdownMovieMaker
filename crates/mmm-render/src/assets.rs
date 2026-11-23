//! 画像の読み込みと、拡大縮小済み画像のキャッシュ。アニメーション GIF に対応する。
use image::{AnimationDecoder, RgbaImage, codecs::gif::GifDecoder, imageops::FilterType};
use std::{
    collections::{HashMap, HashSet},
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

struct Animation {
    frames: Vec<Arc<RgbaImage>>,
    /// 各フレームの終了時刻（秒、累積）。静止画は空。
    ends: Vec<f64>,
}

impl Animation {
    fn index(&self, t: f64) -> usize {
        let Some(total) = self.ends.last().copied().filter(|d| *d > 0.0) else {
            return 0;
        };
        let t = t.rem_euclid(total);
        self.ends
            .partition_point(|e| *e <= t)
            .min(self.frames.len() - 1)
    }
}

type Key = (String, u32, usize);
/// 読めなかった画像は `None` として覚える。
type Cached = Option<Arc<RgbaImage>>;

pub struct Assets {
    base: PathBuf,
    sources: Mutex<HashMap<String, Option<Arc<Animation>>>>,
    scaled: Mutex<HashMap<Key, Arc<RgbaImage>>>,
    /// 黒の上に置いた（不透明にした）背景。
    backgrounds: Mutex<HashMap<(String, u32, u32), Cached>>,
    missing: Mutex<HashSet<String>>,
}

fn load(path: &Path) -> Result<Animation, String> {
    let is_gif = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    if is_gif {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let decoder = GifDecoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
        let mut frames = Vec::new();
        let mut ends = Vec::new();
        let mut acc = 0.0;
        for f in decoder.into_frames() {
            let f = f.map_err(|e| e.to_string())?;
            let (n, d) = f.delay().numer_denom_ms();
            acc += f64::from(n) / f64::from(d.max(1)) / 1000.0;
            ends.push(acc);
            frames.push(Arc::new(f.into_buffer()));
        }
        if frames.is_empty() {
            return Err("GIF にフレームがありません".into());
        }
        return Ok(Animation { frames, ends });
    }
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let img = if is_svg {
        load_svg(path)?
    } else {
        image::open(path).map_err(|e| e.to_string())?.into_rgba8()
    };
    Ok(Animation {
        frames: vec![Arc::new(img)],
        ends: Vec::new(),
    })
}

/// SVG の文字に使うフォント。埋め込みの M+ 1 Bold と OS のフォントに加え、
/// SVG のあるフォルダーから上へたどって最初に見つかった `data/fonts` も読む。
fn svg_fonts(path: &Path) -> Arc<resvg::usvg::fontdb::Database> {
    static CACHE: OnceLock<Mutex<HashMap<Option<PathBuf>, Arc<resvg::usvg::fontdb::Database>>>> =
        OnceLock::new();
    let dir = path
        .ancestors()
        .skip(1)
        .map(|a| a.join("data").join("fonts"))
        .find(|d| d.is_dir());
    let mut cache = CACHE.get_or_init(Mutex::default).lock().unwrap();
    cache
        .entry(dir.clone())
        .or_insert_with(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            if let Some(d) = &dir {
                db.load_fonts_dir(d);
            }
            db.load_font_data(crate::DEFAULT_FONT.to_vec());
            db.set_sans_serif_family("M PLUS 1");
            Arc::new(db)
        })
        .clone()
}

/// SVG を元の大きさ（`width` / `height`、なければ `viewBox`）で RGBA に描く。
fn load_svg(path: &Path) -> Result<RgbaImage, String> {
    use resvg::{tiny_skia, usvg};
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let opt = usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        font_family: "M PLUS 1".into(),
        fontdb: svg_fonts(path),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(&data, &opt).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| "SVG の大きさが 0 です".to_string())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    // tiny-skia は乗算済みアルファなので、ふつうの RGBA に戻す
    let mut rgba = pixmap.take();
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    RgbaImage::from_raw(size.width(), size.height(), rgba)
        .ok_or_else(|| "SVG の描画結果が不正です".to_string())
}

impl Assets {
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            sources: Mutex::default(),
            scaled: Mutex::default(),
            backgrounds: Mutex::default(),
            missing: Mutex::default(),
        }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    /// 読めなかった画像の一覧（警告表示用）。
    pub fn missing(&self) -> Vec<String> {
        let mut v: Vec<_> = self.missing.lock().unwrap().iter().cloned().collect();
        v.sort();
        v
    }

    /// 読めなかったファイルを記録する（動画など画像以外）。
    pub fn report_missing(&self, path: &str, error: &str) {
        self.missing
            .lock()
            .unwrap()
            .insert(format!("{path}: {error}"));
    }

    /// ファイルの更新を反映するためにキャッシュを捨てる。
    pub fn clear(&self) {
        self.sources.lock().unwrap().clear();
        self.scaled.lock().unwrap().clear();
        self.backgrounds.lock().unwrap().clear();
        self.missing.lock().unwrap().clear();
    }

    fn source(&self, path: &str) -> Option<Arc<Animation>> {
        if let Some(a) = self.sources.lock().unwrap().get(path) {
            return a.clone();
        }
        let loaded = match load(&self.base.join(path)) {
            Ok(a) => Some(Arc::new(a)),
            Err(e) => {
                self.missing.lock().unwrap().insert(format!("{path}: {e}"));
                None
            }
        };
        self.sources
            .lock()
            .unwrap()
            .insert(path.to_owned(), loaded.clone());
        loaded
    }

    fn scaled(
        &self,
        path: &str,
        key_size: u32,
        t: f64,
        size: impl Fn(&RgbaImage) -> (u32, u32),
    ) -> Option<Arc<RgbaImage>> {
        let anim = self.source(path)?;
        let index = anim.index(t);
        let key = (path.to_owned(), key_size, index);
        if let Some(img) = self.scaled.lock().unwrap().get(&key) {
            return Some(img.clone());
        }
        let src = &anim.frames[index];
        let (w, h) = size(src);
        let img = if (w, h) == src.dimensions() {
            src.clone()
        } else {
            Arc::new(image::imageops::resize(
                &**src,
                w.max(1),
                h.max(1),
                FilterType::Triangle,
            ))
        };
        self.scaled.lock().unwrap().insert(key, img.clone());
        Some(img)
    }

    /// 出力サイズへ引き伸ばした背景。
    pub fn background(&self, path: &str, w: u32, h: u32) -> Option<Arc<RgbaImage>> {
        let key = (path.to_owned(), w, h);
        if let Some(hit) = self.backgrounds.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let flat = self.scaled(path, w << 16 | h, 0.0, |_| (w, h)).map(|img| {
            // 背景に透過があっても黒の上に置いた結果にする（判定は最初の 1 回だけ）
            if img.pixels().all(|p| p.0[3] == 255) {
                img
            } else {
                let mut flat = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
                image::imageops::overlay(&mut flat, &*img, 0, 0);
                Arc::new(flat)
            }
        });
        self.backgrounds.lock().unwrap().insert(key, flat.clone());
        flat
    }

    /// 高さを `height` にした立ち絵（縦横比は保つ）。
    pub fn sprite(&self, path: &str, height: u32, t: f64) -> Option<Arc<RgbaImage>> {
        self.scaled(path, height, t, |src| {
            let w =
                (src.width() as f64 * height as f64 / src.height().max(1) as f64).round() as u32;
            (w, height)
        })
    }

    /// 元画像の大きさ。
    pub fn size(&self, path: &str) -> Option<(u32, u32)> {
        self.source(path).map(|a| a.frames[0].dimensions())
    }

    /// アニメーション GIF の 1 周の長さ（静止画は 0）。
    pub fn loop_length(&self, path: &str) -> f64 {
        self.source(path)
            .and_then(|a| a.ends.last().copied())
            .unwrap_or(0.0)
    }

    /// 元の大きさの `k` 倍の画像（オーバーレイ）。
    pub fn image(&self, path: &str, k: f32, t: f64) -> Option<Arc<RgbaImage>> {
        let key = 0x8000_0000 | (k * 10000.0) as u32;
        self.scaled(path, key, t, |src| {
            (
                (src.width() as f32 * k).round() as u32,
                (src.height() as f32 * k).round() as u32,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_is_rendered_at_its_own_size() {
        let dir = std::env::temp_dir().join(format!("mmm-svg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.svg");
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
<rect width="20" height="20" fill="#ff0000"/>
<rect x="20" width="20" height="20" fill="#0000ff" fill-opacity="0.5"/>
<text x="2" y="16" font-size="14">あ</text></svg>"##,
        )
        .unwrap();
        let img = load(&path).unwrap();
        let f = &img.frames[0];
        assert_eq!(f.dimensions(), (40, 20));
        assert_eq!(f.get_pixel(18, 2).0, [255, 0, 0, 255]);
        // 乗算済みアルファから戻した色
        let p = f.get_pixel(38, 2).0;
        assert_eq!((p[0], p[2]), (0, 255));
        assert!((126..=129).contains(&p[3]), "{p:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
