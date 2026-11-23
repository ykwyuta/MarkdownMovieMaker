//! `character_master.json`。Python 版の形式をそのまま読み、`voice` を追加で受け付ける。
use crate::{Error, color::Rgba};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub type CharacterMaster = BTreeMap<String, Character>;

pub fn load_master(path: &Path) -> Result<CharacterMaster, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.display().to_string(),
        source,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Character {
    #[serde(default)]
    pub name: String,
    /// VOICEVOX の話者 ID（旧形式）。`voice.engine = "voicevox"` のときに使う。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<u32>,
    /// 音声合成の設定。省略時はエンジンの既定話者。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceConfig>,
    #[serde(default)]
    pub variations: BTreeMap<String, Variation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_style: Option<SubtitleStyle>,
    /// まばたきの平均間隔（秒）。既定 4 秒。0 でまばたきしない。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blink_interval: Option<f64>,
}

impl Character {
    /// 指定のバリエーション。無ければ `default`。
    pub fn variation(&self, name: &str) -> Option<&Variation> {
        self.variations
            .get(name)
            .or_else(|| self.variations.get("default"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Variation {
    /// 口を閉じた立ち絵（パーツ方式では `base` を使う）。
    #[serde(default)]
    pub image_closed: String,
    /// 口を開けた立ち絵。
    #[serde(default)]
    pub image_open: String,
    /// パーツ方式の土台（目・口を除いた体）。`eyes`・`mouth` はこれと同じ大きさの透過画像。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// 目パチ: 開いた目 → 半目 → 閉じた目、の順の画像。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eyes: Vec<String>,
    /// あいうえお口パク: `a` `i` `u` `e` `o` `n`（閉じ）の画像。足りない母音は近いもので代用する。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mouth: BTreeMap<String, String>,
    /// 背景の高さに対する立ち絵の高さの比率。
    #[serde(default = "one")]
    pub scale: f64,
    /// 立ち絵の左上の位置（背景のピクセル座標）。
    #[serde(default)]
    pub pos: [f64; 2],
}

fn one() -> f64 {
    1.0
}

/// 音声合成の設定。エンジンごとに使う項目が異なる。
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct VoiceConfig {
    /// `voicevox`（既定）/ `kokoro`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// 話者名（Kokoro の `jf_alpha`、VOICEVOX の話者 ID など）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// `ja` / `en` / `auto` など。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// 話し方の指示（対応エンジンのみ、互換性のため保持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruct: Option<String>,
    /// 読み上げ速度の倍率（シーンの `speed` と掛け合わせる）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// 音量の倍率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    /// 声の高さの倍率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SubtitleStyle {
    #[serde(default = "fontsize")]
    pub fontsize: f32,
    #[serde(default = "black")]
    pub color: Rgba,
    #[serde(default = "red")]
    pub border_color: Rgba,
    #[serde(default = "fill")]
    pub fill_color: Rgba,
    #[serde(default = "padding")]
    pub padding: f32,
    #[serde(default = "border_width")]
    pub border_width: f32,
    #[serde(default = "corner_radius")]
    pub corner_radius: f32,
    /// 字幕のフォント（基準ディレクトリからの相対パス）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// 文字の縁取り（`"6 #000000"`、`|` 区切りで多重）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<String>,
    /// 文字の影（`"4 4 6 #00000080"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<String>,
    /// 背景の箱を出さない。
    #[serde(default)]
    pub no_box: bool,
    /// 1 文字ずつ表示する間隔（秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typewriter: Option<f64>,
    /// 字幕の下端の位置（画面の高さに対する割合、既定 0.95）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f32>,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            fontsize: fontsize(),
            color: black(),
            border_color: red(),
            fill_color: fill(),
            padding: padding(),
            border_width: border_width(),
            corner_radius: corner_radius(),
            font: None,
            outline: None,
            shadow: None,
            no_box: false,
            typewriter: None,
            bottom: None,
        }
    }
}

fn fontsize() -> f32 {
    70.0
}
fn black() -> Rgba {
    Rgba::BLACK
}
fn red() -> Rgba {
    Rgba([255, 0, 0, 255])
}
fn fill() -> Rgba {
    Rgba([255, 255, 255, 200])
}
fn padding() -> f32 {
    20.0
}
fn border_width() -> f32 {
    4.0
}
fn corner_radius() -> f32 {
    10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_python_format_and_voice() {
        let json = r#"{"zundamon": {"name": "ずんだもん", "speaker_id": 3,
            "voice": {"speaker": "Ono_Anna", "instruct": "元気に"},
            "variations": {"default": {"image_closed": "c.png", "image_open": "o.png", "scale": 0.9, "pos": [1200, 300]}},
            "subtitle_style": {"fontsize": 70, "color": "green", "border_color": "green", "pos": ["center", "bottom"]}}}"#;
        let m: CharacterMaster = serde_json::from_str(json).unwrap();
        let z = &m["zundamon"];
        assert_eq!(z.speaker_id, Some(3));
        assert_eq!(
            z.voice.as_ref().unwrap().speaker.as_deref(),
            Some("Ono_Anna")
        );
        assert_eq!(z.variation("missing").unwrap().pos, [1200.0, 300.0]);
        let s = z.subtitle_style.as_ref().unwrap();
        assert_eq!(s.color, Rgba([0, 128, 0, 255]));
        assert_eq!(s.fill_color, Rgba([255, 255, 255, 200]));
    }

    #[test]
    fn repository_master_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/character_master.json");
        if path.exists() {
            let m = load_master(&path).unwrap();
            assert!(m.contains_key("zundamon"));
        }
    }
}
