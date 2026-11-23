use crate::{AudioBuffer, Result, buffer::to_i16};
use std::{io::Cursor, path::Path};

fn spec(buf: &AudioBuffer) -> hound::WavSpec {
    hound::WavSpec {
        channels: buf.channels,
        sample_rate: buf.rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}

pub fn write_wav(path: &Path, buf: &AudioBuffer) -> Result<()> {
    let mut w = hound::WavWriter::create(path, spec(buf))?;
    for s in &buf.samples {
        w.write_sample(to_i16(*s))?;
    }
    w.finalize()?;
    Ok(())
}

pub fn wav_bytes(buf: &AudioBuffer) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec(buf))?;
        for s in &buf.samples {
            w.write_sample(to_i16(*s))?;
        }
        w.finalize()?;
    }
    Ok(cursor.into_inner())
}

pub fn read_wav(path: &Path) -> Result<AudioBuffer> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<std::result::Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<std::result::Result<_, _>>()?
        }
    };
    Ok(AudioBuffer::new(spec.sample_rate, spec.channels, samples))
}
