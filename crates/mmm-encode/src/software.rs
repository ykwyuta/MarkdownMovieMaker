//! 純 Rust の書き出し: rusty_h264（H.264）+ rusty-opus（Opus）+ muxide（MP4）。
//!
//! 固定 QP の H.264 では GOP（IDR から次の IDR の手前まで）が互いに独立しているので、
//! GOP ごとに新しいエンコーダを作って並列に符号化し、順番どおりに MP4 へ並べる。
//! 各 GOP の先頭には同じ SPS/PPS が付くため、連結した結果は 1 本のストリームとして正しい。
//! 描画も GOP 単位で同じスレッドが行うので、メモリには圧縮後のデータしか溜まらない。
use crate::{Error, Result, Settings, VideoWriter};
use image::RgbaImage;
use muxide::api::{AudioCodec, Muxer, MuxerBuilder, VideoCodec};
use rayon::prelude::*;
use rusty_h264::{Encoder, EncoderConfig, Preset, YuvFrame};
use rusty_opus::{Application, OpusEncoder};
use std::{
    fs::File,
    io::BufWriter,
    path::Path,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
};

const OPUS_FRAME: usize = 960; // 20ms @ 48kHz

fn err(e: impl std::fmt::Display) -> Error {
    Error::Encode(e.to_string())
}

/// RGBA → YUV 4:2:0（BT.709、リミテッドレンジ。HD の既定の色空間）。
///
/// 2 行ずつ処理し、輝度は 1 画素ずつ、色差は 2×2 の平均から整数演算で求める。
pub fn rgba_to_yuv(img: &RgbaImage) -> YuvFrame {
    let (w, h) = (img.width() as usize & !1, img.height() as usize & !1);
    let stride = img.width() as usize * 4;
    let p = img.as_raw();
    let (cw, ch) = (w / 2, h / 2);
    let mut y = vec![0u8; w * h];
    let mut u = vec![0u8; cw * ch];
    let mut v = vec![0u8; cw * ch];
    #[inline(always)]
    fn luma(r: i32, g: i32, b: i32) -> u8 {
        (((47 * r + 157 * g + 16 * b + 128) >> 8) + 16) as u8
    }
    let (y_rows, u_rows, v_rows) = (
        y.chunks_exact_mut(2 * w),
        u.chunks_exact_mut(cw),
        v.chunks_exact_mut(cw),
    );
    for (cy, ((yr, ur), vr)) in y_rows.zip(u_rows).zip(v_rows).enumerate() {
        let (top, bottom) = yr.split_at_mut(w);
        let r0 = &p[2 * cy * stride..2 * cy * stride + w * 4];
        let r1 = &p[(2 * cy + 1) * stride..(2 * cy + 1) * stride + w * 4];
        for cx in 0..cw {
            let a = &r0[cx * 8..cx * 8 + 8];
            let b = &r1[cx * 8..cx * 8 + 8];
            let px =
                |s: &[u8], o: usize| (i32::from(s[o]), i32::from(s[o + 1]), i32::from(s[o + 2]));
            let (p00, p01, p10, p11) = (px(a, 0), px(a, 4), px(b, 0), px(b, 4));
            top[cx * 2] = luma(p00.0, p00.1, p00.2);
            top[cx * 2 + 1] = luma(p01.0, p01.1, p01.2);
            bottom[cx * 2] = luma(p10.0, p10.1, p10.2);
            bottom[cx * 2 + 1] = luma(p11.0, p11.1, p11.2);
            let r = (p00.0 + p01.0 + p10.0 + p11.0) / 4;
            let g = (p00.1 + p01.1 + p10.1 + p11.1) / 4;
            let bl = (p00.2 + p01.2 + p10.2 + p11.2) / 4;
            ur[cx] = (((-26 * r - 86 * g + 112 * bl + 128) >> 8) + 128).clamp(0, 255) as u8;
            vr[cx] = (((112 * r - 102 * g - 10 * bl + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    YuvFrame {
        width: w,
        height: h,
        y,
        u,
        v,
    }
}

fn config(s: &Settings) -> EncoderConfig {
    let mut cfg = EncoderConfig::new(s.width as usize, s.height as usize);
    cfg.preset = if s.fast {
        Preset::Fast
    } else {
        Preset::Balanced
    };
    // mb-tree（時間方向の先読み AQ）は解説動画では効果が小さく、切ると 10〜30% 速い（計測: 画質差 ±0.5dB 以内）
    cfg.mbtree = false;
    cfg.gop_size = s.gop.max(1);
    cfg.bitrate = 0; // 固定 QP（GOP を独立に符号化できる条件）
    cfg.qp = s.qp.min(51);
    cfg.framerate = s.fps as f32;
    cfg
}

/// Annex-B の NAL ユニット（開始コードを含む）の位置。
fn nal_units(data: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            // 4 バイトの開始コード（00 00 00 01）は先頭の 0 も含める
            let s = if i > 0 && data[i - 1] == 0 { i - 1 } else { i };
            starts.push((s, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (k, &(s, h)) in starts.iter().enumerate() {
        let e = starts.get(k + 1).map_or(data.len(), |n| n.0);
        if h < e {
            out.push((s, e));
        }
    }
    out
}

/// エンコーダの出力（0 個以上のアクセスユニットの連結）をフレームごとに分ける。
/// スライスの `first_mb_in_slice == 0` で新しいフレームが始まり、直前の SPS/PPS/SEI はそのフレームに付く。
fn split_access_units(data: &[u8]) -> Vec<Vec<u8>> {
    let mut aus: Vec<Vec<u8>> = Vec::new();
    let mut prefix: Vec<u8> = Vec::new();
    for (s, e) in nal_units(data) {
        let nal = &data[s..e];
        let h = nal.iter().position(|&b| b == 1).map_or(0, |p| p + 1);
        let Some(&header) = nal.get(h) else { continue };
        match header & 0x1f {
            1 | 5 => {
                let first_slice = nal.get(h + 1).is_some_and(|b| b & 0x80 != 0);
                if first_slice || aus.is_empty() {
                    let mut au = std::mem::take(&mut prefix);
                    au.extend_from_slice(nal);
                    aus.push(au);
                } else if let Some(last) = aus.last_mut() {
                    last.extend_from_slice(nal);
                }
            }
            _ => prefix.extend_from_slice(nal),
        }
    }
    if !prefix.is_empty()
        && let Some(last) = aus.last_mut()
    {
        last.extend_from_slice(&prefix);
    }
    aus
}

/// 1 GOP を新しいエンコーダで符号化し、フレームごとのアクセスユニットを返す。
fn encode_gop(
    cfg: &EncoderConfig,
    frames: impl Iterator<Item = Result<Arc<YuvFrame>>>,
) -> Result<Vec<Vec<u8>>> {
    let mut enc = Encoder::new(cfg.clone()).map_err(|e| err(format!("{e:?}")))?;
    let mut bytes = Vec::new();
    let mut count = 0;
    let mut failed = None;
    for f in frames {
        match f {
            Ok(f) => {
                bytes.extend(enc.encode(&f));
                count += 1;
            }
            Err(e) => {
                failed = Some(e);
                break;
            }
        }
    }
    bytes.extend(enc.flush()); // 先読みに溜まったフレームを出し切る
    if let Some(e) = failed {
        return Err(e);
    }
    let aus = split_access_units(&bytes);
    if aus.len() != count {
        return Err(err(format!(
            "H.264 のフレーム数が合いません（{count} 枚に対して {} 個）",
            aus.len()
        )));
    }
    Ok(aus)
}

/// Annex-B のアクセスユニットに IDR スライスが含まれるか。
fn is_idr(au: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 < au.len() {
        if au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 1 {
            if au[i + 3] & 0x1f == 5 {
                return true;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

/// muxide + Opus。H.264 のアクセスユニットと PCM を受け取る。
struct Mp4 {
    mux: Muxer<BufWriter<File>>,
    fps: f64,
    frame: u64,
    opus: Option<OpusEncoder>,
    pending: Vec<f32>,
    audio_packets: u64,
    packet: Vec<u8>,
}

impl Mp4 {
    fn new(path: &Path, s: &Settings) -> Result<Self> {
        let opus = if s.audio {
            let mut enc = OpusEncoder::new(48_000, 2, Application::Audio)
                .map_err(|e| err(format!("Opus: {e:?}")))?;
            enc.bitrate_bps = s.audio_bitrate as i32;
            Some(enc)
        } else {
            None
        };
        let file = BufWriter::new(File::create(path)?);
        let mut builder =
            MuxerBuilder::new(file).video(VideoCodec::H264, s.width, s.height, f64::from(s.fps));
        if s.audio {
            builder = builder.audio(AudioCodec::Opus, 48_000, 2);
        }
        Ok(Self {
            mux: builder.with_fast_start(true).build().map_err(err)?,
            fps: f64::from(s.fps),
            frame: 0,
            opus,
            pending: Vec::new(),
            audio_packets: 0,
            packet: vec![0; 4000],
        })
    }

    /// 映像を 1 フレーム書き、その時刻までの音声を Opus にして書く（両トラックを交互に並べる）。
    fn video(&mut self, au: &[u8]) -> Result<()> {
        if !au.is_empty() {
            let pts = self.frame as f64 / self.fps;
            self.mux.write_video(pts, au, is_idr(au)).map_err(err)?;
        }
        self.frame += 1;
        self.encode_audio(Some(self.frame as f64 / self.fps))
    }

    /// 48kHz ステレオの PCM を積む（書き出しは `video` / `finish` のとき）。
    fn queue_audio(&mut self, samples: &[f32]) {
        if self.opus.is_some() {
            self.pending.extend_from_slice(samples);
        }
    }

    /// `until` 秒より前に始まるパケットを符号化する（`None` は残りすべて）。
    fn encode_audio(&mut self, until: Option<f64>) -> Result<()> {
        let Some(enc) = self.opus.as_mut() else {
            return Ok(());
        };
        if until.is_none() && !self.pending.len().is_multiple_of(OPUS_FRAME * 2) {
            let pad = OPUS_FRAME * 2 - self.pending.len() % (OPUS_FRAME * 2);
            self.pending.extend(std::iter::repeat_n(0.0, pad));
        }
        let mut used = 0;
        while self.pending.len() - used >= OPUS_FRAME * 2 {
            let pts = self.audio_packets as f64 * OPUS_FRAME as f64 / 48_000.0;
            if until.is_some_and(|u| pts >= u) {
                break;
            }
            let n = enc
                .encode(
                    &self.pending[used..used + OPUS_FRAME * 2],
                    OPUS_FRAME,
                    &mut self.packet,
                )
                .map_err(|e| err(format!("Opus: {e:?}")))?;
            self.mux.write_audio(pts, &self.packet[..n]).map_err(err)?;
            self.audio_packets += 1;
            used += OPUS_FRAME * 2;
        }
        self.pending.drain(..used);
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.encode_audio(None)?;
        self.mux.finish().map_err(err)
    }
}

/// 描画と符号化を GOP 単位で並列に行い、MP4 を書き出す。
///
/// - `render(i)`: i 番目のフレーム（`Settings` の大きさ）。複数スレッドから呼ばれる。
/// - `audio(start, count)`: 48kHz ステレオのインターリーブ PCM（48kHz のフレーム単位）。
/// - `progress(done)`: 書き終えたフレーム数。
pub fn encode_parallel(
    path: &Path,
    s: &Settings,
    total: u64,
    render: &(dyn Fn(u64) -> Arc<RgbaImage> + Sync),
    audio: &dyn Fn(usize, usize) -> Vec<f32>,
    progress: &dyn Fn(u64),
    cancel: &AtomicBool,
) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut mp4 = Mp4::new(path, s)?;
    let cfg = config(s);
    Encoder::new(cfg.clone()).map_err(|e| err(format!("{e:?}")))?; // 設定の検証
    let gop = u64::from(cfg.gop_size);
    let gops = total.div_ceil(gop);
    let window = rayon::current_num_threads().max(1) as u64;
    let per_frame = 48_000.0 / f64::from(s.fps);
    let mut written = 0u64;
    let mut g = 0;
    while g < gops {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Encode("中止しました".into()));
        }
        let end = (g + window).min(gops);
        let chunks: Vec<Result<Vec<Vec<u8>>>> = (g..end)
            .into_par_iter()
            .map(|gi| {
                // 直前と同じ絵（同じ Arc）なら YUV 変換もやり直さない
                let mut last: Option<(Arc<RgbaImage>, Arc<YuvFrame>)> = None;
                let frames = (gi * gop..((gi + 1) * gop).min(total)).map(move |f| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(Error::Encode("中止しました".into()));
                    }
                    let rgba = render(f);
                    if let Some((prev, yuv)) = &last
                        && Arc::ptr_eq(prev, &rgba)
                    {
                        return Ok(yuv.clone());
                    }
                    let yuv = Arc::new(rgba_to_yuv(&rgba));
                    last = Some((rgba, yuv.clone()));
                    Ok(yuv)
                });
                encode_gop(&cfg, frames)
            })
            .collect();
        for chunk in chunks {
            for au in chunk? {
                let a = (written as f64 * per_frame).round() as usize;
                let b = ((written + 1) as f64 * per_frame).round() as usize;
                mp4.queue_audio(&audio(a, b - a));
                mp4.video(&au)?;
                written += 1;
            }
        }
        progress(written);
        g = end;
    }
    mp4.finish()
}

/// 1 フレームずつ受け取る形の書き出し（GOP ぶん溜めてから符号化する）。
pub struct SoftwareWriter {
    mp4: Mp4,
    cfg: EncoderConfig,
    frames: Vec<YuvFrame>,
}

impl SoftwareWriter {
    pub fn new(path: &Path, s: &Settings) -> Result<Self> {
        let cfg = config(s);
        Encoder::new(cfg.clone()).map_err(|e| err(format!("{e:?}")))?;
        Ok(Self {
            mp4: Mp4::new(path, s)?,
            cfg,
            frames: Vec::new(),
        })
    }

    fn flush(&mut self) -> Result<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        let frames = std::mem::take(&mut self.frames);
        for au in encode_gop(&self.cfg, frames.into_iter().map(|f| Ok(Arc::new(f))))? {
            self.mp4.video(&au)?;
        }
        Ok(())
    }
}

impl VideoWriter for SoftwareWriter {
    fn push_frame(&mut self, frame: &RgbaImage) -> Result<()> {
        if frame.dimensions() != (self.cfg.width as u32, self.cfg.height as u32) {
            return Err(err(format!(
                "フレームの大きさが {}x{} ではありません",
                self.cfg.width, self.cfg.height
            )));
        }
        self.frames.push(rgba_to_yuv(frame));
        if self.frames.len() as u32 >= self.cfg.gop_size {
            self.flush()?;
        }
        Ok(())
    }

    fn push_audio(&mut self, samples: &[f32]) -> Result<()> {
        self.mp4.queue_audio(samples);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        self.flush()?;
        self.mp4.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(i: u64) -> RgbaImage {
        RgbaImage::from_fn(320, 180, |x, y| {
            image::Rgba([(x as u64 + i * 8) as u8, y as u8, 128, 255])
        })
    }

    fn tone(start: usize, count: usize) -> Vec<f32> {
        (start..start + count)
            .flat_map(|n| {
                let v = 0.3 * (n as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin();
                [v, v]
            })
            .collect()
    }

    fn atoms(bytes: &[u8]) -> bool {
        let has = |tag: &[u8]| bytes.windows(4).any(|w| w == tag);
        &bytes[4..8] == b"ftyp" && has(b"moov") && has(b"avc1") && has(b"Opus") && has(b"mdat")
    }

    #[test]
    fn parallel_and_sequential_writers_produce_identical_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::new(320, 180, 10);
        s.gop = 7; // 端数の GOP も含める
        let total = 25;
        let par = dir.path().join("par.mp4");
        let done = std::sync::atomic::AtomicU64::new(0);
        encode_parallel(
            &par,
            &s,
            total,
            &|i| Arc::new(frame(i)),
            &tone,
            &|d| done.store(d, Ordering::Relaxed),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(done.load(Ordering::Relaxed), total);
        let seq = dir.path().join("seq.mp4");
        let mut w = crate::create(&seq, &s, crate::Backend::Software).unwrap();
        for i in 0..total {
            w.push_frame(&frame(i)).unwrap();
            w.push_audio(&tone(i as usize * 4800, 4800)).unwrap();
        }
        w.finish().unwrap();
        let (a, b) = (std::fs::read(&par).unwrap(), std::fs::read(&seq).unwrap());
        assert!(atoms(&a) && atoms(&b));
        assert_eq!(a, b, "GOP 並列と逐次で同じバイト列になる");
    }

    #[test]
    fn cancel_stops_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::new(64, 64, 10);
        let r = encode_parallel(
            &dir.path().join("x.mp4"),
            &s,
            100,
            &|_| Arc::new(RgbaImage::new(64, 64)),
            &|_, n| vec![0.0; n * 2],
            &|_| {},
            &AtomicBool::new(true),
        );
        assert!(r.is_err());
    }

    /// 以前の 1 画素ずつの実装と同じ結果になる。
    #[test]
    fn yuv_conversion_matches_reference() {
        let img = RgbaImage::from_fn(36, 22, |x, y| {
            image::Rgba([
                (x * 7 + y * 3) as u8,
                (x * y) as u8,
                (255 - x * 5) as u8,
                255,
            ])
        });
        let fast = rgba_to_yuv(&img);
        let luma = |r: i32, g: i32, b: i32| ((47 * r + 157 * g + 16 * b + 128) >> 8) + 16;
        for py in 0..22u32 {
            for px in 0..36u32 {
                let q = img.get_pixel(px, py).0;
                let l = luma(i32::from(q[0]), i32::from(q[1]), i32::from(q[2])) as u8;
                assert_eq!(fast.y[(py * 36 + px) as usize], l);
            }
        }
        let q = |x, y| img.get_pixel(x, y).0.map(i32::from);
        let (a, b, c, d) = (q(2, 2), q(3, 2), q(2, 3), q(3, 3));
        let (r, g, bl) = (
            (a[0] + b[0] + c[0] + d[0]) / 4,
            (a[1] + b[1] + c[1] + d[1]) / 4,
            (a[2] + b[2] + c[2] + d[2]) / 4,
        );
        assert_eq!(
            fast.u[18 + 1],
            ((((-26 * r - 86 * g + 112 * bl + 128) >> 8) + 128).clamp(0, 255)) as u8
        );
        assert_eq!(
            fast.v[18 + 1],
            ((((112 * r - 102 * g - 10 * bl + 128) >> 8) + 128).clamp(0, 255)) as u8
        );
    }

    #[test]
    fn yuv_conversion_matches_bt709_limited_range() {
        let white = RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]));
        let y = rgba_to_yuv(&white);
        assert_eq!((y.y[0], y.u[0], y.v[0]), (235, 128, 128));
        let black = rgba_to_yuv(&RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255])));
        assert_eq!((black.y[0], black.u[0], black.v[0]), (16, 128, 128));
    }
}
