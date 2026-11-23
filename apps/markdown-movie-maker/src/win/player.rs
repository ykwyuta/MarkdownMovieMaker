//! 音声のプレビュー再生（cpal / WASAPI）。再生位置（48kHz のフレーム）を映像の時計として使う。
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mmm_pipeline::Source;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

const FRAC: f64 = 65536.0;

#[derive(Default)]
struct Shared {
    source: Mutex<Option<Arc<dyn Source>>>,
    /// 48kHz のフレーム位置（1/65536 単位）。
    pos: AtomicU64,
    playing: AtomicBool,
}

pub struct Player {
    shared: Arc<Shared>,
    stream: Option<cpal::Stream>,
    pub error: Option<String>,
}

impl Player {
    pub fn new() -> Self {
        let shared = Arc::new(Shared::default());
        match open(shared.clone()) {
            Ok(stream) => Self {
                shared,
                stream: Some(stream),
                error: None,
            },
            Err(e) => Self {
                shared,
                stream: None,
                error: Some(format!("音声出力を開けません（映像だけ再生します）: {e}")),
            },
        }
    }

    pub fn has_device(&self) -> bool {
        self.stream.is_some()
    }

    pub fn play(&self, source: Arc<dyn Source>, from: f64) {
        *self.shared.source.lock().unwrap() = Some(source);
        self.seek(from);
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
    }

    pub fn seek(&self, secs: f64) {
        self.shared
            .pos
            .store((secs.max(0.0) * 48_000.0 * FRAC) as u64, Ordering::Relaxed);
    }

    pub fn playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> f64 {
        self.shared.pos.load(Ordering::Relaxed) as f64 / FRAC / 48_000.0
    }
}

fn open(shared: Arc<Shared>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("出力デバイスがありません")?;
    let config = device
        .default_output_config()
        .map_err(|e| e.to_string())?
        .config();
    let channels = config.channels.max(1) as usize;
    let step = 48_000.0 / f64::from(config.sample_rate);
    let stream = device
        .build_output_stream::<f32, _, _>(
            config,
            move |out: &mut [f32], _| {
                out.fill(0.0);
                if !shared.playing.load(Ordering::Relaxed) {
                    return;
                }
                let Some(source) = shared.source.lock().unwrap().clone() else {
                    return;
                };
                let frames = out.len() / channels;
                let pos = shared.pos.load(Ordering::Relaxed) as f64 / FRAC;
                let start = pos.floor() as usize;
                let src = source.render(start, (frames as f64 * step).ceil() as usize + 2);
                for (i, frame) in out.chunks_exact_mut(channels).enumerate() {
                    let p = pos + i as f64 * step - start as f64;
                    let j = p.floor() as usize;
                    let f = (p - j as f64) as f32;
                    let at = |k: usize, c: usize| src.get(k * 2 + c).copied().unwrap_or(0.0);
                    let l = at(j, 0) * (1.0 - f) + at(j + 1, 0) * f;
                    let r = at(j, 1) * (1.0 - f) + at(j + 1, 1) * f;
                    if frame.len() == 1 {
                        frame[0] = (l + r) / 2.0;
                    } else {
                        frame[0] = l;
                        frame[1] = r;
                    }
                }
                let next = pos + frames as f64 * step;
                shared.pos.store((next * FRAC) as u64, Ordering::Relaxed);
                if next as usize >= source.frames() {
                    shared.playing.store(false, Ordering::Relaxed);
                }
            },
            |_| {},
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
