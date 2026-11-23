//! MP4 書き出し。ffmpeg には依存しない。
//!
//! - [`Backend::Software`]（全 OS）: 純 Rust の rusty_h264 で H.264、rusty-opus で音声、muxide で MP4 へ
//!   多重化。GOP ごとに独立したエンコーダを使い、描画とエンコードを CPU コア数だけ並列に回す
//!   （[`encode_parallel`]）。
//! - [`Backend::MediaFoundation`]（Windows）: OS 標準の H.264/AAC エンコーダ。GPU のハードウェア
//!   エンコーダ（NVIDIA は NVENC）が自動で使われ、AAC なので互換性も高い。
use image::RgbaImage;
use std::path::Path;

#[cfg(windows)]
mod mf;
#[cfg(feature = "software")]
mod software;
#[cfg(feature = "software")]
pub use software::{encode_parallel, rgba_to_yuv};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Encode(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("この環境では {0} を使えません")]
    Unavailable(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Windows なら Media Foundation、それ以外はソフトウェア。
    Auto,
    Software,
    MediaFoundation,
}

impl Backend {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "software" | "rust" | "rusty_h264" => Some(Self::Software),
            "mf" | "mediafoundation" => Some(Self::MediaFoundation),
            _ => None,
        }
    }

    pub fn resolve(self) -> Self {
        match self {
            Self::Auto if cfg!(windows) => Self::MediaFoundation,
            Self::Auto => Self::Software,
            other => other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    /// 偶数にする（H.264 の 4:2:0）。
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Media Foundation の目標ビットレート。
    pub video_bitrate: u32,
    /// rusty_h264 の量子化パラメータ（小さいほど高画質。既定 23）。
    pub qp: u8,
    /// キーフレーム間隔（フレーム数）。並列エンコードの単位にもなる。
    pub gop: u32,
    /// 純 Rust エンコーダを速さ優先のプリセットにする（下書き向け）。
    pub fast: bool,
    pub audio_bitrate: u32,
    /// 音声を含めるか（48kHz ステレオ）。
    pub audio: bool,
}

impl Settings {
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        let pixels = u64::from(width) * u64::from(height);
        Self {
            width: width & !1,
            height: height & !1,
            fps: fps.max(1),
            // 1080p で 6Mbps を目安に解像度へ比例させる
            video_bitrate: ((pixels * 6_000_000) / (1920 * 1080)).clamp(500_000, 40_000_000) as u32,
            qp: 23,
            gop: fps.max(1) * 2,
            fast: false,
            audio_bitrate: 160_000,
            audio: true,
        }
    }
}

/// 映像フレームと音声を順に受け取って MP4 を作る。
pub trait VideoWriter: Send {
    /// 次のフレーム（`Settings` と同じ大きさ）。
    fn push_frame(&mut self, frame: &RgbaImage) -> Result<()>;
    /// 48kHz ステレオのインターリーブ PCM を追加する。フレームと交互に渡してよい。
    fn push_audio(&mut self, samples: &[f32]) -> Result<()>;
    fn finish(self: Box<Self>) -> Result<()>;
}

pub fn create(path: &Path, settings: &Settings, backend: Backend) -> Result<Box<dyn VideoWriter>> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let backend = backend.resolve();
    #[cfg(windows)]
    if backend == Backend::MediaFoundation {
        return Ok(Box::new(mf::MfWriter::new(path, settings)?));
    }
    #[cfg(feature = "software")]
    if backend == Backend::Software {
        return Ok(Box::new(software::SoftwareWriter::new(path, settings)?));
    }
    let _ = settings;
    Err(match backend {
        Backend::MediaFoundation => Error::Unavailable("Media Foundation（Windows 専用）"),
        _ => Error::Unavailable("ソフトウェアエンコーダ（feature `software`）"),
    })
}
