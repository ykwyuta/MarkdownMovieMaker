//! MarkdownMovieMaker の台本モデル・パーサー・キャラクター定義・タイムライン。
//!
//! I/O を持たない純粋なデータ層で、CLI と GUI（プレビュー）の両方から使う。
pub mod attrs;
pub mod character;
pub mod color;
pub mod model;
pub mod parser;
pub mod timeline;

pub use attrs::{Anim, Blend, Ease, Fx, Motion, MotionKind, Props, SceneTime};
pub use character::{Character, CharacterMaster, SubtitleStyle, Variation, VoiceConfig};
pub use color::Rgba;
pub use model::*;
pub use parser::{Diagnostic, Parsed, parse_script};
pub use timeline::{
    BgmCue, SceneSpan, Segment, SegmentKind, SoundCue, Timeline, TimelineInput, estimate_duration,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: キャラクター定義を読めません: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}
