//! symphonia による純 Rust デコード。
use crate::{AudioBuffer, Error, Result};
use std::{fs::File, path::Path};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymError, formats::FormatOptions,
    io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

/// ファイル全体をデコードする（BGM・効果音・音声は数分程度を想定）。
pub fn decode_file(path: &Path) -> Result<AudioBuffer> {
    let err = |message: String| Error::Decode {
        path: path.display().to_string(),
        message,
    };
    let file = File::open(path).map_err(|e| err(e.to_string()))?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    decode_stream(
        MediaSourceStream::new(Box::new(file), Default::default()),
        hint,
    )
    .map_err(err)
}

/// メモリ上の音声（WAV など）をデコードする。
pub fn decode_bytes(bytes: Vec<u8>, extension: &str) -> Result<AudioBuffer> {
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    decode_stream(source, hint).map_err(|message| Error::Decode {
        path: format!("<memory>.{extension}"),
        message,
    })
}

fn decode_stream(
    source: MediaSourceStream,
    hint: Hint,
) -> std::result::Result<AudioBuffer, String> {
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("形式を判別できません: {e}"))?;
    let mut format = probed.format;
    // 動画ファイルでは映像トラックが先頭のことがあるので、音声のトラックを選ぶ
    let track = format
        .tracks()
        .iter()
        .find(|t| {
            t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL
                && t.codec_params.sample_rate.is_some()
        })
        .or_else(|| format.default_track())
        .ok_or("音声トラックがありません")?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("未対応のコーデックです: {e}"))?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(0);
    let mut samples = Vec::new();
    let mut scratch: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // 壊れたフレームは飛ばす（MP3 の先頭タグなど）
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        channels = spec.channels.count() as u16;
        let buf = match &mut scratch {
            Some(b) if b.capacity() >= decoded.capacity() * channels as usize => b,
            _ => scratch.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buf.samples());
    }
    if rate == 0 || channels == 0 {
        return Err("音声データがありません".into());
    }
    Ok(AudioBuffer::new(rate, channels, samples))
}
