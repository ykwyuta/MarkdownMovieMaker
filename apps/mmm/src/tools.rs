//! Python 版 `script2video/tools` の移植（GIF 作成・跳ねる GIF・バリエーション複製）。
use image::{
    Delay, Frame, RgbaImage,
    codecs::gif::{GifEncoder, Repeat},
};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

fn write_gif(out: &Path, frames: Vec<(RgbaImage, u32)>) -> Result<(), String> {
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let file = File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut enc = GifEncoder::new_with_speed(file, 10);
    enc.set_repeat(Repeat::Infinite)
        .map_err(|e| e.to_string())?;
    for (img, ms) in frames {
        enc.encode_frame(Frame::from_parts(
            img,
            0,
            0,
            Delay::from_numer_denom_ms(ms, 1),
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn open(path: &Path) -> Result<RgbaImage, String> {
    Ok(image::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .into_rgba8())
}

/// `create_gif.py`: CSV の各行の 2 枚を交互に表示する GIF。
pub fn gifs_from_csv(csv: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(csv).map_err(|e| format!("{}: {e}", csv.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or(format!("CSV に列 {name} がありません"))
    };
    let (a, b, o, d) = (
        col("image1_path")?,
        col("image2_path")?,
        col("output_path")?,
        col("duration")?,
    );
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        let get = |i: usize| cells.get(i).copied().unwrap_or_default();
        let ms = (get(d).parse::<f64>().unwrap_or(0.5) * 1000.0) as u32;
        match open(Path::new(get(a))).and_then(|x| Ok((x, open(Path::new(get(b)))?))) {
            Ok((x, y)) => {
                write_gif(Path::new(get(o)), vec![(x, ms), (y, ms)])?;
                println!("{}", get(o));
            }
            Err(e) => eprintln!("警告: {e}"),
        }
    }
    Ok(())
}

/// `make_bouncing_gif.py`: 放物線で 1 回跳ねる GIF（30fps、上に `height` px の余白）。
pub fn bounce_gif(input: &Path, out: &Path, height: u32, duration: f64) -> Result<(), String> {
    let src = open(input)?;
    let frames = ((duration * 30.0).round() as u32).max(2);
    let mut out_frames = Vec::new();
    for i in 0..frames {
        let t = f64::from(i) / f64::from(frames);
        let lift = (f64::from(height) * 4.0 * t * (1.0 - t)).round() as i64;
        let mut canvas = RgbaImage::new(src.width(), src.height() + height);
        image::imageops::overlay(&mut canvas, &src, 0, i64::from(height) - lift);
        out_frames.push((canvas, 1000 / 30));
    }
    write_gif(out, out_frames)?;
    println!("{}", out.display());
    Ok(())
}

#[derive(clap::Args)]
pub struct VariationArgs {
    #[arg(long, alias = "character_id")]
    character_id: String,
    #[arg(long, alias = "base_variation")]
    base_variation: String,
    #[arg(long, alias = "new_variation")]
    new_variation: String,
    /// 画像生成 AI へのプロンプトのヒント（表示のみ）
    #[arg(long, default_value = "")]
    prompt: String,
    #[arg(long, default_value = "data/character_master.json")]
    chars: PathBuf,
    #[arg(long, default_value = "data/character")]
    character_dir: PathBuf,
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

/// `create_variation.py`: 画像を複製してキャラクター定義へ追加する（JSON のキー順は保つ）。
pub fn variation(a: &VariationArgs) -> Result<(), String> {
    let master_path = a.root.join(&a.chars);
    let text = std::fs::read_to_string(&master_path)
        .map_err(|e| format!("{}: {e}", master_path.display()))?;
    let mut master: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let variations = master
        .get_mut(&a.character_id)
        .ok_or(format!("キャラクター {} がありません", a.character_id))?
        .get_mut("variations")
        .and_then(|v| v.as_object_mut())
        .ok_or("variations がありません")?;
    if variations.contains_key(&a.new_variation) {
        return Err(format!("バリエーション {} は既にあります", a.new_variation));
    }
    let mut new = variations
        .get(&a.base_variation)
        .ok_or(format!("バリエーション {} がありません", a.base_variation))?
        .clone();
    let mut copied = Vec::new();
    for key in ["image_open", "image_closed"] {
        let Some(base) = new.get(key).and_then(|v| v.as_str()).map(str::to_owned) else {
            continue;
        };
        let base_path = a.root.join(&base);
        let suffix = base_path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit('_').next())
            .unwrap_or("image.png");
        let rel = a
            .character_dir
            .join(format!("{}_{}_{suffix}", a.character_id, a.new_variation));
        match std::fs::copy(&base_path, a.root.join(&rel)) {
            Ok(_) => {
                new[key] = rel.to_string_lossy().replace('\\', "/").into();
                copied.push(a.root.join(&rel));
            }
            Err(e) => eprintln!("警告: {} を複製できません: {e}", base_path.display()),
        }
    }
    if copied.is_empty() {
        return Err("複製できる画像がありません".into());
    }
    variations.insert(a.new_variation.clone(), new);
    let json = serde_json::to_string_pretty(&master).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::write(&master_path, json + "\n") {
        for f in &copied {
            let _ = std::fs::remove_file(f);
        }
        return Err(format!("{}: {e}", master_path.display()));
    }
    println!(
        "バリエーション {} を {} に追加しました。次の画像を編集してください:",
        a.new_variation, a.character_id
    );
    for f in copied {
        println!("- {}", f.display());
    }
    if !a.prompt.is_empty() {
        println!("プロンプトのヒント: {}", a.prompt);
    }
    Ok(())
}

/// `powershell/ConvertImage.ps1`（`magick -flop`）の置き換え。既に反転済みの名前は飛ばす。
pub fn flip(dir: &Path, suffix: &str, ext: &str) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        })
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| !s.ends_with(suffix))
        })
        .collect();
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let out = path.with_file_name(format!("{stem}{suffix}.{ext}"));
        let img = image::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        img.fliph()
            .save(&out)
            .map_err(|e| format!("{}: {e}", out.display()))?;
        println!("{} -> {}", path.display(), out.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_mirrors_and_skips_existing_variants() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.save(dir.path().join("a.png")).unwrap();
        flip(dir.path(), "_left", "png").unwrap();
        let out = image::open(dir.path().join("a_left.png"))
            .unwrap()
            .into_rgba8();
        assert_eq!(out.get_pixel(1, 0).0, [255, 0, 0, 255]);
        flip(dir.path(), "_left", "png").unwrap();
        assert!(!dir.path().join("a_left_left.png").exists());
    }

    #[test]
    fn bounce_gif_has_extra_headroom() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.png");
        RgbaImage::from_pixel(8, 8, image::Rgba([0, 255, 0, 255]))
            .save(&src)
            .unwrap();
        let out = dir.path().join("b.gif");
        bounce_gif(&src, &out, 4, 0.2).unwrap();
        let gif = image::open(&out).unwrap();
        assert_eq!((gif.width(), gif.height()), (8, 12));
    }
}
