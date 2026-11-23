//! Windows Media Foundation の SinkWriter による H.264/AAC の MP4 書き出し。
//!
//! yyeditor の yytoolbox（`apps/yytoolbox/src/recording.rs` の `Encoder`）から移植し、
//! ハードウェアエンコーダ（NVIDIA は NVENC、Intel は Quick Sync）を有効にした。
#![allow(unsafe_code)]
use crate::{Error, Result, Settings, VideoWriter};
use image::RgbaImage;
use std::path::Path;
use windows::{
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
    },
    core::HSTRING,
};

/// 100ns 単位。
const SECOND: i64 = 10_000_000;

fn err(e: windows::core::Error) -> Error {
    Error::Encode(format!("Media Foundation: {e}"))
}

struct Runtime;

impl Runtime {
    fn start() -> Result<Self> {
        unsafe {
            // 既に別のモードで初期化済みのスレッドでも続行できるよう結果は確認しない
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| {
                Error::Encode(format!(
                    "Media Foundation を開始できません。Windows のメディア機能（N エディションでは Media Feature Pack）を確認してください: {e}"
                ))
            })?;
        }
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

pub struct MfWriter {
    writer: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
    settings: Settings,
    frame: i64,
    audio_frames: i64,
    bgra: Vec<u8>,
    _runtime: Runtime,
}

// IMFSinkWriter はフリースレッドの COM オブジェクトで、MTA で作成している
unsafe impl Send for MfWriter {}

impl MfWriter {
    pub fn new(path: &Path, s: &Settings) -> Result<Self> {
        let runtime = Runtime::start()?;
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 2).map_err(err)?;
            let attributes =
                attributes.ok_or_else(|| Error::Encode("属性を作成できません".into()))?;
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(err)?;
            attributes
                .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
                .map_err(err)?;
            let writer =
                MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attributes)
                    .map_err(err)?;

            let output = MFCreateMediaType().map_err(err)?;
            output
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(err)?;
            output
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
                .map_err(err)?;
            output
                .SetUINT32(&MF_MT_AVG_BITRATE, s.video_bitrate)
                .map_err(err)?;
            output
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(err)?;
            output
                .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)
                .map_err(err)?;
            output
                .SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    (u64::from(s.width) << 32) | u64::from(s.height),
                )
                .map_err(err)?;
            output
                .SetUINT64(&MF_MT_FRAME_RATE, (u64::from(s.fps) << 32) | 1)
                .map_err(err)?;
            output
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
                .map_err(err)?;
            let video = writer.AddStream(&output).map_err(err)?;
            let input = MFCreateMediaType().map_err(err)?;
            output.CopyAllItems(&input).map_err(err)?;
            input
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(err)?;
            input
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, s.width * 4)
                .map_err(err)?;
            writer
                .SetInputMediaType(video, &input, None)
                .map_err(|e| Error::Encode(format!("H.264 エンコーダを初期化できません: {e}")))?;

            let audio = if s.audio {
                let output = MFCreateMediaType().map_err(err)?;
                output
                    .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                    .map_err(err)?;
                output
                    .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)
                    .map_err(err)?;
                output
                    .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)
                    .map_err(err)?;
                output
                    .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)
                    .map_err(err)?;
                output
                    .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                    .map_err(err)?;
                // AAC エンコーダが受け付けるのは 12000/16000/20000/24000 バイト毎秒
                output
                    .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)
                    .map_err(err)?;
                let stream = writer.AddStream(&output).map_err(err)?;
                let input = MFCreateMediaType().map_err(err)?;
                input
                    .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                    .map_err(err)?;
                input
                    .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
                    .map_err(err)?;
                input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(err)?;
                input
                    .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)
                    .map_err(err)?;
                input
                    .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                    .map_err(err)?;
                input
                    .SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)
                    .map_err(err)?;
                input
                    .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)
                    .map_err(err)?;
                writer
                    .SetInputMediaType(stream, &input, None)
                    .map_err(|e| Error::Encode(format!("AAC エンコーダを初期化できません: {e}")))?;
                Some(stream)
            } else {
                None
            };
            writer.BeginWriting().map_err(err)?;
            Ok(Self {
                writer,
                video,
                audio,
                settings: s.clone(),
                frame: 0,
                audio_frames: 0,
                bgra: Vec::new(),
                _runtime: runtime,
            })
        }
    }

    fn sample(&self, stream: u32, data: &[u8], time: i64, duration: i64) -> Result<()> {
        unsafe {
            let buffer = MFCreateMemoryBuffer(data.len() as u32).map_err(err)?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).map_err(err)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            buffer.Unlock().map_err(err)?;
            buffer.SetCurrentLength(data.len() as u32).map_err(err)?;
            let sample = MFCreateSample().map_err(err)?;
            sample.AddBuffer(&buffer).map_err(err)?;
            sample.SetSampleTime(time).map_err(err)?;
            sample.SetSampleDuration(duration).map_err(err)?;
            self.writer.WriteSample(stream, &sample).map_err(err)
        }
    }
}

impl VideoWriter for MfWriter {
    fn push_frame(&mut self, frame: &RgbaImage) -> Result<()> {
        let s = &self.settings;
        if frame.dimensions() != (s.width, s.height) {
            return Err(Error::Encode(format!(
                "フレームの大きさが {}x{} ではありません",
                s.width, s.height
            )));
        }
        // RGB32 は BGRX の上から下の並び（正のストライド）
        self.bgra.clear();
        self.bgra.extend(
            frame
                .as_raw()
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], 255]),
        );
        let fps = i64::from(s.fps);
        let time = self.frame * SECOND / fps;
        let next = (self.frame + 1) * SECOND / fps;
        let data = std::mem::take(&mut self.bgra);
        let result = self.sample(self.video, &data, time, next - time);
        self.bgra = data;
        self.frame += 1;
        result
    }

    fn push_audio(&mut self, samples: &[f32]) -> Result<()> {
        let Some(stream) = self.audio else {
            return Ok(());
        };
        for chunk in samples.chunks(4800 * 2) {
            let frames = (chunk.len() / 2) as i64;
            let pcm: Vec<u8> = chunk
                .iter()
                .flat_map(|v| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes())
                .collect();
            let time = self.audio_frames * SECOND / 48_000;
            let duration = frames * SECOND / 48_000;
            self.sample(stream, &pcm, time, duration)?;
            self.audio_frames += frames;
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<()> {
        unsafe { self.writer.Finalize() }
            .map_err(|e| Error::Encode(format!("MP4 を確定できません: {e}")))
    }
}
