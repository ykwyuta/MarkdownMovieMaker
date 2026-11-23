//! フレームの合成。
//!
//! 1 フレームは「背景 → レイヤー順のアイテム（立ち絵・画像・動画・図形・テキスト）→ カメラ →
//! 字幕 → シーン名」の順に重ね、シーンの始まりでは場面切り替えで前のシーンと混ぜる。
//! 立ち絵の既定のレイヤーは 10 で、話しているキャラクターが同じレイヤーの中で一番手前になる
//! （Python 版 composer.py と同じ Z 順）。
//!
//! 座標は基準解像度 1920×1080 のピクセル。アイテムの `x`・`y` は画面中央が原点（YMM4 と同じ）、
//! キャラクター定義の `pos` は Python 版と同じく立ち絵の左上。出力解像度が違えば全体を拡大縮小する。
mod assets;
pub mod fx;
pub mod lipsync;
pub mod raster;
pub mod shape;
pub mod shatter;
pub mod text;
pub mod transition;
pub mod video;

pub use assets::Assets;
pub use lipsync::{LipSync, Mouth};
pub use text::TextRenderer;

use image::RgbaImage;
use mmm_core::{
    Character, CharacterMaster, Effect, Fx, ItemKind, Line, MotionKind, Props, Rgba, Script,
    SegmentKind, SubtitleStyle, TextBox, TextSpec, Timeline, Variation,
};
use raster::Placement;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// 埋め込みの既定フォント（M+ 1 Bold, OFL）。
pub static DEFAULT_FONT: &[u8] = include_bytes!("../assets/Mplus1-Bold.ttf");

/// 座標系の基準サイズ（Python 版は背景画像の大きさ = 1920×1080）。
pub const DESIGN_WIDTH: f32 = 1920.0;
pub const DESIGN_HEIGHT: f32 = 1080.0;

/// 立ち絵・字幕・シーン名の既定レイヤー。
pub const CHARACTER_LAYER: i32 = 10;
pub const SUBTITLE_LAYER: i32 = 100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("フォントを読めません: {0}")]
    Font(String),
}

/// エフェクト済み画像のキャッシュ（元画像を保持して、ポインタのキーが再利用されないようにする）。
type FxEntry = (Arc<RgbaImage>, Arc<RgbaImage>);

/// 描く 1 つの要素（重ねる順に並べ替えてから描く）。
struct Element {
    order: (i32, f32),
    image: Arc<RgbaImage>,
    /// 出力上の中心（変形前）。
    cx: f32,
    cy: f32,
    props: Props,
    /// アイテム内の経過秒と長さ。
    t: f64,
    duration: f64,
    /// 話している間だけのエフェクトなど、props 以外に足すもの。
    extra_fx: Vec<Fx>,
    /// 元画像に掛けた倍率（props の拡大率はこれに掛ける）。
    seed: u64,
}

pub struct Renderer {
    pub master: CharacterMaster,
    pub assets: Assets,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub text: TextRenderer,
    fx_cache: Mutex<HashMap<(usize, String), FxEntry>>,
    parts_cache: Mutex<HashMap<String, Arc<RgbaImage>>>,
    videos: Mutex<HashMap<String, Option<Arc<video::Video>>>>,
    /// 直前のフレームと同じ絵なら使い回す（連続したフレームを描く書き出し向け）。
    reuse_frames: std::sync::atomic::AtomicBool,
    id: u64,
    one_pixel: Arc<RgbaImage>,
}

/// フレームの見た目を決めるものの要約。画像は Arc を持ち続けてアドレスの再利用を防ぐ。
struct FrameKey {
    renderer: u64,
    hash: u64,
    images: Vec<Arc<RgbaImage>>,
}

impl PartialEq for FrameKey {
    fn eq(&self, other: &Self) -> bool {
        self.renderer == other.renderer
            && self.hash == other.hash
            && self.images.len() == other.images.len()
            && self
                .images
                .iter()
                .zip(&other.images)
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

thread_local! {
    /// スレッドごとの直前のフレーム（書き出しは各スレッドが連続したフレームを描く）。
    static LAST_FRAME: std::cell::RefCell<Option<(FrameKey, Arc<RgbaImage>)>> = const { std::cell::RefCell::new(None) };
}

static RENDERER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn seed_of(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Renderer {
    /// `base_dir` は台本中の相対パス（`data/bg/...`）の基準ディレクトリ。
    pub fn new(
        master: CharacterMaster,
        base_dir: impl Into<PathBuf>,
        font: Option<Vec<u8>>,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let base = base_dir.into();
        Ok(Self {
            master,
            text: TextRenderer::new(font, base.clone())?,
            assets: Assets::new(base),
            width,
            height,
            fps: 30.0,
            fx_cache: Mutex::default(),
            parts_cache: Mutex::default(),
            videos: Mutex::default(),
            reuse_frames: std::sync::atomic::AtomicBool::new(false),
            id: RENDERER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            one_pixel: Arc::new(RgbaImage::new(1, 1)),
        })
    }

    /// 直前のフレームと同じ絵を使い回すか（書き出しで有効にする。プレビューは時刻が飛ぶので無効）。
    pub fn set_reuse_frames(&self, on: bool) {
        self.reuse_frames
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn k(&self) -> f32 {
        self.height as f32 / DESIGN_HEIGHT
    }

    /// 画面中央原点の基準座標 → 出力ピクセル。
    fn to_out(&self, x: f32, y: f32) -> (f32, f32) {
        let k = self.k();
        (
            self.width as f32 / 2.0 + x * k,
            self.height as f32 / 2.0 + y * k,
        )
    }

    /// タイムラインの時刻 `time` のフレーム。
    pub fn frame(
        &self,
        script: &Script,
        timeline: &Timeline,
        lipsync: &LipSync,
        time: f64,
    ) -> RgbaImage {
        let f = self.frame_shared(script, timeline, lipsync, time);
        Arc::try_unwrap(f).unwrap_or_else(|f| (*f).clone())
    }

    /// [`Self::frame`] の共有版。使い回したフレームは直前と同じ `Arc` を返すので、
    /// 呼び出し側は `Arc::ptr_eq` で「前と同じ絵」と分かる（YUV 変換も省ける）。
    pub fn frame_shared(
        &self,
        script: &Script,
        timeline: &Timeline,
        lipsync: &LipSync,
        time: f64,
    ) -> Arc<RgbaImage> {
        let Some((seg, _)) = timeline.at(time) else {
            return Arc::new(self.blank());
        };
        let si = seg.scene;
        let current = self.scene_frame(script, timeline, lipsync, si, time);
        let scene = &script.scenes[si];
        let span = &timeline.scenes[si];
        if let Some(tr) = scene.transition
            && time - span.start < tr.duration
            && let Some(prev) = (0..si)
                .rev()
                .find(|&p| timeline.scenes[p].end > timeline.scenes[p].start)
        {
            let before = self.scene_frame(
                script,
                timeline,
                lipsync,
                prev,
                timeline.scenes[prev].end - 1e-4,
            );
            let p = tr.ease.apply((time - span.start) / tr.duration.max(1e-6)) as f32;
            return Arc::new(transition::blend(&before, &current, tr.kind, p, self.k()));
        }
        current
    }

    /// シーン `si` の静止画（`--image-only`・シーン一覧）。セリフの途中、または場面の中ほど。
    pub fn still(
        &self,
        script: &Script,
        timeline: &Timeline,
        lipsync: &LipSync,
        si: usize,
        line: Option<usize>,
    ) -> RgbaImage {
        let span = &timeline.scenes[si];
        let t = match line.and_then(|l| span.lines.get(l).copied().flatten()) {
            Some((a, b)) => a + (b - a) * 0.5,
            None => span.start + (span.end - span.start) * 0.5,
        };
        let f = self.scene_frame(script, timeline, lipsync, si, t);
        Arc::try_unwrap(f).unwrap_or_else(|f| (*f).clone())
    }

    fn blank(&self) -> RgbaImage {
        RgbaImage::from_pixel(self.width, self.height, image::Rgba([0, 0, 0, 255]))
    }

    fn scene_frame(
        &self,
        script: &Script,
        timeline: &Timeline,
        lipsync: &LipSync,
        si: usize,
        time: f64,
    ) -> Arc<RgbaImage> {
        let scene = &script.scenes[si];
        let span = &timeline.scenes[si];
        let k = self.k();
        let local = time - span.start;
        let scene_len = span.end - span.start;
        // 話しているセリフ
        let speaking = span.lines.iter().enumerate().find_map(|(li, r)| {
            r.filter(|(a, b)| time >= *a && time < *b)
                .map(|(a, b)| (li, time - a, b - a))
        });
        let speaking_line = speaking.map(|(li, ..)| &scene.lines[li]);
        // 字幕と口パクを出す区間（セリフ前後の間は話していない扱い）
        // 背景（そのまま使えるなら背景の複製から始めて、黒で塗る手間を省く）
        let background = scene
            .background
            .as_ref()
            .and_then(|bg| Some((bg, self.assets.background(bg, self.width, self.height)?)));
        let plain = scene.background_props == Props::default();
        let bg_element = background
            .as_ref()
            .filter(|_| !plain)
            .map(|(bg, img)| Element {
                order: (i32::MIN, 0.0),
                image: img.clone(),
                cx: self.width as f32 / 2.0,
                cy: self.height as f32 / 2.0,
                props: scene.background_props.clone(),
                t: local,
                duration: scene_len,
                extra_fx: Vec::new(),
                seed: seed_of(bg),
            });

        let mut elements: Vec<Element> = Vec::new();
        // 立ち絵（話していない人 → 話している人）
        let speaker = speaking_line.map(|l| l.speaker.as_str());
        let mut chars: Vec<(&str, &str, Option<&Effect>, Props, usize)> = scene
            .characters
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    c.id.as_str(),
                    c.variation.as_str(),
                    c.effect.as_ref(),
                    c.props.clone(),
                    i,
                )
            })
            .collect();
        if let Some(id) = speaker
            && !chars.iter().any(|c| c.0 == id)
        {
            chars.push((id, "default", None, Props::default(), chars.len()));
        }
        for (id, variation, effect, props, order) in chars {
            let Some(character) = self.master.get(id) else {
                continue;
            };
            let is_speaker = Some(id) == speaker;
            let face = if is_speaker {
                speaking_line.and_then(|l| l.face.as_deref())
            } else {
                None
            };
            let Some(v) = face
                .and_then(|f| character.variations.get(f))
                .or_else(|| character.variation(variation))
            else {
                continue;
            };
            let (item_start, item_end) = timeline.item_span(si, &props);
            if time < item_start || time >= item_end.max(item_start + 1e-6) && item_end < span.end {
                continue;
            }
            let (mouth, mouth_open) = match speaking {
                Some((li, lt, ld)) if is_speaker => (
                    lipsync.mouth(si, li, lt, ld),
                    lipsync.mouth_open(si, li, lt, ld),
                ),
                _ => (Mouth::Closed, false),
            };
            let seed = seed_of(id);
            let Some((img, w, h)) =
                self.character_image(character, v, mouth, mouth_open, time, seed)
            else {
                continue;
            };
            let center_x = v.pos[0] as f32 + w / 2.0;
            let center_y = v.pos[1] as f32 + h / 2.0;
            let mut extra_fx = Vec::new();
            match effect {
                Some(Effect::Bouncing) if is_speaker => extra_fx.push(Fx::Bounce {
                    height: 20.0,
                    speed: 5.0,
                }),
                Some(Effect::Shaking) => extra_fx.push(Fx::Shake {
                    amount: 8.0,
                    speed: self.fps as f32,
                }),
                Some(Effect::Grayscale) => extra_fx.push(Fx::Grayscale),
                _ => {}
            }
            if is_speaker && let Some(l) = speaking_line {
                extra_fx.extend(l.motion.iter().cloned());
            }
            let mut props = props;
            if effect == Some(&Effect::Shatter) && props.exit.is_none() {
                // Python 版の `.shatter`（モザイク＋フェード）を本物の破砕に置き換える
                props.exit = Some(mmm_core::Motion {
                    kind: MotionKind::Shatter,
                    duration: 0.8,
                    ease: mmm_core::Ease::Linear,
                });
            }
            let (cx, cy) = (center_x * k, center_y * k);
            elements.push(Element {
                order: (
                    props.layer.unwrap_or(CHARACTER_LAYER),
                    if is_speaker {
                        0.5
                    } else {
                        order as f32 * 0.001
                    },
                ),
                image: img,
                cx,
                cy,
                props,
                t: time - item_start,
                duration: item_end - item_start,
                extra_fx,
                seed,
            });
        }

        // アイテム
        for (ii, item) in scene.items.iter().enumerate() {
            let (a, b) = timeline.item_span(si, &item.props);
            if time < a || time >= b {
                continue;
            }
            let t = time - a;
            let image = match &item.kind {
                ItemKind::Image(path) => self.assets.image(path, k, t),
                ItemKind::Text(spec) => {
                    let visible = spec.typewriter.map(|tw| (t / tw.max(1e-3)) as usize);
                    Some(self.text.render(spec, k, visible))
                }
                ItemKind::Shape(spec) => Some(self.shape(spec, k)),
                ItemKind::Video(v) => self.video_frame(v, t, k),
            };
            let Some(image) = image else { continue };
            let (cx, cy) = self.to_out(0.0, 0.0);
            elements.push(Element {
                order: (
                    item.props.layer.unwrap_or(item.kind.default_layer()),
                    0.01 * ii as f32,
                ),
                image,
                cx,
                cy,
                props: item.props.clone(),
                t,
                duration: b - a,
                extra_fx: Vec::new(),
                seed: item.source_line as u64,
            });
        }

        elements.sort_by(|a, b| {
            a.order
                .0
                .cmp(&b.order.0)
                .then(a.order.1.total_cmp(&b.order.1))
        });
        let (under, over): (Vec<_>, Vec<_>) = elements
            .into_iter()
            .partition(|e| e.order.0 < SUBTITLE_LAYER);
        let in_line = timeline
            .at(time)
            .is_some_and(|(s, _)| s.scene == si && matches!(s.kind, SegmentKind::Line(_)));
        let subtitle = match (speaking, speaking_line) {
            (Some((li, lt, _)), Some(line)) if line.show_subtitle => self
                .master
                .get(&line.speaker)
                .and_then(|c| c.subtitle_style.as_ref())
                .map(|style| (li, lt, style)),
            _ => None,
        };
        // 直前のフレームと同じ絵なら描かずに使い回す（書き出し時）
        let key = self
            .reuse_frames
            .load(std::sync::atomic::Ordering::Relaxed)
            .then(|| {
                let mut images: Vec<Arc<RgbaImage>> = Vec::new();
                let mut h = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                (si, in_line, plain).hash(&mut h);
                if let Some((_, img)) = &background {
                    images.push(img.clone());
                }
                for e in bg_element.iter().chain(&under).chain(&over) {
                    images.push(e.image.clone());
                    self.hash_element(e, &mut h);
                }
                if let Some(cam) = &scene.camera {
                    let el = Element {
                        order: (0, 0.0),
                        image: self.one_pixel.clone(),
                        cx: self.width as f32 / 2.0,
                        cy: self.height as f32 / 2.0,
                        props: cam.clone(),
                        t: local,
                        duration: scene_len,
                        extra_fx: Vec::new(),
                        seed: si as u64,
                    };
                    self.hash_element(&el, &mut h);
                }
                if let Some((li, lt, style)) = subtitle {
                    (li, style.typewriter.map(|tw| (lt / tw.max(1e-3)) as usize)).hash(&mut h);
                    // 編集中のプレビューでも正しいよう、字幕の文と見た目も鍵に入れる
                    speaking_line.map(|l| (&l.text, &l.speaker)).hash(&mut h);
                    format!("{style:?}").hash(&mut h);
                }
                scene.name.hash(&mut h);
                FrameKey {
                    renderer: self.id,
                    hash: h.finish(),
                    images,
                }
            });
        if let Some(key) = &key
            && let Some(frame) = LAST_FRAME.with(|l| {
                l.borrow()
                    .as_ref()
                    .filter(|(k, _)| k == key)
                    .map(|(_, f)| f.clone())
            })
        {
            return frame;
        }
        let mut canvas = match &background {
            Some((_, img)) if plain => (**img).clone(),
            _ => self.blank(),
        };
        if let Some(el) = &bg_element {
            self.draw_element(&mut canvas, el);
        }
        for e in &under {
            self.draw_element(&mut canvas, e);
        }
        // カメラ（字幕より下の全体を動かす）
        if let Some(cam) = &scene.camera {
            let el = Element {
                order: (0, 0.0),
                image: Arc::new(std::mem::replace(&mut canvas, self.blank())),
                cx: self.width as f32 / 2.0,
                cy: self.height as f32 / 2.0,
                props: cam.clone(),
                t: local,
                duration: scene_len,
                extra_fx: Vec::new(),
                seed: si as u64,
            };
            self.draw_element(&mut canvas, &el);
        }
        // 字幕
        if let (Some((_, lt, style)), Some(line)) = (subtitle, speaking_line) {
            self.draw_subtitle(&mut canvas, line, style, lt);
        }
        for e in &over {
            self.draw_element(&mut canvas, e);
        }
        // シーン名（Python 版と同じくセリフの区間だけ）
        if in_line && let Some(name) = &scene.name {
            let spec = TextSpec {
                text: name.clone(),
                size: 40.0,
                color: Rgba::WHITE,
                boxed: Some(TextBox {
                    fill: Rgba([0, 0, 0, 128]),
                    padding: 10.0,
                    radius: 5.0,
                    border: 2.0,
                    border_color: Rgba::BLACK,
                }),
                ..TextSpec::default()
            };
            let img = self.text.render(&spec, k, None);
            let m = 10.0 * k;
            raster::draw(
                &mut canvas,
                &img,
                &Placement::at(m + img.width() as f32 / 2.0, m + img.height() as f32 / 2.0),
            );
        }
        let canvas = Arc::new(canvas);
        if let Some(key) = key {
            LAST_FRAME.with(|l| *l.borrow_mut() = Some((key, canvas.clone())));
        }
        canvas
    }

    /// 要素の見た目を決めるもの（画像・配置・割れ具合・ぼかし）をハッシュに入れる。
    fn hash_element(&self, e: &Element, h: &mut std::collections::hash_map::DefaultHasher) {
        use std::hash::Hash;
        let all_fx: Vec<Fx> = e.props.fx.iter().chain(&e.extra_fx).cloned().collect();
        format!("{:?}", e.extra_fx).hash(h);
        if all_fx.iter().any(|f| matches!(f, Fx::Afterimage { .. })) {
            e.t.to_bits().hash(h); // 残像は過去の時刻にも依存する
        }
        match self.placement(e, &all_fx, e.t, &e.image) {
            Some(p) => [
                p.cx, p.cy, p.sx, p.sy, p.rotate, p.opacity, p.clip[0], p.clip[1], p.clip[2],
                p.clip[3],
            ]
            .map(f32::to_bits)
            .hash(h),
            None => 0u8.hash(h),
        }
        (
            self.shatter_amount(e, e.t).to_bits(),
            self.motion_blur(e, e.t).to_bits(),
            e.props.blend as u8,
        )
            .hash(h);
    }

    fn draw_subtitle(&self, canvas: &mut RgbaImage, line: &Line, style: &SubtitleStyle, t: f64) {
        let k = self.k();
        let inset = 2.0 * (style.padding + style.border_width);
        let spec = text::subtitle_spec(style, &line.text, DESIGN_WIDTH * 0.9 - inset);
        let visible = spec.typewriter.map(|tw| (t / tw.max(1e-3)) as usize);
        let img = self.text.render(&spec, k, visible);
        let bottom = style.bottom.unwrap_or(0.95) * self.height as f32;
        let p = Placement::at(
            self.width as f32 / 2.0,
            (bottom - img.height() as f32 / 2.0).round(),
        );
        raster::draw(canvas, &img, &p);
    }

    /// 立ち絵の画像（出力の大きさ）と、基準座標での幅・高さ。
    fn character_image(
        &self,
        c: &Character,
        v: &Variation,
        mouth: Mouth,
        mouth_open: bool,
        time: f64,
        seed: u64,
    ) -> Option<(Arc<RgbaImage>, f32, f32)> {
        let k = self.k();
        let h_design = DESIGN_HEIGHT * v.scale as f32;
        let height = (h_design * k).round().max(1.0) as u32;
        let img = match &v.base {
            Some(base) => {
                let eyes = if v.eyes.is_empty() {
                    None
                } else {
                    let i = lipsync::blink_frame(
                        time,
                        v.eyes.len(),
                        c.blink_interval.unwrap_or(4.0),
                        seed,
                    );
                    Some(v.eyes[i].as_str())
                };
                let mouth_path = mouth
                    .fallbacks()
                    .iter()
                    .find_map(|m| v.mouth.get(*m))
                    .or_else(|| v.mouth.get("n"))
                    .map(String::as_str);
                let parts: Vec<&str> = std::iter::once(base.as_str())
                    .chain(eyes)
                    .chain(mouth_path)
                    .collect();
                let key = format!("{}|{height}", parts.join("|"));
                if let Some(img) = self.parts_cache.lock().unwrap().get(&key) {
                    img.clone()
                } else {
                    let mut layers = parts
                        .iter()
                        .filter_map(|p| self.assets.sprite(p, height, time));
                    let mut out = (*layers.next()?).clone();
                    for l in layers {
                        image::imageops::overlay(&mut out, &*l, 0, 0);
                    }
                    let out = Arc::new(out);
                    let mut cache = self.parts_cache.lock().unwrap();
                    if cache.len() > 256 {
                        cache.clear();
                    }
                    cache.insert(key, out.clone());
                    out
                }
            }
            None => {
                let path = if mouth_open && !v.image_open.is_empty() {
                    &v.image_open
                } else {
                    &v.image_closed
                };
                self.assets.sprite(path, height, time)?
            }
        };
        let w_design = img.width() as f32 / k;
        Some((img, w_design, h_design))
    }

    fn shape(&self, spec: &mmm_core::ShapeSpec, k: f32) -> Arc<RgbaImage> {
        let key = format!("shape|{spec:?}|{k}");
        if let Some(img) = self.parts_cache.lock().unwrap().get(&key) {
            return img.clone();
        }
        let img = Arc::new(shape::render(spec, k));
        self.parts_cache.lock().unwrap().insert(key, img.clone());
        img
    }

    fn video_frame(&self, v: &mmm_core::VideoSpec, t: f64, k: f32) -> Option<Arc<RgbaImage>> {
        let video = {
            let mut videos = self.videos.lock().unwrap();
            videos
                .entry(v.path.clone())
                .or_insert_with(|| {
                    match video::Video::open(&self.assets.base_dir().join(&v.path)) {
                        Ok(vid) => Some(Arc::new(vid)),
                        Err(e) => {
                            self.assets.report_missing(&v.path, &e);
                            None
                        }
                    }
                })
                .clone()?
        };
        let mut pos = v.trim + t * v.speed;
        if v.looped && video.duration > 0.0 {
            pos %= video.duration;
        }
        let frame = match video.frame_at(pos) {
            Ok(f) => f,
            Err(e) => {
                self.assets.report_missing(&v.path, &e);
                return None;
            }
        };
        if (k - 1.0).abs() < 1e-3 {
            return Some(frame);
        }
        let (w, h) = (
            (frame.width() as f32 * k).round() as u32,
            (frame.height() as f32 * k).round() as u32,
        );
        Some(Arc::new(image::imageops::resize(
            &*frame,
            w.max(1),
            h.max(1),
            image::imageops::FilterType::Triangle,
        )))
    }

    /// 静的なエフェクトを掛けた画像（キャッシュする）。
    fn with_static_fx(&self, img: &Arc<RgbaImage>, list: &[Fx]) -> Arc<RgbaImage> {
        let statics: Vec<Fx> = list.iter().filter(|f| f.is_static()).cloned().collect();
        if statics.is_empty() {
            return img.clone();
        }
        let key = (Arc::as_ptr(img) as usize, format!("{statics:?}"));
        if let Some((_, out)) = self.fx_cache.lock().unwrap().get(&key) {
            return out.clone();
        }
        let out = Arc::new(fx::apply(img, &statics, self.k()));
        let mut cache = self.fx_cache.lock().unwrap();
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(key, (img.clone(), out.clone()));
        out
    }

    fn draw_element(&self, canvas: &mut RgbaImage, e: &Element) {
        let all_fx: Vec<Fx> = e.props.fx.iter().chain(&e.extra_fx).cloned().collect();
        let image = self.with_static_fx(&e.image, &all_fx);
        // 残像: 少し前の時刻の姿を薄く重ねる
        if let Some(Fx::Afterimage { count, interval }) =
            all_fx.iter().find(|f| matches!(f, Fx::Afterimage { .. }))
        {
            for i in (1..=*count).rev() {
                let t = e.t - f64::from(*interval) * f64::from(i);
                if t >= 0.0
                    && let Some(mut p) = self.placement(e, &all_fx, t, &image)
                {
                    p.opacity *= 0.5 * (1.0 - i as f32 / (*count + 1) as f32);
                    raster::draw(canvas, &image, &p);
                }
            }
        }
        if let Some(p) = self.placement(e, &all_fx, e.t, &image) {
            // 登場・退場の「割れる」は破片にして描く
            let broken = self.shatter_amount(e, e.t);
            if broken > 0.0 {
                shatter::draw(canvas, &image, &p, broken, e.seed ^ 0x5ea7);
                return;
            }
            // 登場・退場の「ぼかし」はその場でぼかす
            let blur = self.motion_blur(e, e.t);
            if blur > 0.5 {
                let b = fx::blur(&image, blur * self.k());
                raster::draw(canvas, &b, &p);
            } else {
                raster::draw(canvas, &image, &p);
            }
        }
    }

    /// 割れ具合（0 で無傷）。退場では割れていき、登場では破片が集まってくる。
    fn shatter_amount(&self, e: &Element, t: f64) -> f32 {
        let mut r: f32 = 0.0;
        for (m, entering) in [(e.props.enter, true), (e.props.exit, false)] {
            if let Some(m) = m.filter(|m| m.kind == MotionKind::Shatter)
                && let Some(q) = progress(m, t, e.duration, entering)
            {
                r = r.max(1.0 - q);
            }
        }
        r
    }

    fn motion_blur(&self, e: &Element, t: f64) -> f32 {
        let mut b = 0.0;
        for (m, entering) in [(e.props.enter, true), (e.props.exit, false)] {
            if let Some(m) = m.filter(|m| m.kind == MotionKind::Blur)
                && let Some(q) = progress(m, t, e.duration, entering)
            {
                b += 20.0 * (1.0 - q);
            }
        }
        b
    }

    /// アイテムの時刻 `t` での配置。
    fn placement(
        &self,
        e: &Element,
        all_fx: &[Fx],
        t: f64,
        image: &RgbaImage,
    ) -> Option<Placement> {
        let k = self.k();
        let p = &e.props;
        let d = e.duration;
        let val =
            |a: &Option<mmm_core::Anim>, default: f64| a.as_ref().map_or(default, |a| a.at(t, d));
        let (mut cx, mut cy) = (e.cx, e.cy);
        if let Some(x) = &p.x {
            cx = self.width as f32 / 2.0 + x.at(t, d) as f32 * k;
        }
        if let Some(y) = &p.y {
            cy = self.height as f32 / 2.0 + y.at(t, d) as f32 * k;
        }
        let scale = val(&p.scale, 1.0) as f32;
        let mut sx = scale * val(&p.scale_x, 1.0) as f32;
        let mut sy = scale * val(&p.scale_y, 1.0) as f32;
        let mut rotate = val(&p.rotate, 0.0) as f32;
        let mut opacity = val(&p.opacity, 1.0) as f32;
        let m = fx::motion(all_fx, t, e.seed);
        cx += m.dx * k;
        cy += m.dy * k;
        rotate += m.rotate;
        sx *= m.scale;
        sy *= m.scale;
        opacity *= m.opacity;
        let mut clip: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
        let (iw, ih) = (image.width() as f32, image.height() as f32);
        for (motion, entering) in [(p.enter, true), (p.exit, false)] {
            let Some(motion) = motion else { continue };
            let Some(q) = progress(motion, t, d, entering) else {
                continue;
            };
            let r = 1.0 - q; // 0 で通常の姿
            match motion.kind {
                MotionKind::Fade | MotionKind::Blur => opacity *= q,
                MotionKind::Zoom => {
                    sx *= q;
                    sy *= q;
                    opacity *= q.min(1.0);
                }
                MotionKind::Pop => {
                    let s = mmm_core::Ease::Back.apply(f64::from(q)) as f32;
                    sx *= s;
                    sy *= s;
                }
                MotionKind::Spin => {
                    rotate += 360.0 * r;
                    sx *= q;
                    sy *= q;
                }
                MotionKind::SlideLeft => cx -= r * (cx + iw * sx.abs() / 2.0),
                MotionKind::SlideRight => cx += r * (self.width as f32 - cx + iw * sx.abs() / 2.0),
                MotionKind::SlideUp => cy -= r * (cy + ih * sy.abs() / 2.0),
                MotionKind::SlideDown => cy += r * (self.height as f32 - cy + ih * sy.abs() / 2.0),
                MotionKind::Drop => cy -= r * 600.0 * k,
                MotionKind::WipeLeft => clip[2] = clip[2].min(q),
                MotionKind::WipeRight => clip[0] = clip[0].max(1.0 - q),
                MotionKind::WipeUp => clip[1] = clip[1].max(1.0 - q),
                MotionKind::WipeDown => clip[3] = clip[3].min(q),
                MotionKind::Shatter => {} // draw_element で破片として描く
            }
        }
        if p.flip {
            sx = -sx;
        }
        if p.flip_v {
            sy = -sy;
        }
        (opacity > 0.0).then_some(Placement {
            cx,
            cy,
            sx,
            sy,
            rotate,
            opacity,
            blend: p.blend,
            clip,
        })
    }
}

/// 登場（`entering`）・退場アニメーションの進み具合（1 で通常の姿）。範囲外は `None`。
fn progress(m: mmm_core::Motion, t: f64, duration: f64, entering: bool) -> Option<f32> {
    let len = m.duration.max(1e-6);
    let x = if entering {
        t / len
    } else {
        (duration - t) / len
    };
    (x < 1.0).then(|| m.ease.apply(x.max(0.0)) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmm_core::{TimelineInput, parse_script};

    fn setup(text: &str) -> (Script, Timeline, Renderer) {
        let parsed = parse_script(text);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let dur = |_: usize, _: usize| Some(2.0);
        let tl = Timeline::build(&TimelineInput {
            script: &parsed.script,
            line_duration: &dur,
            global_bgm: None,
            bgm_gain: 0.1,
        });
        let master: CharacterMaster =
            serde_json::from_str(r#"{"a": {"name": "A", "subtitle_style": {"fontsize": 40}}}"#)
                .unwrap();
        let r = Renderer::new(master, ".", None, 192, 108).unwrap();
        (parsed.script, tl, r)
    }

    #[test]
    fn draws_layers_animations_and_transitions() {
        let (script, tl, r) = setup(concat!(
            "# Scene\n- length: 2\n- shape: rect {w: 1920, h: 1080, color: #ff0000}\n",
            "# Scene\n- transition: fade 1 linear\n- length: 4\n",
            "- shape: rect {w: 1920, h: 1080, color: #0000ff}\n",
            "- shape: rect {w: 200, h: 200, color: #00ff00, x: -800->800, layer: 20}\n",
            "## Lines\na: こんにちは\n"
        ));
        let ls = LipSync::default();
        let red = r.frame(&script, &tl, &ls, 1.0);
        assert_eq!(red.get_pixel(96, 54).0, [255, 0, 0, 255]);
        // 切り替えの途中は赤と青が混ざる
        let mid = r.frame(&script, &tl, &ls, 2.5);
        let p = mid.get_pixel(96, 20).0;
        assert!(p[0] > 60 && p[2] > 60, "{p:?}");
        // 緑の四角は左から右へ動く
        let early = r.frame(&script, &tl, &ls, 3.2);
        let late = r.frame(&script, &tl, &ls, 5.8);
        let green_x = |img: &RgbaImage| {
            (0..192)
                .filter(|&x| img.get_pixel(x, 54).0[1] > 200)
                .sum::<u32>()
        };
        assert!(green_x(&early) < green_x(&late));
        // セリフの間は字幕（下の方に白い箱）が出る
        let line = r.frame(&script, &tl, &ls, 3.0);
        let whitish = |img: &RgbaImage| {
            (80..105)
                .flat_map(|y| (60..130).map(move |x| (x, y)))
                .filter(|&(x, y)| img.get_pixel(x, y).0[0] > 150 && img.get_pixel(x, y).0[1] > 150)
                .count()
        };
        assert!(whitish(&line) > 50, "{}", whitish(&line));
        assert_eq!(whitish(&late), 0);
    }

    #[test]
    fn text_item_with_enter_animation() {
        let (script, tl, r) = setup(
            "# Scene\n- length: 2\n- text: あ {size: 400, color: #ffffff, in: fade 1 linear}\n",
        );
        let ls = LipSync::default();
        let bright = |img: &RgbaImage| img.pixels().map(|p| u32::from(p.0[0])).max().unwrap();
        let a = r.frame(&script, &tl, &ls, 0.25);
        let b = r.frame(&script, &tl, &ls, 1.5);
        assert!(bright(&a) < bright(&b), "{} {}", bright(&a), bright(&b));
        assert_eq!(bright(&b), 255);
    }

    /// パーツ方式の立ち絵: 口の画像が母音で切り替わり、目は閉じる瞬間がある。
    #[test]
    fn parts_character_uses_vowel_mouths_and_blinks() {
        let dir = std::env::temp_dir().join(format!("mmm-parts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let solid = |name: &str, rgba: [u8; 4], rect: (u32, u32, u32, u32)| {
            let mut img = RgbaImage::new(100, 200);
            for y in rect.1..rect.1 + rect.3 {
                for x in rect.0..rect.0 + rect.2 {
                    img.put_pixel(x, y, image::Rgba(rgba));
                }
            }
            img.save(dir.join(name)).unwrap();
        };
        solid("base.png", [200, 200, 200, 255], (0, 0, 100, 200));
        solid("eye_open.png", [0, 0, 255, 255], (30, 40, 40, 10));
        solid("eye_closed.png", [0, 0, 0, 255], (30, 44, 40, 2));
        solid("m_a.png", [255, 0, 0, 255], (40, 80, 20, 20));
        solid("m_n.png", [0, 255, 0, 255], (40, 88, 20, 4));
        let master: CharacterMaster = serde_json::from_str(
            r#"{"c": {"blink_interval": 1.0, "variations": {"default": {
            "base": "base.png", "eyes": ["eye_open.png", "eye_closed.png"],
            "mouth": {"a": "m_a.png", "n": "m_n.png"}, "scale": 1.0, "pos": [0, 0]}}}}"#,
        )
        .unwrap();
        let r = Renderer::new(master.clone(), &dir, None, 1920, 1080).unwrap();
        let c = &master["c"];
        let v = c.variation("default").unwrap();
        let (open_mouth, ..) = r.character_image(c, v, Mouth::A, true, 0.0, 1).unwrap();
        let (closed, ..) = r
            .character_image(c, v, Mouth::Closed, false, 0.0, 1)
            .unwrap();
        // 高さ 1080 に拡大されている（倍率 5.4）
        let at =
            |img: &RgbaImage, x: f32, y: f32| img.get_pixel((x * 5.4) as u32, (y * 5.4) as u32).0;
        assert_eq!(at(&open_mouth, 50.0, 85.0), [255, 0, 0, 255]);
        assert_eq!(at(&closed, 50.0, 90.0), [0, 255, 0, 255]);
        // `i` の画像はないので近い口で代用される
        let (i_mouth, ..) = r.character_image(c, v, Mouth::I, true, 0.0, 1).unwrap();
        assert_ne!(at(&i_mouth, 50.0, 85.0), [200, 200, 200, 255]);
        let blinked = (0..300).any(|i| {
            let (img, ..) = r
                .character_image(c, v, Mouth::Closed, false, i as f64 * 0.01, 1)
                .unwrap();
            at(&img, 50.0, 41.0) != [0, 0, 255, 255]
        });
        assert!(blinked, "目を閉じる瞬間がある");
        std::fs::remove_dir_all(&dir).ok();
    }
}
