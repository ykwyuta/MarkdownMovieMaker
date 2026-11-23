//! プレビュー画像を別スレッドで描く。要求は最新のものだけ処理し、できたら画面へ通知する。
use mmm_core::Timeline;
use mmm_pipeline::{Options, Project};
use mmm_render::{LipSync, Renderer};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

pub struct Request {
    pub project: Arc<Project>,
    pub timeline: Arc<Timeline>,
    pub lipsync: Arc<LipSync>,
    pub time: f64,
    pub width: u32,
    pub height: u32,
}

/// 描き終えた画像（GDI にそのまま渡せる BGRA、上から下）。
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub missing: Vec<String>,
}

pub struct Worker {
    tx: mpsc::Sender<Request>,
    pub latest: Arc<Mutex<Option<Frame>>>,
}

impl Worker {
    /// `notify` は描き終えるたびに（描画スレッドから）呼ばれる。
    pub fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let latest = Arc::new(Mutex::new(None));
        let out = latest.clone();
        thread::Builder::new()
            .name("preview".into())
            .spawn(move || {
                let mut renderer: Option<Renderer> = None;
                let mut last: Option<Arc<image::RgbaImage>> = None;
                while let Ok(mut req) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        req = newer;
                    }
                    let reuse = renderer.as_ref().is_some_and(|r| {
                        r.master == req.project.master
                            && r.width == req.width
                            && r.assets.base_dir() == req.project.root
                    });
                    if !reuse {
                        let opts = Options {
                            width: req.width,
                            height: req.height,
                            ..Options::default()
                        };
                        renderer = req.project.renderer(&opts).ok();
                        // 再生中は直前と同じ絵のフレームを描き直さない
                        if let Some(r) = &renderer {
                            r.set_reuse_frames(true);
                        }
                    }
                    let Some(r) = renderer.as_ref() else { continue };
                    let script = &req.project.parsed.script;
                    let img = if req.timeline.segments.is_empty() {
                        Arc::new(image::RgbaImage::from_pixel(
                            req.width,
                            req.height,
                            image::Rgba([24, 24, 24, 255]),
                        ))
                    } else {
                        r.frame_shared(script, &req.timeline, &req.lipsync, req.time)
                    };
                    // 直前と同じ絵（使い回したフレーム）なら変換も再描画もいらない
                    if last.as_ref().is_some_and(|l| Arc::ptr_eq(l, &img)) {
                        continue;
                    }
                    last = Some(img.clone());
                    let (width, height) = img.dimensions();
                    // RGBA → BGRA はその場で R と B を入れ替えるだけ
                    let mut bgra = Arc::unwrap_or_clone(img).into_raw();
                    for p in bgra.chunks_exact_mut(4) {
                        p.swap(0, 2);
                    }
                    *out.lock().unwrap() = Some(Frame {
                        width,
                        height,
                        bgra,
                        missing: r.assets.missing(),
                    });
                    notify();
                }
            })
            .expect("preview thread");
        Self { tx, latest }
    }

    pub fn request(&self, req: Request) {
        let _ = self.tx.send(req);
    }
}

/// スライド一覧のサムネイル（シーンごとの静止画）を別スレッドで描く。
pub struct ThumbRequest {
    pub project: Arc<Project>,
    pub timeline: Arc<Timeline>,
    pub lipsync: Arc<LipSync>,
    pub generation: usize,
    /// シーンごとの内容の鍵。前回と同じシーンは描き直さない。
    pub keys: Vec<u64>,
    pub width: u32,
    pub height: u32,
}

pub struct Thumbs {
    tx: mpsc::Sender<ThumbRequest>,
    /// シーン番号 → (内容の鍵, 画像)。
    pub images: Arc<Mutex<std::collections::HashMap<usize, (u64, Frame)>>>,
    latest: Arc<std::sync::atomic::AtomicUsize>,
}

impl Thumbs {
    pub fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        use std::sync::atomic::Ordering;
        let (tx, rx) = mpsc::channel::<ThumbRequest>();
        let images: Arc<Mutex<std::collections::HashMap<usize, (u64, Frame)>>> = Arc::default();
        let latest: Arc<std::sync::atomic::AtomicUsize> = Arc::default();
        let (out, newest) = (images.clone(), latest.clone());
        thread::Builder::new()
            .name("thumbnails".into())
            .spawn(move || {
                let mut renderer: Option<Renderer> = None;
                while let Ok(mut req) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        req = newer;
                    }
                    let reuse = renderer.as_ref().is_some_and(|r| {
                        r.master == req.project.master
                            && r.width == req.width
                            && r.assets.base_dir() == req.project.root
                    });
                    if !reuse {
                        let opts = Options {
                            width: req.width,
                            height: req.height,
                            ..Options::default()
                        };
                        renderer = req.project.renderer(&opts).ok();
                    }
                    let Some(r) = renderer.as_ref() else { continue };
                    let script = &req.project.parsed.script;
                    for (si, span) in req.timeline.scenes.iter().enumerate() {
                        // 新しい要求が来たら古い世代は途中でやめる
                        if newest.load(Ordering::Relaxed) != req.generation {
                            break;
                        }
                        if span.end <= span.start {
                            continue;
                        }
                        let key = req.keys.get(si).copied().unwrap_or(0) ^ u64::from(req.width);
                        if out.lock().unwrap().get(&si).is_some_and(|(k, _)| *k == key) {
                            continue; // 内容が変わっていないシーン
                        }
                        // 最初のセリフの途中（セリフがなければシーンの中ほど）
                        let line = span.lines.iter().position(Option::is_some);
                        let img = r.still(script, &req.timeline, &req.lipsync, si, line);
                        let bgra = img
                            .as_raw()
                            .chunks_exact(4)
                            .flat_map(|p| [p[2], p[1], p[0], 255])
                            .collect();
                        let frame = Frame {
                            width: img.width(),
                            height: img.height(),
                            bgra,
                            missing: Vec::new(),
                        };
                        out.lock().unwrap().insert(si, (key, frame));
                        notify();
                    }
                }
            })
            .expect("thumbnail thread");
        Self { tx, images, latest }
    }

    pub fn request(&self, req: ThumbRequest) {
        self.latest
            .store(req.generation, std::sync::atomic::Ordering::Relaxed);
        let _ = self.tx.send(req);
    }
}
