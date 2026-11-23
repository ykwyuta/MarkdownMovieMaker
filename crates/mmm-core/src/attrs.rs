//! 拡張構文の属性 `{key: value, flag, ...}` と、その値（アニメーション・時刻・エフェクト）。
//!
//! YMM4 のアイテム設定（座標・拡大率・回転・不透明度・中間点・表示/非表示アニメーション・
//! 映像エフェクト・合成モード）に相当するものを台本の 1 行に書けるようにする。
use crate::color::Rgba;
use std::collections::BTreeMap;

/// 行末の `{...}` を本文と属性に分ける。`{` が対応していなければ属性なし。
pub fn split_attrs(line: &str) -> (&str, Option<&str>) {
    let t = line.trim_end();
    if !t.ends_with('}') {
        return (line, None);
    }
    let mut depth = 0i32;
    let mut quote = false;
    for (i, c) in t.char_indices().rev() {
        match c {
            '"' => quote = !quote,
            '}' if !quote => depth += 1,
            '{' if !quote => {
                depth -= 1;
                if depth == 0 {
                    return (t[..i].trim_end(), Some(&t[i + 1..t.len() - 1]));
                }
            }
            _ => {}
        }
    }
    (line, None)
}

/// `a, b(1, 2), "c, d"` をトップレベルの区切りで分ける。
pub fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut start) = (0i32, false, 0);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quote = !quote,
            '(' | '[' | '{' if !quote => depth += 1,
            ')' | ']' | '}' if !quote => depth -= 1,
            c if c == sep && !quote && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out.into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 属性の生の値（キー → 値。値のないキーは `""`）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attrs(pub BTreeMap<String, String>);

impl Attrs {
    pub fn parse(body: &str) -> Self {
        let mut map = BTreeMap::new();
        for entry in split_top(body, ',') {
            match entry.split_once(':') {
                Some((k, v)) => {
                    map.insert(k.trim().to_ascii_lowercase(), unquote(v.trim()).to_owned())
                }
                None => map.insert(entry.trim().to_ascii_lowercase(), String::new()),
            };
        }
        Self(map)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|v| v.is_empty() || matches!(v, "true" | "yes" | "on" | "1"))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

/// `120%` → 1.2、`1.5` → 1.5。
pub fn number(s: &str) -> Option<f64> {
    let s = s.trim();
    let v = match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok()? / 100.0,
        None => s.parse().ok()?,
    };
    v.is_finite().then_some(v)
}

/// 秒数（`1.5`、`1.5s`、`500ms`）。
pub fn seconds(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        return number(ms).map(|v| v / 1000.0);
    }
    number(s.strip_suffix('s').unwrap_or(s))
}

// ---- 補間 ---------------------------------------------------------------

/// 中間点の間の補間方法（YMM4 の「移動方法」）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ease {
    #[default]
    Linear,
    In,
    Out,
    InOut,
    /// 少し行き過ぎて戻る。
    Back,
    /// 弾んで止まる。
    Bounce,
    /// ばねのように揺れて止まる。
    Elastic,
    /// 補間せず次の中間点で切り替える。
    Step,
}

impl Ease {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "linear" | "直線" => Self::Linear,
            "ease-in" | "in" | "加速" => Self::In,
            "ease-out" | "out" | "減速" => Self::Out,
            "ease-in-out" | "in-out" | "ease" | "加減速" => Self::InOut,
            "back" => Self::Back,
            "bounce" | "弾む" => Self::Bounce,
            "elastic" => Self::Elastic,
            "step" | "瞬間" => Self::Step,
            _ => return None,
        })
    }

    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::In => t * t * t,
            Self::Out => 1.0 - (1.0 - t).powi(3),
            Self::InOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::Back => {
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            Self::Bounce => {
                let (n, d) = (7.5625, 2.75);
                if t < 1.0 / d {
                    n * t * t
                } else if t < 2.0 / d {
                    let t = t - 1.5 / d;
                    n * t * t + 0.75
                } else if t < 2.5 / d {
                    let t = t - 2.25 / d;
                    n * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d;
                    n * t * t + 0.984375
                }
            }
            Self::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * std::f64::consts::TAU / 3.0).sin()
                        + 1.0
                }
            }
            Self::Step => {
                if t >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// アイテム内の時刻の指定（中間点の位置）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyTime {
    /// アイテム開始からの秒。
    Secs(f64),
    /// アイテムの長さに対する割合（0〜1）。
    Ratio(f64),
    /// 終了から遡った秒（`end` は 0）。
    FromEnd(f64),
}

impl KeyTime {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s == "end" {
            return Some(Self::FromEnd(0.0));
        }
        if let Some(r) = s.strip_prefix("end-") {
            return seconds(r).map(Self::FromEnd);
        }
        if let Some(p) = s.strip_suffix('%') {
            return p.trim().parse::<f64>().ok().map(|v| Self::Ratio(v / 100.0));
        }
        seconds(s).map(Self::Secs)
    }

    pub fn resolve(self, duration: f64) -> f64 {
        match self {
            Self::Secs(s) => s,
            Self::Ratio(r) => r * duration,
            Self::FromEnd(s) => duration - s,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub time: KeyTime,
    pub value: f64,
    /// 前の中間点からこの中間点へ向かうときの補間。
    pub ease: Ease,
}

/// 時間で変化する数値。`100`、`0->400 ease-out`、`0 400@1.5~bounce 300@end`。
#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    pub keys: Vec<Key>,
}

impl Anim {
    pub fn constant(v: f64) -> Self {
        Self {
            keys: vec![Key {
                time: KeyTime::Secs(0.0),
                value: v,
                ease: Ease::Linear,
            }],
        }
    }

    pub fn is_constant(&self) -> bool {
        self.keys.iter().all(|k| k.value == self.keys[0].value)
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some((a, b)) = s.split_once("->") {
            let mut parts = b.split_whitespace();
            let to = number(parts.next()?)?;
            let ease = match parts.next() {
                Some(e) => Ease::parse(e)?,
                None => Ease::Linear,
            };
            return Some(Self {
                keys: vec![
                    Key {
                        time: KeyTime::Secs(0.0),
                        value: number(a)?,
                        ease: Ease::Linear,
                    },
                    Key {
                        time: KeyTime::FromEnd(0.0),
                        value: to,
                        ease,
                    },
                ],
            });
        }
        let tokens: Vec<&str> = s.split_whitespace().collect();
        let n = tokens.len();
        let mut keys = Vec::new();
        for (i, tok) in tokens.iter().enumerate() {
            let (tok, ease) = match tok.split_once('~') {
                Some((v, e)) => (v, Ease::parse(e)?),
                None => (*tok, Ease::Linear),
            };
            let (value, time) = match tok.split_once('@') {
                Some((v, t)) => (number(v)?, KeyTime::parse(t)?),
                None if i == 0 => (number(tok)?, KeyTime::Secs(0.0)),
                None if i + 1 == n => (number(tok)?, KeyTime::FromEnd(0.0)),
                None => (number(tok)?, KeyTime::Ratio(i as f64 / (n - 1) as f64)),
            };
            keys.push(Key { time, value, ease });
        }
        (!keys.is_empty()).then_some(Self { keys })
    }

    /// アイテム内の時刻 `t`（長さ `duration`）の値。
    pub fn at(&self, t: f64, duration: f64) -> f64 {
        let first = &self.keys[0];
        if self.keys.len() == 1 {
            return first.value;
        }
        let times: Vec<f64> = self.keys.iter().map(|k| k.time.resolve(duration)).collect();
        if t <= times[0] {
            return first.value;
        }
        for i in 0..self.keys.len() - 1 {
            let (a, b) = (times[i], times[i + 1]);
            if t < b {
                let span = (b - a).max(1e-9);
                let p = self.keys[i + 1].ease.apply((t - a) / span);
                return self.keys[i].value + (self.keys[i + 1].value - self.keys[i].value) * p;
            }
        }
        self.keys.last().map(|k| k.value).unwrap_or(0.0)
    }
}

// ---- 時刻 ---------------------------------------------------------------

/// シーン内の時刻。`1.5`、`L2`（2 番目のセリフの開始）、`L2+0.5`、`L2.end`、`end-1`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SceneTime {
    /// シーン開始からの秒。
    Secs(f64),
    /// n 番目（0 始まり）のセリフの開始 + 秒。
    LineStart(usize, f64),
    /// n 番目のセリフの終了 + 秒。
    LineEnd(usize, f64),
    /// シーン終了 + 秒（通常は負）。
    End(f64),
}

impl SceneTime {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (base, offset) = match s.find(['+', '-']).filter(|&i| i > 0) {
            Some(i) => (&s[..i], {
                let v = seconds(&s[i + 1..])?;
                if s.as_bytes()[i] == b'-' { -v } else { v }
            }),
            None => (s, 0.0),
        };
        if base == "end" {
            return Some(Self::End(offset));
        }
        if let Some(rest) = base.strip_prefix('L').or_else(|| base.strip_prefix('l')) {
            let (n, end) = match rest.strip_suffix(".end") {
                Some(n) => (n, true),
                None => (rest, false),
            };
            let n: usize = n.parse().ok().filter(|n| *n >= 1)?;
            return Some(if end {
                Self::LineEnd(n - 1, offset)
            } else {
                Self::LineStart(n - 1, offset)
            });
        }
        if base == "start" {
            return Some(Self::Secs(offset));
        }
        seconds(s).map(Self::Secs)
    }
}

// ---- 表示・非表示アニメーション -------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    Fade,
    Zoom,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    Spin,
    Pop,
    Blur,
    Drop,
    /// ガラスのように割れて飛び散る（`in` では破片が集まってくる）。
    Shatter,
}

impl MotionKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "fade" | "フェード" => Self::Fade,
            "zoom" | "拡大" => Self::Zoom,
            "slide-left" => Self::SlideLeft,
            "slide-right" => Self::SlideRight,
            "slide-up" => Self::SlideUp,
            "slide-down" => Self::SlideDown,
            "wipe-left" => Self::WipeLeft,
            "wipe-right" => Self::WipeRight,
            "wipe-up" => Self::WipeUp,
            "wipe-down" => Self::WipeDown,
            "spin" | "回転" => Self::Spin,
            "pop" | "ポップ" => Self::Pop,
            "blur" | "ぼかし" => Self::Blur,
            "drop" | "落下" => Self::Drop,
            "shatter" | "破砕" | "割れる" => Self::Shatter,
            _ => return None,
        })
    }
}

/// `in: slide-left 0.4 ease-out`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub kind: MotionKind,
    pub duration: f64,
    pub ease: Ease,
}

impl Motion {
    pub fn parse(s: &str) -> Option<Self> {
        let mut it = s.split_whitespace();
        let kind = MotionKind::parse(it.next()?)?;
        let mut m = Self {
            kind,
            duration: 0.3,
            ease: Ease::Out,
        };
        for tok in it {
            if let Some(e) = Ease::parse(tok) {
                m.ease = e;
            } else {
                m.duration = seconds(tok)?.max(0.0);
            }
        }
        Some(m)
    }
}

// ---- 映像エフェクト -------------------------------------------------------

/// 映像エフェクト（YMM4 の「映像エフェクト」相当）。`fx: blur(6) shadow(8,8,10,#0008)`
#[derive(Clone, Debug, PartialEq)]
pub enum Fx {
    Blur(f32),
    Shadow {
        dx: f32,
        dy: f32,
        blur: f32,
        color: Rgba,
    },
    Glow {
        radius: f32,
        color: Rgba,
    },
    Outline {
        width: f32,
        color: Rgba,
    },
    Mosaic(u32),
    Grayscale,
    Sepia,
    Negative,
    Brightness(f32),
    Contrast(f32),
    Saturation(f32),
    Hue(f32),
    /// 指定色を透過（クロマキー）。許容差 0〜1。
    Chroma {
        color: Rgba,
        tolerance: f32,
    },
    /// 色を乗算する（色調補正）。
    Tint(Rgba),
    // 以下は時間で動くもの
    Shake {
        amount: f32,
        speed: f32,
    },
    Bounce {
        height: f32,
        speed: f32,
    },
    Float {
        amount: f32,
        period: f32,
    },
    Sway {
        degrees: f32,
        period: f32,
    },
    Spin(f32),
    Pulse {
        amount: f32,
        period: f32,
    },
    Blink {
        period: f32,
    },
    Afterimage {
        count: u32,
        interval: f32,
    },
}

impl Fx {
    /// `name(arg, ...)` または `name`。
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (name, args) = match s.split_once('(') {
            Some((n, rest)) => (n.trim(), split_top(rest.strip_suffix(')')?, ',')),
            None => (s, Vec::new()),
        };
        let num = |i: usize, d: f32| -> Option<f32> {
            match args.get(i) {
                Some(a) => number(a).map(|v| v as f32),
                None => Some(d),
            }
        };
        let color = |i: usize, d: Rgba| -> Option<Rgba> {
            match args.get(i) {
                Some(a) => Rgba::parse(a),
                None => Some(d),
            }
        };
        let shadow = Rgba([0, 0, 0, 160]);
        Some(match name {
            "blur" | "ぼかし" => Self::Blur(num(0, 6.0)?),
            "shadow" | "影" => Self::Shadow {
                dx: num(0, 6.0)?,
                dy: num(1, 6.0)?,
                blur: num(2, 8.0)?,
                color: color(3, shadow)?,
            },
            "glow" | "発光" => Self::Glow {
                radius: num(0, 12.0)?,
                color: color(1, Rgba::WHITE)?,
            },
            "outline" | "縁取り" => Self::Outline {
                width: num(0, 4.0)?,
                color: color(1, Rgba::WHITE)?,
            },
            "mosaic" | "モザイク" => Self::Mosaic(num(0, 12.0)?.max(1.0) as u32),
            "grayscale" | "モノクロ" => Self::Grayscale,
            "sepia" | "セピア" => Self::Sepia,
            "negative" | "invert" | "ネガ" => Self::Negative,
            "brightness" | "明るさ" => Self::Brightness(num(0, 1.2)?),
            "contrast" | "コントラスト" => Self::Contrast(num(0, 1.2)?),
            "saturation" | "彩度" => Self::Saturation(num(0, 1.5)?),
            "hue" | "色相" => Self::Hue(num(0, 30.0)?),
            "chroma" | "クロマキー" => Self::Chroma {
                color: color(0, Rgba([0, 255, 0, 255]))?,
                tolerance: num(1, 0.3)?,
            },
            "tint" | "色調" => Self::Tint(color(0, Rgba([255, 220, 200, 255]))?),
            "shake" | "振動" => Self::Shake {
                amount: num(0, 8.0)?,
                speed: num(1, 30.0)?,
            },
            "bounce" | "跳ねる" => Self::Bounce {
                height: num(0, 20.0)?,
                speed: num(1, 5.0)?,
            },
            "float" | "浮遊" => Self::Float {
                amount: num(0, 10.0)?,
                period: num(1, 2.0)?,
            },
            "sway" | "揺れ" => Self::Sway {
                degrees: num(0, 5.0)?,
                period: num(1, 1.5)?,
            },
            "spin" | "回転" => Self::Spin(num(0, 90.0)?),
            "pulse" | "鼓動" => Self::Pulse {
                amount: num(0, 0.08)?,
                period: num(1, 0.8)?,
            },
            "blink" | "点滅" => Self::Blink {
                period: num(0, 0.5)?,
            },
            "afterimage" | "残像" => Self::Afterimage {
                count: num(0, 4.0)?.max(1.0) as u32,
                interval: num(1, 0.05)?,
            },
            _ => return None,
        })
    }

    /// 時間に依存しない（画像のキャッシュに使える）エフェクトか。
    pub fn is_static(&self) -> bool {
        !matches!(
            self,
            Self::Shake { .. }
                | Self::Bounce { .. }
                | Self::Float { .. }
                | Self::Sway { .. }
                | Self::Spin(_)
                | Self::Pulse { .. }
                | Self::Blink { .. }
                | Self::Afterimage { .. }
        )
    }

    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        split_top(s, ' ')
            .into_iter()
            .map(|f| Self::parse(f).ok_or_else(|| format!("未知のエフェクト `{f}`")))
            .collect()
    }
}

/// 合成モード。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    #[default]
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Lighten,
    Darken,
}

impl Blend {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "normal" | "通常" => Self::Normal,
            "add" | "加算" => Self::Add,
            "multiply" | "乗算" => Self::Multiply,
            "screen" | "スクリーン" => Self::Screen,
            "overlay" | "オーバーレイ" => Self::Overlay,
            "lighten" | "比較(明)" => Self::Lighten,
            "darken" | "比較(暗)" => Self::Darken,
            _ => return None,
        })
    }
}

/// 描画アイテム共通の設定（YMM4 の「描画」グループ）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    /// 画面中央を原点とする中心座標（基準解像度 1920×1080 のピクセル）。
    pub x: Option<Anim>,
    pub y: Option<Anim>,
    /// 拡大率（1.0 = 100%）。
    pub scale: Option<Anim>,
    pub scale_x: Option<Anim>,
    pub scale_y: Option<Anim>,
    /// 回転（度、時計回り）。
    pub rotate: Option<Anim>,
    pub opacity: Option<Anim>,
    pub flip: bool,
    pub flip_v: bool,
    pub blend: Blend,
    /// 大きいほど手前。
    pub layer: Option<i32>,
    pub at: Option<SceneTime>,
    pub until: Option<SceneTime>,
    pub duration: Option<f64>,
    pub enter: Option<Motion>,
    pub exit: Option<Motion>,
    pub fx: Vec<Fx>,
}

impl Props {
    /// 共通キーを取り出す。解釈できない値はエラー文にして返す。共通でないキーは残す。
    pub fn take(attrs: &mut Attrs) -> (Self, Vec<String>) {
        let mut p = Self::default();
        let mut errors = Vec::new();
        let anim = |attrs: &mut Attrs, key: &str, errors: &mut Vec<String>| -> Option<Anim> {
            let v = attrs.0.remove(key)?;
            let a = Anim::parse(&v);
            if a.is_none() {
                errors.push(format!("`{key}` の値を解釈できません: {v}"));
            }
            a
        };
        p.x = anim(attrs, "x", &mut errors);
        p.y = anim(attrs, "y", &mut errors);
        p.scale = anim(attrs, "scale", &mut errors).or_else(|| anim(attrs, "zoom", &mut errors));
        p.scale_x = anim(attrs, "scale_x", &mut errors);
        p.scale_y = anim(attrs, "scale_y", &mut errors);
        p.rotate = anim(attrs, "rotate", &mut errors);
        p.opacity =
            anim(attrs, "opacity", &mut errors).or_else(|| anim(attrs, "alpha", &mut errors));
        p.flip = attrs.0.remove("flip").is_some();
        p.flip_v = attrs.0.remove("flipv").is_some();
        let mut take = |key: &str| attrs.0.remove(key);
        if let Some(v) = take("blend") {
            match Blend::parse(&v) {
                Some(b) => p.blend = b,
                None => errors.push(format!("未知の合成モード `{v}`")),
            }
        }
        if let Some(v) = take("layer") {
            match v.trim().parse() {
                Ok(l) => p.layer = Some(l),
                Err(_) => errors.push(format!("`layer` は整数で指定してください: {v}")),
            }
        }
        for (key, slot) in [("at", &mut p.at), ("until", &mut p.until)] {
            if let Some(v) = take(key) {
                match SceneTime::parse(&v) {
                    Some(t) => *slot = Some(t),
                    None => errors.push(format!(
                        "`{key}` の時刻を解釈できません: {v}（例: 1.5, L2, L2+0.5, L3.end, end-1）"
                    )),
                }
            }
        }
        if let Some(v) = take("dur") {
            match seconds(&v) {
                Some(d) if d > 0.0 => p.duration = Some(d),
                _ => errors.push(format!("`dur` は正の秒数で指定してください: {v}")),
            }
        }
        for (key, slot) in [("in", &mut p.enter), ("out", &mut p.exit)] {
            if let Some(v) = take(key) {
                match Motion::parse(&v) {
                    Some(m) => *slot = Some(m),
                    None => errors.push(format!("`{key}` のアニメーションを解釈できません: {v}")),
                }
            }
        }
        if let Some(v) = take("fx") {
            match Fx::parse_list(&v) {
                Ok(f) => p.fx = f,
                Err(e) => errors.push(e),
            }
        }
        (p, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_trailing_attribute_block() {
        assert_eq!(
            split_attrs("zundamon: やあ {face: smile, x: 1}"),
            ("zundamon: やあ", Some("face: smile, x: 1"))
        );
        assert_eq!(
            split_attrs("text: \"{a}\" {x: 1}"),
            ("text: \"{a}\"", Some("x: 1"))
        );
        assert_eq!(split_attrs("普通の文"), ("普通の文", None));
        let a = Attrs::parse("face: smile, flip, fx: blur(4) shadow(2, 2), text: \"a, b\"");
        assert_eq!(a.get("face"), Some("smile"));
        assert!(a.flag("flip"));
        assert_eq!(a.get("fx"), Some("blur(4) shadow(2, 2)"));
        assert_eq!(a.get("text"), Some("a, b"));
    }

    #[test]
    fn animations_interpolate_with_easing() {
        let a = Anim::parse("0->100").unwrap();
        assert_eq!(a.at(0.5, 1.0), 50.0);
        assert_eq!(a.at(2.0, 1.0), 100.0);
        let k = Anim::parse("0 100@1~ease-out 50@end").unwrap();
        assert_eq!(k.at(0.0, 3.0), 0.0);
        assert!(k.at(0.5, 3.0) > 50.0); // 減速は前半で大きく進む
        assert_eq!(k.at(1.0, 3.0), 100.0);
        assert_eq!(k.at(2.0, 3.0), 75.0);
        assert_eq!(Anim::parse("120%").unwrap().at(0.0, 1.0), 1.2);
        let three = Anim::parse("0 10 0").unwrap();
        assert_eq!(three.at(1.0, 2.0), 10.0);
        assert!(Anim::parse("x").is_none());
        for e in [Ease::Back, Ease::Bounce, Ease::Elastic, Ease::InOut] {
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9 && e.apply(0.0).abs() < 1e-9);
        }
    }

    #[test]
    fn scene_times_effects_and_props() {
        assert_eq!(
            SceneTime::parse("L2+0.5"),
            Some(SceneTime::LineStart(1, 0.5))
        );
        assert_eq!(SceneTime::parse("L3.end"), Some(SceneTime::LineEnd(2, 0.0)));
        assert_eq!(SceneTime::parse("end-1"), Some(SceneTime::End(-1.0)));
        assert_eq!(SceneTime::parse("1.5"), Some(SceneTime::Secs(1.5)));
        assert_eq!(
            Fx::parse("shadow(4,4)"),
            Some(Fx::Shadow {
                dx: 4.0,
                dy: 4.0,
                blur: 8.0,
                color: Rgba([0, 0, 0, 160])
            })
        );
        assert_eq!(Fx::parse("ぼかし(3)"), Some(Fx::Blur(3.0)));
        assert!(Fx::parse_list("blur(2) nope").is_err());
        let mut a = Attrs::parse(
            "x: -300->0 ease-out, layer: 5, in: slide-left 0.5, fx: glow(8, #ff0), at: L2, size: 80",
        );
        let (p, errs) = Props::take(&mut a);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(p.layer, Some(5));
        assert_eq!(p.enter.unwrap().kind, MotionKind::SlideLeft);
        assert_eq!(p.at, Some(SceneTime::LineStart(1, 0.0)));
        assert_eq!(a.get("size"), Some("80")); // 種類ごとのキーは残る
        let mut bad = Attrs::parse("layer: x, in: wobble");
        assert_eq!(Props::take(&mut bad).1.len(), 2);
    }
}
