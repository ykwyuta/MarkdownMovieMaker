//! 純 Rust の MP3 書き出し（`--audio-only`）。
//!
//! yyeditor の yytoolbox（`apps/yytoolbox/src/mp3.rs`）のストリーミングエンコーダを移植したもの。
//! libmp3lame・ffmpeg は使わない。入力は 48kHz ステレオ。
use crate::{AudioBuffer, Error, Result, SAMPLE_RATE};
use rusty_mp3::{Error as CodecError, Mp3Encoder, Mp3EncoderConfig};
use std::io::{Seek, SeekFrom, Write};

pub struct Encoder<W> {
    codec: Mp3Encoder,
    writer: W,
    start: u64,
    frames: u64,
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Mp3(e.to_string())
}

impl<W: Write + Seek> Encoder<W> {
    pub fn new(mut writer: W) -> Result<Self> {
        let start = writer.stream_position()?;
        Ok(Self {
            // VBR はパケットを逐次出力する（CBR のリザーバは finish まで溜め込む）
            codec: Mp3Encoder::new(Mp3EncoderConfig {
                bitrate_kbps: 192,
                vbr_quality: Some(rusty_mp3::vbr_quality_index(2.0)),
            }),
            writer,
            start,
            frames: 0,
        })
    }

    /// 16bit ステレオ PCM（リトルエンディアン）を追加する。
    pub fn push(&mut self, pcm: &[u8]) -> Result<()> {
        if !pcm.len().is_multiple_of(4) {
            return Err(err("16bit ステレオ PCM の完全なフレームが必要です"));
        }
        for block in pcm.chunks(1152 * 4) {
            self.codec
                .push_pcm_s16le(block, 2, SAMPLE_RATE)
                .map_err(err)?;
            self.frames += block.len() as u64 / 4;
            self.drain(false)?;
        }
        Ok(())
    }

    fn drain(&mut self, finished: bool) -> Result<()> {
        loop {
            match self.codec.next_packet() {
                Ok(packet) => self.writer.write_all(&packet)?,
                Err(CodecError::Again) if !finished => return Ok(()),
                Err(CodecError::Eof) if finished => return Ok(()),
                Err(e) => return Err(err(e)),
            }
        }
    }

    pub fn finish(mut self) -> Result<W> {
        if self.frames == 0 {
            return Err(err("保存する音声がありません"));
        }
        self.codec.finish();
        self.drain(true)?;
        let tag = self
            .codec
            .lame_tag_frame()
            .ok_or_else(|| err("再生時間情報を作成できません"))?;
        self.writer.seek(SeekFrom::Start(self.start))?;
        self.writer.write_all(tag)?;
        self.writer.seek(SeekFrom::End(0))?;
        self.writer.flush()?;
        Ok(self.writer)
    }
}

/// 48kHz ステレオのバッファを MP3 ファイルへ書き出す。
pub fn write_mp3(path: &std::path::Path, buf: &AudioBuffer) -> Result<()> {
    assert_eq!((buf.rate, buf.channels), (SAMPLE_RATE, 2));
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = Encoder::new(file)?;
    for chunk in buf.samples.chunks(48_000 * 2) {
        enc.push(&AudioBuffer::new(SAMPLE_RATE, 2, chunk.to_vec()).to_i16_bytes())?;
    }
    enc.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_roundtrip_through_symphonia() {
        let n = 48_000;
        let samples: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = (0.4 * (std::f64::consts::TAU * 440.0 * i as f64 / 48_000.0).sin()) as f32;
                [v, v]
            })
            .collect();
        let dir = tempdir();
        let path = dir.join("t.mp3");
        write_mp3(&path, &AudioBuffer::new(SAMPLE_RATE, 2, samples)).unwrap();
        let decoded = crate::decode::decode_file(&path).unwrap();
        assert_eq!(decoded.rate, 48_000);
        assert_eq!(decoded.channels, 2);
        // デコーダ遅延・パディングを許容して長さを確認
        assert!(
            (decoded.duration() - 1.0).abs() < 0.1,
            "{}",
            decoded.duration()
        );
        let rms = (decoded.samples.iter().map(|s| s * s).sum::<f32>()
            / decoded.samples.len() as f32)
            .sqrt();
        assert!(rms > 0.2, "rms {rms}");
        std::fs::remove_dir_all(dir).ok();
    }

    fn tempdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mmm-mp3-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
