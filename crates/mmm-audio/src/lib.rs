//! 音声処理。デコード・リサンプル・速度変更・タイムラインのミックス・WAV/MP3 書き出し。
//!
//! すべて純 Rust で、ffmpeg / libmp3lame には依存しない。
pub mod buffer;
pub mod decode;
pub mod mix;
pub mod mp3;
pub mod resample;
pub mod stretch;
pub mod wav;

pub use buffer::AudioBuffer;
pub use mix::{MixInput, Mixer, Source, SourceCache, VoiceClip};

/// 内部の標準サンプリングレート（Opus/AAC と合わせる）。
pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {message}")]
    Decode { path: String, message: String },
    #[error("WAV: {0}")]
    Wav(#[from] hound::Error),
    #[error("MP3: {0}")]
    Mp3(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
