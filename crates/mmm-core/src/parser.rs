//! 台本（Markdown 風）のパーサー。
//!
//! Python 版 `script_parser.py` の文法をそのまま受け付け、次を拡張する。
//! - 行末の属性 `{key: value, ...}`（[`crate::attrs`]）
//! - アイテム行 `- text:` / `- image:` / `- video:` / `- shape:`、`- transition:`、`- camera:`、`- length:`
//! - セリフの属性（表情・読み・速度・高さ・音量・間・動き・効果音）とルビ `｜漢字《かんじ》`
//! - セリフの区切りの `:` の後の空白の省略、全角 `：`
//!
//! 不正な行は捨てずに [`Diagnostic`] として返す。
use crate::{
    attrs::{Anim, Attrs, Ease, Fx, Props, SceneTime, number, seconds, split_attrs, unquote},
    color::Rgba,
    model::*,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// 0 始まりの行番号。
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub script: Script,
    pub diagnostics: Vec<Diagnostic>,
}

/// Python の `\w` 相当（Unicode の英数字・アンダースコア）。
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 先頭から `\w+` を切り出す。
fn take_word(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !is_word(c)).unwrap_or(s.len());
    s.split_at(end)
}

fn is_scene_header(line: &str) -> bool {
    line.strip_prefix('#')
        .map(|rest| rest.trim_start().starts_with("Scene"))
        .unwrap_or(false)
}

fn scene_name(header: &str) -> Option<String> {
    let rest = header
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("Scene")?;
    let name = rest.strip_prefix('@')?.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// `(\w+)(?:@(\w+))?(?:\.(\w+))?`
fn parse_character(value: &str) -> Option<(String, String, Option<Effect>)> {
    let (id, mut rest) = take_word(value);
    if id.is_empty() {
        return None;
    }
    let mut variation = "default".to_owned();
    if let Some(r) = rest.strip_prefix('@') {
        let (v, r) = take_word(r);
        if !v.is_empty() {
            variation = v.to_owned();
            rest = r;
        }
    }
    let effect = rest
        .strip_prefix('.')
        .map(|r| take_word(r).0)
        .filter(|e| !e.is_empty())
        .map(Effect::parse);
    Some((id.to_owned(), variation, effect))
}

fn is_kanji(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff)
        || matches!(c, '々' | '〆' | 'ヶ')
}

/// ルビ記法を (表示, 読み) に分ける。`｜表示《読み》`、または `漢字《かんじ》`（直前の漢字の並び）。
pub fn split_ruby(text: &str) -> (String, String) {
    let mut display = String::new();
    let mut read = String::new();
    let mut base_start: Option<(usize, usize)> = None; // (display の位置, read の位置)
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '｜' | '|' if text.contains('《') => base_start = Some((display.len(), read.len())),
            '《' => {
                let ruby: String = chars.by_ref().take_while(|&c| c != '》').collect();
                let (_, r) = match base_start.take() {
                    Some(p) => p,
                    None => {
                        // 直前の漢字の並びを親文字にする
                        let start = display
                            .char_indices()
                            .rev()
                            .take_while(|(_, c)| is_kanji(*c))
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(display.len());
                        let base_len = display.len() - start;
                        (start, read.len() - base_len)
                    }
                };
                read.truncate(r);
                read.push_str(&ruby);
            }
            c => {
                display.push(c);
                read.push(c);
            }
        }
    }
    (display, read)
}

/// `(\w+)(?:@(\w+))?:\s*(.+)`
fn parse_dialogue(line: &str, source_line: usize) -> Option<(Line, Option<&str>)> {
    let (body, attrs) = split_attrs(line);
    let (speaker, rest) = take_word(body);
    if speaker.is_empty() {
        return None;
    }
    let (option, rest) = match rest.strip_prefix('@') {
        Some(r) => {
            let (o, r) = take_word(r);
            (Some(o), r)
        }
        None => (None, rest),
    };
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('：'))?;
    let text = rest.trim();
    if text.is_empty() {
        return None;
    }
    let (display, read) = split_ruby(text);
    let mut l = Line::new(speaker, &display, source_line);
    l.read = read;
    match option {
        Some("noscript") => l.show_subtitle = false,
        // `zundamon@smile:` は表情の指定として扱う
        Some(face) if !face.is_empty() => l.face = Some(face.to_owned()),
        _ => {}
    }
    Some((l, attrs))
}

struct Ctx<'a> {
    no: usize,
    diags: &'a mut Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn warn(&mut self, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            line: self.no,
            message: message.into(),
        });
    }

    fn props(&mut self, attrs: &mut Attrs) -> Props {
        let (p, errors) = Props::take(attrs);
        for e in errors {
            self.warn(e);
        }
        p
    }

    fn leftovers(&mut self, attrs: &Attrs, what: &str) {
        for k in attrs.0.keys() {
            self.warn(format!("{what}に未知の属性 `{k}` があります"));
        }
    }

    fn num(&mut self, attrs: &mut Attrs, key: &str) -> Option<f64> {
        let v = attrs.0.remove(key)?;
        let n = number(&v);
        if n.is_none() {
            self.warn(format!("`{key}` には数値を指定してください: {v}"));
        }
        n
    }

    fn secs(&mut self, attrs: &mut Attrs, key: &str) -> Option<f64> {
        let v = attrs.0.remove(key)?;
        let n = seconds(&v).filter(|s| *s >= 0.0);
        if n.is_none() {
            self.warn(format!("`{key}` には 0 以上の秒数を指定してください: {v}"));
        }
        n
    }

    fn color(&mut self, attrs: &mut Attrs, key: &str) -> Option<Rgba> {
        let v = attrs.0.remove(key)?;
        let c = Rgba::parse(&v);
        if c.is_none() {
            self.warn(format!(
                "`{key}` の色を解釈できません: {v}（例: white, #ff8800, #00000080）"
            ));
        }
        c
    }

    fn fx(&mut self, attrs: &mut Attrs, key: &str) -> Vec<Fx> {
        match attrs.0.remove(key).map(|v| Fx::parse_list(&v)) {
            Some(Ok(f)) => f,
            Some(Err(e)) => {
                self.warn(e);
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    fn scene_time(&mut self, attrs: &mut Attrs, key: &str) -> Option<SceneTime> {
        let v = attrs.0.remove(key)?;
        let t = SceneTime::parse(&v);
        if t.is_none() {
            self.warn(format!("`{key}` の時刻を解釈できません: {v}"));
        }
        t
    }
}

/// `"6 #000"` → (6, 黒)。
fn width_color(s: &str) -> Option<(f32, Rgba)> {
    let mut it = s.split_whitespace();
    let w = number(it.next()?)? as f32;
    let c = match it.next() {
        Some(c) => Rgba::parse(c)?,
        None => Rgba::BLACK,
    };
    Some((w, c))
}

/// テキストの装飾属性を取り出す（テキストアイテムと字幕で共通）。
pub fn take_text_style(spec: &mut TextSpec, attrs: &mut Attrs) -> Vec<String> {
    let mut errors = Vec::new();
    let mut take = |k: &str| attrs.0.remove(k);
    if let Some(v) = take("size") {
        match number(&v) {
            Some(n) if n > 0.0 => spec.size = n as f32,
            _ => errors.push(format!("`size` を解釈できません: {v}")),
        }
    }
    if let Some(v) = take("color") {
        match Rgba::parse(&v) {
            Some(c) => spec.color = c,
            None => errors.push(format!("`color` を解釈できません: {v}")),
        }
    }
    if let Some(v) = take("font") {
        spec.font = Some(v);
    }
    if let Some(v) = take("outline") {
        spec.outlines.clear();
        for part in v.split('|') {
            match width_color(part) {
                Some(o) => spec.outlines.push(o),
                None => errors.push(format!(
                    "`outline` は `太さ 色 | 太さ 色` で指定してください: {v}"
                )),
            }
        }
    }
    if let Some(v) = take("shadow") {
        let p: Vec<&str> = v.split_whitespace().collect();
        let n = |i: usize, d: f32| p.get(i).and_then(|s| number(s)).map_or(d, |v| v as f32);
        let color = p
            .get(3)
            .and_then(|c| Rgba::parse(c))
            .unwrap_or(Rgba([0, 0, 0, 160]));
        spec.shadow = Some((n(0, 4.0), n(1, 4.0), n(2, 4.0), color));
    }
    if let Some(v) = take("box") {
        let p: Vec<&str> = v.split_whitespace().collect();
        let n = |i: usize, d: f32| p.get(i).and_then(|s| number(s)).map_or(d, |v| v as f32);
        match p.first().and_then(|c| Rgba::parse(c)) {
            Some(fill) => {
                spec.boxed = Some(TextBox {
                    fill,
                    padding: n(1, 16.0),
                    radius: n(2, 10.0),
                    border: n(3, 0.0),
                    border_color: p.get(4).and_then(|c| Rgba::parse(c)).unwrap_or(Rgba::WHITE),
                })
            }
            None => errors.push(format!(
                "`box` は `塗り色 余白 半径 [枠の太さ 枠の色]` で指定してください: {v}"
            )),
        }
    }
    if take("bold").is_some() {
        spec.bold = true;
    }
    if take("italic").is_some() {
        spec.italic = true;
    }
    if take("vertical").is_some() {
        spec.vertical = true;
    }
    if let Some(v) = take("align") {
        spec.align = match v.as_str() {
            "left" | "左" => Align::Left,
            "right" | "右" => Align::Right,
            "center" | "中央" => Align::Center,
            _ => {
                errors.push(format!("`align` は left / center / right です: {v}"));
                spec.align
            }
        };
    }
    for (key, slot) in [
        ("spacing", &mut spec.spacing),
        ("line_height", &mut spec.line_height),
    ] {
        if let Some(v) = take(key) {
            match number(&v) {
                Some(n) => *slot = n as f32,
                None => errors.push(format!("`{key}` を解釈できません: {v}")),
            }
        }
    }
    if let Some(v) = take("width") {
        spec.max_width = number(&v).map(|n| n as f32);
    }
    if let Some(v) = take("typewriter") {
        spec.typewriter = Some(if v.is_empty() {
            0.06
        } else {
            seconds(&v).unwrap_or(0.06)
        });
    }
    errors
}

/// `"a\nb"` のエスケープを解く。
fn text_value(v: &str) -> String {
    unquote(v.trim()).replace("\\n", "\n")
}

fn parse_setting(
    scene: &mut Scene,
    key: &str,
    value: &str,
    attrs: Option<&str>,
    ctx: &mut Ctx<'_>,
    last_bg: &mut Option<(String, Props)>,
) {
    let mut attrs = attrs.map(Attrs::parse).unwrap_or_default();
    let simple_number = |value: &str, key: &str, ctx: &mut Ctx<'_>| -> Option<f64> {
        match seconds(value) {
            Some(v) if v >= 0.0 => Some(v),
            _ => {
                ctx.warn(format!(
                    "`{key}` には 0 以上の数値を指定してください: {value}"
                ));
                None
            }
        }
    };
    match key {
        "bg" => {
            let props = ctx.props(&mut attrs);
            ctx.leftovers(&attrs, "背景");
            scene.background = Some(value.to_owned());
            scene.background_props = props.clone();
            *last_bg = Some((value.to_owned(), props));
        }
        "before_pause" => scene.before_pause = simple_number(value, key, ctx).unwrap_or(0.0),
        "after_pause" => scene.after_pause = simple_number(value, key, ctx).unwrap_or(0.0),
        "length" => scene.length = simple_number(value, key, ctx),
        "speed" => scene.speed = simple_number(value, key, ctx).filter(|v| *v > 0.0),
        "overlay" | "image" => {
            let props = ctx.props(&mut attrs);
            ctx.leftovers(&attrs, "画像");
            scene.items.push(Item {
                kind: ItemKind::Image(value.to_owned()),
                props,
                source_line: ctx.no,
            });
        }
        "text" => {
            let props = ctx.props(&mut attrs);
            let mut spec = TextSpec {
                text: text_value(value),
                ..TextSpec::default()
            };
            for e in take_text_style(&mut spec, &mut attrs) {
                ctx.warn(e);
            }
            ctx.leftovers(&attrs, "テキスト");
            scene.items.push(Item {
                kind: ItemKind::Text(spec),
                props,
                source_line: ctx.no,
            });
        }
        "video" => {
            let props = ctx.props(&mut attrs);
            let spec = VideoSpec {
                path: value.to_owned(),
                volume: ctx.num(&mut attrs, "volume").unwrap_or(1.0),
                speed: ctx
                    .num(&mut attrs, "speed")
                    .filter(|s| *s > 0.0)
                    .unwrap_or(1.0),
                trim: ctx.secs(&mut attrs, "trim").unwrap_or(0.0),
                looped: attrs.0.remove("loop").is_some(),
            };
            let spec = if attrs.0.remove("mute").is_some() {
                VideoSpec {
                    volume: 0.0,
                    ..spec
                }
            } else {
                spec
            };
            ctx.leftovers(&attrs, "動画");
            scene.items.push(Item {
                kind: ItemKind::Video(spec),
                props,
                source_line: ctx.no,
            });
        }
        "shape" => {
            let shape = match value {
                "rect" | "四角形" | "box" => ShapeKind::Rect,
                "ellipse" | "circle" | "円" => ShapeKind::Ellipse,
                "triangle" | "三角形" => ShapeKind::Triangle,
                "star" | "星" => ShapeKind::Star,
                "line" | "線" => ShapeKind::Line,
                _ => {
                    ctx.warn(format!(
                        "未知の図形 `{value}`（rect / ellipse / triangle / star / line）"
                    ));
                    return;
                }
            };
            let props = ctx.props(&mut attrs);
            let width = ctx
                .num(&mut attrs, "w")
                .or_else(|| ctx.num(&mut attrs, "width"))
                .unwrap_or(400.0) as f32;
            let height = ctx
                .num(&mut attrs, "h")
                .or_else(|| ctx.num(&mut attrs, "height"))
                .unwrap_or(width as f64) as f32;
            let fill = ctx
                .color(&mut attrs, "color")
                .or(Some(Rgba::WHITE))
                .filter(|_| attrs.0.remove("nofill").is_none());
            let stroke = attrs.0.remove("stroke").and_then(|v| {
                let s = width_color(&v);
                if s.is_none() {
                    ctx.warn(format!("`stroke` は `太さ 色` で指定してください: {v}"));
                }
                s
            });
            let radius = ctx.num(&mut attrs, "radius").unwrap_or(0.0) as f32;
            ctx.leftovers(&attrs, "図形");
            scene.items.push(Item {
                kind: ItemKind::Shape(ShapeSpec {
                    shape,
                    width,
                    height,
                    fill,
                    stroke,
                    radius,
                }),
                props,
                source_line: ctx.no,
            });
        }
        "transition" => {
            let mut it = value.split_whitespace();
            let Some(kind) = it.next().and_then(TransitionKind::parse) else {
                ctx.warn(format!(
                    "未知の場面切り替え `{value}`（fade / black / white / wipe-left / slide-left / circle / zoom / mosaic など）"
                ));
                return;
            };
            let mut t = Transition {
                kind,
                duration: 0.5,
                ease: Ease::InOut,
            };
            for tok in it {
                if let Some(e) = Ease::parse(tok) {
                    t.ease = e;
                } else if let Some(d) = seconds(tok) {
                    t.duration = d.max(0.0);
                } else {
                    ctx.warn(format!("場面切り替えの指定を解釈できません: {tok}"));
                }
            }
            scene.transition = Some(t);
        }
        "camera" => {
            // `- camera: zoom 1->1.2` の短縮形も受け付ける
            if let Some((k, v)) = value.split_once(' ') {
                attrs.0.insert(k.to_owned(), v.to_owned());
            }
            let props = ctx.props(&mut attrs);
            ctx.leftovers(&attrs, "カメラ");
            scene.camera = Some(props);
        }
        "bgm" => {
            let bgm = Bgm {
                path: value.to_owned(),
                volume: ctx.num(&mut attrs, "volume"),
                fade_in: ctx.secs(&mut attrs, "fade_in").unwrap_or(0.0),
                fade_out: ctx.secs(&mut attrs, "fade_out"),
                at: ctx.scene_time(&mut attrs, "at"),
                until: ctx.scene_time(&mut attrs, "until"),
                keep: attrs.0.remove("continue").is_some(),
            };
            ctx.leftovers(&attrs, "BGM");
            scene.bgm.push(bgm);
        }
        "sound" | "se" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let Some(path) = parts.first() else {
                ctx.warn("効果音のファイルを指定してください");
                return;
            };
            let positional = |i: usize| parts.get(i).and_then(|v| seconds(v));
            if parts.len() > 3 || parts.iter().skip(1).any(|p| seconds(p).is_none()) {
                ctx.warn("`sound: パス [長さ] [開始秒]` の形式で書いてください");
                return;
            }
            let duration = ctx
                .secs(&mut attrs, "dur")
                .or(positional(1))
                .filter(|d| *d > 0.0);
            let at = ctx
                .scene_time(&mut attrs, "at")
                .or(positional(2).map(SceneTime::Secs))
                .unwrap_or(SceneTime::Secs(0.0));
            let volume = ctx.num(&mut attrs, "volume").unwrap_or(1.0);
            ctx.leftovers(&attrs, "効果音");
            scene.sounds.push(Sound {
                path: (*path).to_owned(),
                duration,
                at,
                volume,
            });
        }
        "id" => match parse_character(value) {
            Some((id, variation, effect)) => {
                if let Some(Effect::Other(name)) = &effect {
                    ctx.warn(format!(
                        "未知のエフェクト `{name}`（bouncing / shaking / grayscale / shatter）"
                    ));
                }
                let props = ctx.props(&mut attrs);
                ctx.leftovers(&attrs, "立ち絵");
                scene.characters.push(CharacterRef {
                    id,
                    variation,
                    effect,
                    props,
                });
            }
            None => ctx.warn(format!("キャラクター指定を解釈できません: {value}")),
        },
        other => ctx.warn(format!("未知の設定キー `{other}`")),
    }
}

fn apply_line_attrs(line: &mut Line, body: &str, ctx: &mut Ctx<'_>) {
    let mut attrs = Attrs::parse(body);
    if let Some(v) = attrs.0.remove("face") {
        line.face = Some(v);
    }
    if let Some(v) = attrs.0.remove("read") {
        line.read = v;
    }
    if let Some(v) = attrs.0.remove("subtitle") {
        line.show_subtitle = !matches!(v.as_str(), "off" | "false" | "no" | "0");
    }
    line.speed = ctx.num(&mut attrs, "speed").filter(|v| *v > 0.0);
    line.pitch = ctx.num(&mut attrs, "pitch").filter(|v| *v > 0.0);
    line.volume = ctx.num(&mut attrs, "volume").filter(|v| *v >= 0.0);
    line.pre = ctx.secs(&mut attrs, "pre").unwrap_or(0.0);
    line.wait = ctx.secs(&mut attrs, "wait").unwrap_or(0.0);
    line.motion = ctx.fx(&mut attrs, "motion");
    line.se = attrs.0.remove("se");
    line.instruct = attrs.0.remove("instruct");
    ctx.leftovers(&attrs, "セリフ");
}

pub fn parse_script(text: &str) -> Parsed {
    let mut diagnostics = Vec::new();
    let mut scenes: Vec<Scene> = Vec::new();
    let mut last_bg: Option<(String, Props)> = None;
    let mut in_lines = false;

    for (no, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if is_scene_header(line) {
            let (bg, bg_props) = last_bg
                .clone()
                .map_or((None, Props::default()), |(b, p)| (Some(b), p));
            scenes.push(Scene {
                id: scenes.len() + 1,
                name: scene_name(line),
                background: bg,
                background_props: bg_props,
                source_line: no,
                ..Scene::default()
            });
            in_lines = false;
            continue;
        }
        let Some(scene) = scenes.last_mut() else {
            continue; // 最初の `# Scene` より前は Python 版と同じく無視する
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("<!--") || trimmed.starts_with("//") {
            continue;
        }
        let mut ctx = Ctx {
            no,
            diags: &mut diagnostics,
        };
        if trimmed.starts_with("## Lines") {
            in_lines = true;
            continue;
        }
        if in_lines {
            match parse_dialogue(trimmed, no) {
                Some((mut l, attrs)) => {
                    if let Some(a) = attrs {
                        apply_line_attrs(&mut l, a, &mut ctx);
                    }
                    scene.lines.push(l);
                }
                None => ctx.warn("セリフは `キャラID: セリフ` の形式で書いてください"),
            }
            continue;
        }
        let Some(item) = trimmed.strip_prefix('-') else {
            continue;
        };
        let (item, attrs) = split_attrs(item);
        let Some((key, value)) = item.split_once(':') else {
            // `- camera {zoom: 1.2}` のように値がない形
            let key = item.trim();
            if matches!(key, "camera") {
                parse_setting(scene, key, "", attrs, &mut ctx, &mut last_bg);
            } else {
                ctx.warn("設定は `- キー: 値` の形式で書いてください");
            }
            continue;
        };
        parse_setting(
            scene,
            key.trim(),
            value.trim(),
            attrs,
            &mut ctx,
            &mut last_bg,
        );
    }
    Parsed {
        script: Script { scenes },
        diagnostics,
    }
}

/// 数値アニメーションの短縮（テスト・GUI 用）。
pub fn anim(s: &str) -> Option<Anim> {
    Anim::parse(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attrs::{Blend, MotionKind};

    const SAMPLE: &str = "前置きは無視\n# Scene 0\n\n- bg: data/bg/a.jpg\n- after_pause: 1\n- id: zundamon@default_left.bouncing\n- id: metan@default\n\n# Scene@第一章：冒険の始まり\n\n- id: zundamon\n- speed: 1.3\n- sound: data/sound/a.mp3 1\n- sound: data/sound/b.mp3 0.5 2\n- bgm: data/bgm/001.mp3\n\n## Lines\nzundamon: こんにちはなのだ。\nmetan@noscript: こんにちは。\nzundamon:空白なしも可\ntumugi：全角コロン\nこれは不正\n";

    #[test]
    fn parses_python_compatible_script() {
        let p = parse_script(SAMPLE);
        let s = &p.script.scenes;
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, 1);
        assert_eq!(s[0].name, None);
        assert_eq!(s[0].after_pause, 1.0);
        assert_eq!(s[0].characters[0].variation, "default_left");
        assert_eq!(s[0].characters[0].effect, Some(Effect::Bouncing));
        assert_eq!(s[1].name.as_deref(), Some("第一章：冒険の始まり"));
        assert_eq!(s[1].background.as_deref(), Some("data/bg/a.jpg"));
        assert_eq!(s[1].characters[0].variation, "default");
        assert_eq!(s[1].speed, Some(1.3));
        assert_eq!(s[1].sounds[0].duration, Some(1.0));
        assert_eq!(s[1].sounds[1].at, SceneTime::Secs(2.0));
        assert_eq!(s[1].bgm[0].path, "data/bgm/001.mp3");
        let lines = &s[1].lines;
        assert_eq!(lines.len(), 4);
        assert!(!lines[1].show_subtitle);
        assert_eq!(lines[2].text, "空白なしも可");
        assert_eq!(lines[3].speaker, "tumugi");
        assert_eq!(p.diagnostics.len(), 1);
        assert_eq!(p.diagnostics[0].line, 21);
        assert_eq!(p.script.speakers(), ["zundamon", "metan", "tumugi"]);
    }

    #[test]
    fn reports_bad_values_without_failing() {
        let p = parse_script(
            "# Scene\n- before_pause: x\n- foo: 1\n- id: zundamon.wobble\n- sound: a.mp3 x\n",
        );
        assert_eq!(p.script.scenes[0].before_pause, 0.0);
        assert_eq!(p.diagnostics.len(), 4);
    }

    #[test]
    fn crlf_and_unicode_ids() {
        let p =
            parse_script("# Scene@A\r\n- id: ずんだもん@笑顔\r\n## Lines\r\nずんだもん: やあ\r\n");
        let s = &p.script.scenes[0];
        assert_eq!(s.name.as_deref(), Some("A"));
        assert_eq!(s.characters[0].id, "ずんだもん");
        assert_eq!(s.lines[0].text, "やあ");
    }

    #[test]
    fn ruby_separates_display_and_reading() {
        assert_eq!(
            split_ruby("｜錯誤《さくご》とは"),
            ("錯誤とは".into(), "さくごとは".into())
        );
        assert_eq!(
            split_ruby("民法《みんぽう》の話"),
            ("民法の話".into(), "みんぽうの話".into())
        );
        assert_eq!(split_ruby("A|B"), ("A|B".into(), "A|B".into()));
    }

    #[test]
    fn parses_extended_items_and_line_attributes() {
        let p = parse_script(concat!(
            "# Scene@拡張\n",
            "- bg: bg.png {scale: 1->1.1, fx: blur(2)}\n",
            "- transition: wipe-left 0.8 ease-out\n",
            "- camera: {zoom: 1->1.2, fx: shake(3)}\n",
            "- length: 6\n",
            "- id: zundamon@default_left {x: -500, flip, in: slide-left 0.4, layer: 12}\n",
            "- text: \"タイトル\\n2行目\" {y: -380, size: 90, color: #ffee00, outline: 8 #000 | 14 #fff, shadow: 4 4 6, at: L2, until: end-0.5, in: pop, typewriter: 0.05, bold}\n",
            "- image: data/overlay/a.png {scale: 80%, rotate: 0 10 0, blend: screen, fx: shadow(6,6) glow(10)}\n",
            "- video: clip.mp4 {volume: 0.5, trim: 2, loop}\n",
            "- shape: rect {w: 600, h: 120, color: #00000080, radius: 20, stroke: 4 white, layer: 40}\n",
            "- bgm: b.mp3 {volume: 0.3, fade_in: 1, fade_out: 2, continue}\n",
            "- sound: s.mp3 {at: L2+0.3, volume: 0.7}\n",
            "## Lines\n",
            "zundamon@smile: ｜詐欺《さぎ》なのだ！ {pitch: 1.1, volume: 1.2, wait: 0.5, motion: bounce(30), se: pop.mp3}\n",
            "metan: 了解です {face: komatta, speed: 0.9, subtitle: off, instruct: 落ち着いて}\n",
            "metan: 変な属性 {nope: 1}\n",
        ));
        assert_eq!(
            p.diagnostics.iter().map(|d| d.line).collect::<Vec<_>>(),
            [15],
            "{:?}",
            p.diagnostics
        );
        let s = &p.script.scenes[0];
        assert!(s.background_props.scale.is_some() && s.background_props.fx.len() == 1);
        let t = s.transition.unwrap();
        assert_eq!(
            (t.kind, t.duration, t.ease),
            (TransitionKind::WipeLeft, 0.8, Ease::Out)
        );
        assert_eq!(s.camera.as_ref().unwrap().fx.len(), 1);
        assert_eq!(s.length, Some(6.0));
        let c = &s.characters[0];
        assert!(c.props.flip && c.props.layer == Some(12));
        assert_eq!(c.props.enter.unwrap().kind, MotionKind::SlideLeft);
        assert_eq!(s.items.len(), 4);
        let ItemKind::Text(text) = &s.items[0].kind else {
            panic!()
        };
        assert_eq!(text.text, "タイトル\n2行目");
        assert_eq!(text.outlines.len(), 2);
        assert!(text.bold && text.shadow.is_some() && text.typewriter == Some(0.05));
        assert_eq!(s.items[0].props.at, Some(SceneTime::LineStart(1, 0.0)));
        assert_eq!(s.items[1].props.blend, Blend::Screen);
        assert_eq!(s.items[1].props.fx.len(), 2);
        let ItemKind::Video(v) = &s.items[2].kind else {
            panic!()
        };
        assert!(v.looped && v.trim == 2.0 && v.volume == 0.5);
        let ItemKind::Shape(sh) = &s.items[3].kind else {
            panic!()
        };
        assert_eq!((sh.width, sh.height, sh.radius), (600.0, 120.0, 20.0));
        assert_eq!(s.bgm[0].volume, Some(0.3));
        assert!(s.bgm[0].keep);
        assert_eq!(s.sounds[0].at, SceneTime::LineStart(1, 0.3));
        let l = &s.lines[0];
        assert_eq!(
            (l.text.as_str(), l.read.as_str()),
            ("詐欺なのだ！", "さぎなのだ！")
        );
        assert_eq!(l.face.as_deref(), Some("smile"));
        assert_eq!((l.pitch, l.volume, l.wait), (Some(1.1), Some(1.2), 0.5));
        assert_eq!(l.motion.len(), 1);
        assert_eq!(l.se.as_deref(), Some("pop.mp3"));
        let m = &s.lines[1];
        assert!(!m.show_subtitle);
        assert_eq!(m.face.as_deref(), Some("komatta"));
        assert_eq!(m.instruct.as_deref(), Some("落ち着いて"));
    }
}
