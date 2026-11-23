//! 音声合成。
//!
//! 既定のエンジンは **VOICEVOX**（起動済みの VOICEVOX エンジンへ HTTP で問い合わせる）。
//! そのほかにオフラインで動く軽量な **Kokoro-82M**（Apache-2.0、any-tts / Candle による純 Rust 推論）を選べる。
//! 合成結果は (エンジン, 話者, 文, 速度…) のハッシュでディスクにキャッシュし、
//! プレビューと書き出しで再合成しない。
mod dictionary;
mod engines;
mod request;

pub use dictionary::Dictionary;
pub use engines::{EngineSettings, available_engines};
pub use request::{EngineKind, VoiceRequest, detect_language, request_for_line};

use mmm_audio::{
    AudioBuffer, decode,
    stretch::{pitch_shift, time_stretch},
    wav,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{engine}: {message}")]
    Engine {
        engine: &'static str,
        message: String,
    },
    #[error(
        "音声合成エンジン `{0}` はこのビルドに含まれていません（Cargo の feature を確認してください）"
    )]
    Unavailable(&'static str),
    #[error(transparent)]
    Audio(#[from] mmm_audio::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// エンジン 1 種類。
pub trait SpeechEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    /// モデル・話者を含むキャッシュ用の識別子。
    fn cache_id(&self) -> String;
    /// エンジン自身が速度指定に対応しているか（非対応なら WSOLA で伸縮する）。
    fn native_speed(&self) -> bool;
    fn synthesize(&self, req: &VoiceRequest, text: &str) -> Result<AudioBuffer>;
    /// 選べる話者名。
    fn voices(&self) -> Vec<String>;
}

/// エンジンの遅延読み込み・辞書・キャッシュをまとめた合成器。スレッド間で共有できる。
pub struct Synthesizer {
    settings: EngineSettings,
    dictionary: Dictionary,
    cache_dir: Option<PathBuf>,
    engines: Mutex<HashMap<EngineKind, Arc<dyn SpeechEngine>>>,
}

impl Synthesizer {
    pub fn new(
        settings: EngineSettings,
        dictionary: Dictionary,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            settings,
            dictionary,
            cache_dir,
            engines: Mutex::default(),
        }
    }

    pub fn settings(&self) -> &EngineSettings {
        &self.settings
    }

    /// エンジンを取得する（初回はモデルの読み込み・ダウンロードを行うので時間がかかる）。
    pub fn engine(&self, kind: EngineKind) -> Result<Arc<dyn SpeechEngine>> {
        if let Some(e) = self.engines.lock().unwrap().get(&kind) {
            return Ok(e.clone());
        }
        let engine = engines::load(kind, &self.settings, &self.dictionary)?;
        self.engines.lock().unwrap().insert(kind, engine.clone());
        Ok(engine)
    }

    fn cache_path(
        &self,
        engine: &dyn SpeechEngine,
        req: &VoiceRequest,
        text: &str,
    ) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let mut h = blake3::Hasher::new();
        for part in [
            engine.cache_id().as_str(),
            req.speaker.as_deref().unwrap_or(""),
            req.language.as_deref().unwrap_or(""),
            req.instruct.as_deref().unwrap_or(""),
            text,
            &format!("{:.3}/{:.3}/{:.3}", req.speed, req.volume, req.pitch),
        ] {
            h.update(part.as_bytes());
            h.update(&[0]);
        }
        Some(dir.join(format!("{}.wav", &h.finalize().to_hex()[..32])))
    }

    /// 1 セリフを合成する。キャッシュがあればそれを返す。
    pub fn synthesize(&self, req: &VoiceRequest) -> Result<Arc<AudioBuffer>> {
        let engine = self.engine(req.engine)?;
        let text = if req.engine == EngineKind::Voicevox {
            req.text.clone() // VOICEVOX はユーザー辞書をエンジン側に登録する
        } else {
            self.dictionary.apply(&req.text)
        };
        let cache = self.cache_path(engine.as_ref(), req, &text);
        if let Some(path) = cache.as_ref().filter(|p| p.exists())
            && let Ok(buf) = wav::read_wav(path)
        {
            return Ok(Arc::new(buf));
        }
        let mut buf = engine.synthesize(req, &text)?;
        if !engine.native_speed() && (req.speed - 1.0).abs() > 1e-3 {
            buf = time_stretch(&buf, req.speed);
        }
        if (req.pitch - 1.0).abs() > 1e-3 {
            buf = pitch_shift(&buf.to_mono(), req.pitch);
        }
        if (req.volume - 1.0).abs() > 1e-3 {
            for s in &mut buf.samples {
                *s *= req.volume as f32;
            }
        }
        let buf = trim_silence(buf);
        if let Some(path) = cache {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            wav::write_wav(&path, &buf)?;
        }
        Ok(Arc::new(buf))
    }

    /// エンジン外部で用意した WAV/MP3 を音声として読み込む（テスト・差し替え用）。
    pub fn load_file(path: &std::path::Path) -> Result<AudioBuffer> {
        Ok(decode::decode_file(path)?)
    }
}

/// 前後の長い無音を詰める（前後 80ms は残す）。生成モデルは前後に無音を付けることがある。
pub fn trim_silence(mut buf: AudioBuffer) -> AudioBuffer {
    let ch = buf.channels.max(1) as usize;
    let threshold = 0.01;
    let keep = (0.08 * f64::from(buf.rate)) as usize;
    let frames = buf.frames();
    let loud = |i: usize| {
        buf.samples[i * ch..i * ch + ch]
            .iter()
            .any(|s| s.abs() > threshold)
    };
    let Some(first) = (0..frames).find(|&i| loud(i)) else {
        return buf;
    };
    let last = (0..frames).rev().find(|&i| loud(i)).unwrap_or(first);
    let a = first.saturating_sub(keep);
    let b = (last + 1 + keep).min(frames);
    buf.samples = buf.samples[a * ch..b * ch].to_vec();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_engine_caches_and_applies_volume() {
        let dir = tempfile::tempdir().unwrap();
        let s = Synthesizer::new(
            EngineSettings::default(),
            Dictionary::default(),
            Some(dir.path().into()),
        );
        let req = VoiceRequest {
            engine: EngineKind::Silent,
            text: "こんにちは".into(),
            speaker: None,
            language: Some("ja".into()),
            instruct: None,
            speed: 1.0,
            volume: 1.0,
            pitch: 1.0,
        };
        let a = s.synthesize(&req).unwrap();
        assert!(a.duration() > 0.5);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        let b = s.synthesize(&req).unwrap();
        assert_eq!(a.frames(), b.frames());
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut samples = vec![0.0; 24_000];
        samples[12_000] = 0.5;
        let out = trim_silence(AudioBuffer::new(24_000, 1, samples));
        assert_eq!(out.frames(), 1920 * 2 + 1);
    }
}
