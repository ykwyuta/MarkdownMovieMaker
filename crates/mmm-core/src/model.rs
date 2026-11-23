//! 台本のデータモデル。
//!
//! Python 版 `scene.py` の構造（シーン・キャラクター・セリフ・効果音）に、YMM4 相当の
//! アイテム（テキスト・画像・動画・図形）、レイヤー、アニメーション、場面切り替え、
//! セリフごとの表情・読み・声の調整を加えたもの。
use crate::{
    attrs::{Ease, Fx, Props},
    color::Rgba,
};

/// 台本全体。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub scenes: Vec<Scene>,
}

impl Script {
    /// 台本に登場する（セリフを話す）キャラクター ID の一覧（重複なし、出現順）。
    pub fn speakers(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for line in self.scenes.iter().flat_map(|s| &s.lines) {
            if !out.contains(&line.speaker.as_str()) {
                out.push(&line.speaker);
            }
        }
        out
    }
}

/// `# Scene` で始まる 1 シーン。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// 1 始まりの通し番号（Python 版と同じ）。
    pub id: usize,
    /// `# Scene@名前` の名前。
    pub name: Option<String>,
    /// 背景画像。省略時は直前のシーンの背景を引き継ぐ。
    pub background: Option<String>,
    /// 背景に掛ける設定（`- bg: path {fx: blur(4), scale: 1->1.1}`）。
    pub background_props: Props,
    pub bgm: Vec<Bgm>,
    pub lines: Vec<Line>,
    pub characters: Vec<CharacterRef>,
    pub before_pause: f64,
    pub after_pause: f64,
    /// シーンの最短の長さ（`- length: 5`）。足りない分は終了後の間を延ばす。
    pub length: Option<f64>,
    /// テキスト・画像・動画・図形（`overlay` もここに入る）。
    pub items: Vec<Item>,
    pub sounds: Vec<Sound>,
    /// 読み上げ速度の倍率。
    pub speed: Option<f64>,
    /// 前のシーンからの場面切り替え。
    pub transition: Option<Transition>,
    /// シーン全体（字幕を除く）に掛けるカメラ（ズーム・移動・回転・エフェクト）。
    pub camera: Option<Props>,
    /// 見出し行の行番号（0 始まり）。エディタのジャンプに使う。
    pub source_line: usize,
}

impl Scene {
    pub fn character(&self, id: &str) -> Option<&CharacterRef> {
        self.characters.iter().find(|c| c.id == id)
    }
}

/// `- id: zundamon@smile.bouncing {x: 300, flip, in: fade}`
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterRef {
    pub id: String,
    pub variation: String,
    pub effect: Option<Effect>,
    pub props: Props,
}

/// キャラクターに掛ける映像効果（Python 版の `.effect`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// 話している間だけ跳ねる。
    Bouncing,
    Shaking,
    Grayscale,
    Shatter,
    /// 未知の効果名。描画では無視し、診断に表示する。
    Other(String),
}

impl Effect {
    pub fn parse(name: &str) -> Self {
        match name {
            "bouncing" => Self::Bouncing,
            "shaking" => Self::Shaking,
            "grayscale" => Self::Grayscale,
            "shatter" => Self::Shatter,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// `zundamon: セリフ {face: smile, pitch: 1.1, wait: 0.5}`
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub speaker: String,
    /// 字幕に表示する文（ルビ記法 `｜漢字《かんじ》` は漢字側）。
    pub text: String,
    /// 読み上げる文（ルビ記法の読み側、または `read:`）。
    pub read: String,
    /// `speaker@noscript:` や `{subtitle: off}` のとき false。
    pub show_subtitle: bool,
    /// このセリフの間だけ使う立ち絵のバリエーション（表情）。
    pub face: Option<String>,
    pub speed: Option<f64>,
    /// 声の高さ（1.0 = 元のまま、1.1 で約 1.65 半音上がる）。
    pub pitch: Option<f64>,
    pub volume: Option<f64>,
    /// セリフの前後の間（秒）。
    pub pre: f64,
    pub wait: f64,
    /// 話している間だけ話者に掛けるエフェクト（`motion: bounce(30) shake(4)`）。
    pub motion: Vec<Fx>,
    /// セリフの開始と同時に鳴らす効果音。
    pub se: Option<String>,
    /// 話し方の指示（対応エンジンのみ、互換性のため保持）。
    pub instruct: Option<String>,
    pub source_line: usize,
}

impl Line {
    pub fn new(speaker: &str, text: &str, source_line: usize) -> Self {
        Self {
            speaker: speaker.to_owned(),
            text: text.to_owned(),
            read: text.to_owned(),
            show_subtitle: true,
            face: None,
            speed: None,
            pitch: None,
            volume: None,
            pre: 0.0,
            wait: 0.0,
            motion: Vec::new(),
            se: None,
            instruct: None,
            source_line,
        }
    }
}

/// `- sound: path 長さ [開始秒] {volume: 0.8, at: L2}`
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub path: String,
    /// 再生する最大の長さ（秒）。`None` はファイルの最後まで。
    pub duration: Option<f64>,
    /// シーン開始からの再生開始位置（秒）。
    pub at: crate::attrs::SceneTime,
    pub volume: f64,
}

/// `- bgm: path {volume: 0.2, fade_in: 1, fade_out: 2, at: L2, until: end}`
#[derive(Clone, Debug, PartialEq)]
pub struct Bgm {
    pub path: String,
    /// `None` は全体 BGM と同じ音量。
    pub volume: Option<f64>,
    pub fade_in: f64,
    pub fade_out: Option<f64>,
    pub at: Option<crate::attrs::SceneTime>,
    pub until: Option<crate::attrs::SceneTime>,
    /// 次のシーン以降も止めずに流し続ける（`- bgm: path {continue}`）。
    pub keep: bool,
}

/// 場面切り替え（`- transition: wipe-left 0.6`）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: f64,
    pub ease: Ease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    /// クロスフェード。
    Fade,
    /// 黒を経由する。
    Black,
    /// 白を経由する。
    White,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    /// 中心から円が広がる。
    Circle,
    /// 拡大しながら切り替える。
    Zoom,
    /// モザイクを経由する。
    Mosaic,
    /// 前のシーンがガラスのように割れて飛び散り、新しいシーンが現れる。
    Shatter,
}

impl TransitionKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "fade" | "crossfade" | "クロスフェード" => Self::Fade,
            "black" | "fade-black" | "暗転" => Self::Black,
            "white" | "fade-white" | "ホワイトアウト" => Self::White,
            "wipe-left" => Self::WipeLeft,
            "wipe-right" => Self::WipeRight,
            "wipe-up" => Self::WipeUp,
            "wipe-down" => Self::WipeDown,
            "slide-left" => Self::SlideLeft,
            "slide-right" => Self::SlideRight,
            "slide-up" => Self::SlideUp,
            "slide-down" => Self::SlideDown,
            "circle" | "iris" | "円" => Self::Circle,
            "zoom" | "ズーム" => Self::Zoom,
            "mosaic" | "モザイク" => Self::Mosaic,
            "shatter" | "破砕" | "割れる" => Self::Shatter,
            _ => return None,
        })
    }
}

/// 立ち絵以外のアイテム。
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub props: Props,
    pub source_line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Text(TextSpec),
    Image(String),
    Video(VideoSpec),
    Shape(ShapeSpec),
}

impl ItemKind {
    /// 既定のレイヤー（立ち絵は 10、字幕は 100）。
    pub fn default_layer(&self) -> i32 {
        match self {
            Self::Video(_) | Self::Shape(_) => 5,
            Self::Image(_) => 50,
            Self::Text(_) => 60,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

/// テキストの装飾（字幕と共通）。
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    pub text: String,
    pub size: f32,
    pub color: Rgba,
    /// フォントファイル（基準ディレクトリからの相対パス）。
    pub font: Option<String>,
    /// 縁取り（内側から順に。複数指定で多重縁取り）。
    pub outlines: Vec<(f32, Rgba)>,
    pub shadow: Option<(f32, f32, f32, Rgba)>,
    /// 背景の箱（色, 余白, 角の半径, 枠の太さ, 枠の色）。
    pub boxed: Option<TextBox>,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    /// 文字間隔（ピクセル）。
    pub spacing: f32,
    /// 行の高さ（文字サイズに対する倍率）。
    pub line_height: f32,
    /// 1 文字ずつ表示する間隔（秒）。
    pub typewriter: Option<f64>,
    /// 折り返す幅（ピクセル、基準解像度）。
    pub max_width: Option<f32>,
    pub vertical: bool,
}

impl Default for TextSpec {
    fn default() -> Self {
        Self {
            text: String::new(),
            size: 64.0,
            color: Rgba::WHITE,
            font: None,
            outlines: Vec::new(),
            shadow: None,
            boxed: None,
            bold: false,
            italic: false,
            align: Align::Center,
            spacing: 0.0,
            line_height: 1.2,
            typewriter: None,
            max_width: None,
            vertical: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub fill: Rgba,
    pub padding: f32,
    pub radius: f32,
    pub border: f32,
    pub border_color: Rgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoSpec {
    pub path: String,
    pub volume: f64,
    /// 再生速度。
    pub speed: f64,
    /// 動画ファイル内の開始位置（秒）。
    pub trim: f64,
    pub looped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Triangle,
    Star,
    Line,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSpec {
    pub shape: ShapeKind,
    pub width: f32,
    pub height: f32,
    pub fill: Option<Rgba>,
    pub stroke: Option<(f32, Rgba)>,
    pub radius: f32,
}
