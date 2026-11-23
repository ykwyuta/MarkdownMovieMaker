//! 動画アイテム（MP4 / H.264）のデコード。純 Rust の mp4 デマルチプレクサと rusty_h264 を使う。
//!
//! 任意の時刻のフレームは、その時刻を含むフレームの直前のキーフレームから順にデコードして得る。
//! デコーダの状態を複数持ち、連続したフレームの要求（書き出しの各スレッド）では続きからデコードする。
use image::RgbaImage;
use mp4::{Mp4Reader, TrackType};
use rusty_h264::{Decoder, YuvFrame};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, Mutex},
};

struct Sample {
    /// 表示時刻（秒）。
    pts: f64,
    sync: bool,
}

struct State {
    decoder: Decoder,
    /// 次にデコードするサンプル（0 始まり、デコード順）。
    next: usize,
}

pub struct Video {
    reader: Mutex<Mp4Reader<BufReader<File>>>,
    track: u32,
    length_size: usize,
    params: Vec<u8>,
    samples: Vec<Sample>,
    /// 表示順に並べたサンプル番号。
    by_pts: Vec<usize>,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    states: Mutex<Vec<State>>,
    cache: Mutex<HashMap<usize, Arc<RgbaImage>>>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Video {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(err)?;
        let size = file.metadata().map_err(err)?.len();
        let mut reader = Mp4Reader::read_header(BufReader::new(file), size).map_err(err)?;
        let track = reader
            .tracks()
            .values()
            .find(|t| matches!(t.track_type(), Ok(TrackType::Video)))
            .ok_or("映像トラックがありません")?;
        let avc = track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .ok_or("H.264（avc1）以外の動画には対応していません")?;
        let mut params = Vec::new();
        for nal in avc
            .avcc
            .sequence_parameter_sets
            .iter()
            .chain(&avc.avcc.picture_parameter_sets)
        {
            params.extend_from_slice(&[0, 0, 0, 1]);
            params.extend_from_slice(&nal.bytes);
        }
        let length_size = usize::from(avc.avcc.length_size_minus_one & 3) + 1;
        let (id, timescale) = (track.track_id(), f64::from(track.timescale().max(1)));
        let (width, height) = (u32::from(track.width()), u32::from(track.height()));
        let count = reader.sample_count(id).map_err(err)?;
        let mut samples = Vec::with_capacity(count as usize);
        for sid in 1..=count {
            // メタデータだけでなく本体も読むが、ヘッダーの解析時に 1 回だけ
            if let Some(s) = reader.read_sample(id, sid).map_err(err)? {
                samples.push(Sample {
                    pts: (s.start_time as f64 + f64::from(s.rendering_offset)) / timescale,
                    sync: s.is_sync,
                });
            }
        }
        if samples.is_empty() {
            return Err("フレームがありません".into());
        }
        let mut by_pts: Vec<usize> = (0..samples.len()).collect();
        by_pts.sort_by(|a, b| samples[*a].pts.total_cmp(&samples[*b].pts));
        let first = samples[by_pts[0]].pts;
        for s in &mut samples {
            s.pts -= first;
        }
        let duration = reader.duration().as_secs_f64();
        Ok(Self {
            reader: Mutex::new(reader),
            track: id,
            length_size,
            params,
            samples,
            by_pts,
            width,
            height,
            duration,
            states: Mutex::default(),
            cache: Mutex::default(),
        })
    }

    /// 長さ付きの NAL を Annex-B へ。キーフレームには SPS/PPS を前に付ける。
    fn annex_b(&self, index: usize) -> Result<Vec<u8>, String> {
        let sample = self
            .reader
            .lock()
            .unwrap()
            .read_sample(self.track, index as u32 + 1)
            .map_err(err)?
            .ok_or("サンプルを読めません")?;
        let data = &sample.bytes[..];
        let mut out = Vec::with_capacity(data.len() + 64);
        if self.samples[index].sync {
            out.extend_from_slice(&self.params);
        }
        let mut i = 0;
        while i + self.length_size <= data.len() {
            let mut n = 0usize;
            for b in &data[i..i + self.length_size] {
                n = (n << 8) | usize::from(*b);
            }
            i += self.length_size;
            let end = (i + n).min(data.len());
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&data[i..end]);
            i = end;
        }
        Ok(out)
    }

    /// 時刻 `t`（動画内の秒）のフレーム。
    pub fn frame_at(&self, t: f64) -> Result<Arc<RgbaImage>, String> {
        let p = self
            .by_pts
            .partition_point(|&i| self.samples[i].pts <= t)
            .max(1)
            - 1;
        let target = self.by_pts[p];
        if let Some(img) = self.cache.lock().unwrap().get(&target) {
            return Ok(img.clone());
        }
        let key = (0..=target)
            .rev()
            .find(|&i| self.samples[i].sync)
            .unwrap_or(0);
        // 続きからデコードできる状態を探す（なければ新しく作る）
        let mut state = {
            let mut states = self.states.lock().unwrap();
            match states
                .iter()
                .position(|s| s.next > key && s.next <= target + 1)
            {
                Some(i) => states.swap_remove(i),
                None => State {
                    decoder: Decoder::new(),
                    next: key,
                },
            }
        };
        let mut picture: Option<YuvFrame> = None;
        while state.next <= target {
            let au = self.annex_b(state.next)?;
            let decoded = state
                .decoder
                .decode(&au)
                .map_err(|e| format!("H.264 をデコードできません: {e:?}"))?;
            if state.next == target {
                picture = decoded;
            } else if let Some(f) = decoded {
                state.decoder.recycle_frame(f);
            }
            state.next += 1;
        }
        let img = match picture {
            Some(f) => {
                let img = Arc::new(to_rgba(&f, self.width, self.height));
                state.decoder.recycle_frame(f);
                img
            }
            // 同じフレームをもう一度求められた（状態がすでに先へ進んでいた）
            None => self
                .cache
                .lock()
                .unwrap()
                .get(&target)
                .cloned()
                .ok_or("フレームをデコードできません")?,
        };
        {
            let mut states = self.states.lock().unwrap();
            states.push(state);
            if states.len() > 8 {
                states.remove(0);
            }
        }
        let mut cache = self.cache.lock().unwrap();
        if cache.len() > 32 {
            cache.clear();
        }
        cache.insert(target, img.clone());
        Ok(img)
    }
}

/// YUV 4:2:0 → RGBA（HD 以上は BT.709、それ未満は BT.601、リミテッドレンジ）。
pub fn to_rgba(f: &YuvFrame, width: u32, height: u32) -> RgbaImage {
    let (w, h) = (
        (width as usize).min(f.width),
        (height as usize).min(f.height),
    );
    let cw = f.width / 2;
    let hd = h >= 720;
    let (rv, gu, gv, bu) = if hd {
        (1.793, 0.213, 0.533, 2.112)
    } else {
        (1.596, 0.392, 0.813, 2.017)
    };
    let mut img = RgbaImage::new(w as u32, h as u32);
    for (i, p) in img.pixels_mut().enumerate() {
        let (x, y) = (i % w, i / w);
        let yv = (f32::from(f.y[y * f.width + x]) - 16.0) * 1.164;
        let u = f32::from(f.u[(y / 2) * cw + x / 2]) - 128.0;
        let v = f32::from(f.v[(y / 2) * cw + x / 2]) - 128.0;
        let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        p.0 = [c(yv + rv * v), c(yv - gu * u - gv * v), c(yv + bu * u), 255];
    }
    img
}
