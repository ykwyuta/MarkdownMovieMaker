/// インターリーブされた f32 PCM。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioBuffer {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn new(rate: u32, channels: u16, samples: Vec<f32>) -> Self {
        Self {
            rate,
            channels,
            samples,
        }
    }

    pub fn silence(rate: u32, channels: u16, frames: usize) -> Self {
        Self::new(rate, channels, vec![0.0; frames * channels as usize])
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }

    /// モノラルへ平均化する。
    pub fn to_mono(&self) -> Self {
        let ch = self.channels.max(1) as usize;
        if ch == 1 {
            return self.clone();
        }
        let samples = self
            .samples
            .chunks_exact(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect();
        Self::new(self.rate, 1, samples)
    }

    /// ステレオへ変換する（モノラルは両チャンネルへ複製、3ch 以上は先頭 2ch）。
    pub fn to_stereo(&self) -> Self {
        match self.channels {
            2 => self.clone(),
            1 => Self::new(
                self.rate,
                2,
                self.samples.iter().flat_map(|&s| [s, s]).collect(),
            ),
            ch => Self::new(
                self.rate,
                2,
                self.samples
                    .chunks_exact(ch as usize)
                    .flat_map(|f| [f[0], f[1]])
                    .collect(),
            ),
        }
    }

    /// 区間ごとの RMS（口パク用）。`window` 秒ごとに 1 値。
    pub fn envelope(&self, window: f64) -> Vec<f32> {
        let mono = self.to_mono();
        let n = ((window * f64::from(self.rate)) as usize).max(1);
        mono.samples
            .chunks(n)
            .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
            .collect()
    }

    /// 16bit 符号付き PCM（リトルエンディアンのバイト列）へ変換する。
    pub fn to_i16_bytes(&self) -> Vec<u8> {
        self.samples
            .iter()
            .flat_map(|s| to_i16(*s).to_le_bytes())
            .collect()
    }
}

pub fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}
