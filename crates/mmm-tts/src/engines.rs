use crate::{Dictionary, EngineKind, Error, Result, SpeechEngine, VoiceRequest};
use mmm_audio::AudioBuffer;
use std::sync::Arc;

/// エンジンの設定（モデルの場所・デバイス・VOICEVOX の URL）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineSettings {
    /// Kokoro のモデル（Hugging Face のモデル ID かローカルのディレクトリ）。
    pub kokoro_model: String,
    /// `auto`（CUDA があれば GPU）/ `cpu` / `cuda:0`。
    pub device: String,
    pub voicevox_url: String,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            kokoro_model: "hexgrad/Kokoro-82M".into(),
            device: "auto".into(),
            voicevox_url: "http://127.0.0.1:50021".into(),
        }
    }
}

/// このビルドで使えるエンジン。
pub fn available_engines() -> Vec<EngineKind> {
    EngineKind::ALL
        .into_iter()
        .filter(|k| match k {
            EngineKind::Voicevox => cfg!(feature = "voicevox"),
            EngineKind::Kokoro => cfg!(feature = "kokoro"),
            EngineKind::Silent => true,
        })
        .collect()
}

pub(crate) fn load(
    kind: EngineKind,
    settings: &EngineSettings,
    dictionary: &Dictionary,
) -> Result<Arc<dyn SpeechEngine>> {
    let _ = dictionary;
    match kind {
        EngineKind::Silent => Ok(Arc::new(Silent)),
        #[cfg(feature = "kokoro")]
        EngineKind::Kokoro => Ok(Arc::new(neural::Kokoro::load(settings)?)),
        #[cfg(feature = "voicevox")]
        EngineKind::Voicevox => Ok(Arc::new(voicevox::Voicevox::connect(settings, dictionary)?)),
        #[allow(unreachable_patterns)]
        other => {
            let _ = settings;
            Err(Error::Unavailable(other.id()))
        }
    }
}

/// 推定の長さの無音。
struct Silent;

impl SpeechEngine for Silent {
    fn kind(&self) -> EngineKind {
        EngineKind::Silent
    }
    fn cache_id(&self) -> String {
        "silent/1".into()
    }
    fn native_speed(&self) -> bool {
        true
    }
    fn synthesize(&self, req: &VoiceRequest, text: &str) -> Result<AudioBuffer> {
        let secs = mmm_core::estimate_duration(text, req.speed);
        let rate = 24_000;
        // 無音のままだと前後の無音除去で消えるため、聞こえない程度のごく小さな信号を入れる
        let n = (secs * f64::from(rate)) as usize;
        let samples = (0..n)
            .map(|i| if i % 2 == 0 { 0.011 } else { -0.011 })
            .collect();
        Ok(AudioBuffer::new(rate, 1, samples))
    }
    fn voices(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(feature = "kokoro")]
mod neural {
    use super::*;
    use any_tts::{DeviceSelection, ModelType, SynthesisRequest, TtsConfig, TtsModel};

    /// any-tts（Candle）上の Kokoro-82M。
    pub struct Kokoro {
        model_id: String,
        model: Box<dyn TtsModel>,
    }

    fn err(e: impl std::fmt::Display) -> Error {
        Error::Engine {
            engine: EngineKind::Kokoro.id(),
            message: e.to_string(),
        }
    }

    fn device(s: &str) -> DeviceSelection {
        match s.trim() {
            "cpu" => DeviceSelection::Cpu,
            d if d.starts_with("cuda") => DeviceSelection::Cuda(
                d.trim_start_matches("cuda")
                    .trim_start_matches(':')
                    .parse()
                    .unwrap_or(0),
            ),
            _ => DeviceSelection::Auto,
        }
    }

    impl Kokoro {
        pub fn load(settings: &EngineSettings) -> Result<Self> {
            let model_id = settings.kokoro_model.clone();
            let mut config = TtsConfig::new(ModelType::Kokoro).with_preferred_runtime();
            if settings.device.trim() != "auto" {
                config = config.with_device(device(&settings.device));
            }
            config = if std::path::Path::new(&model_id).is_dir() {
                config.with_model_path(model_id.clone())
            } else {
                config.with_hf_model_id(model_id.clone())
            };
            let model = any_tts::load_model(config).map_err(err)?;
            Ok(Self { model_id, model })
        }
    }

    impl SpeechEngine for Kokoro {
        fn kind(&self) -> EngineKind {
            EngineKind::Kokoro
        }
        fn cache_id(&self) -> String {
            format!("kokoro/{}", self.model_id)
        }
        fn native_speed(&self) -> bool {
            true
        }
        fn synthesize(&self, req: &VoiceRequest, text: &str) -> Result<AudioBuffer> {
            let mut r = SynthesisRequest::new(text).with_speed(req.speed);
            if let Some(lang) = &req.language {
                r = r.with_language(lang.clone());
            }
            if let Some(speaker) = &req.speaker {
                r = r.with_voice(speaker.clone());
            }
            let audio = self.model.synthesize(&r).map_err(err)?;
            Ok(AudioBuffer::new(
                audio.sample_rate,
                audio.channels.max(1),
                audio.samples,
            ))
        }
        fn voices(&self) -> Vec<String> {
            self.model.supported_voices()
        }
    }
}

#[cfg(feature = "voicevox")]
mod voicevox {
    use super::*;
    use crate::dictionary::hira_to_kata;
    use serde_json::Value;

    /// 起動済みの VOICEVOX エンジンへ HTTP で問い合わせる（Python 版と同じ API 呼び出し）。
    pub struct Voicevox {
        url: String,
        agent: ureq::Agent,
    }

    fn err(e: impl std::fmt::Display) -> Error {
        Error::Engine {
            engine: "voicevox",
            message: e.to_string(),
        }
    }

    impl Voicevox {
        pub fn connect(settings: &EngineSettings, dictionary: &Dictionary) -> Result<Self> {
            let v = Self {
                url: settings.voicevox_url.trim_end_matches('/').to_owned(),
                agent: ureq::AgentBuilder::new()
                    .timeout(std::time::Duration::from_secs(120))
                    .build(),
            };
            v.agent
                .get(&format!("{}/version", v.url))
                .call()
                .map_err(|e| {
                    err(format!(
                        "{} に接続できません（VOICEVOX を起動してください）: {e}",
                        v.url
                    ))
                })?;
            v.register(dictionary)?;
            Ok(v)
        }

        fn query(&self, text: &str, speaker: &str) -> Result<Value> {
            self.agent
                .post(&format!("{}/audio_query", self.url))
                .query("text", text)
                .query("speaker", speaker)
                .call()
                .map_err(err)?
                .into_json()
                .map_err(err)
        }

        /// 読み辞書をユーザー辞書へ登録する（登録済みの表記は飛ばす）。
        fn register(&self, dictionary: &Dictionary) -> Result<()> {
            if dictionary.entries.is_empty() {
                return Ok(());
            }
            let current: Value = self
                .agent
                .get(&format!("{}/user_dict", self.url))
                .call()
                .map_err(err)?
                .into_json()
                .map_err(err)?;
            let known: Vec<&str> = current
                .as_object()
                .map(|o| o.values().filter_map(|w| w["surface"].as_str()).collect())
                .unwrap_or_default();
            for (surface, reading) in &dictionary.entries {
                if known.iter().any(|k| k == surface) {
                    continue;
                }
                let kata = hira_to_kata(reading);
                let accent = self.query(&kata, "3")?["accent_phrases"][0]["accent"]
                    .as_i64()
                    .unwrap_or(0);
                self.agent
                    .post(&format!("{}/user_dict_word", self.url))
                    .query("surface", surface)
                    .query("pronunciation", &kata)
                    .query("accent_type", &accent.to_string())
                    .call()
                    .map_err(err)?;
            }
            Ok(())
        }
    }

    impl SpeechEngine for Voicevox {
        fn kind(&self) -> EngineKind {
            EngineKind::Voicevox
        }
        fn cache_id(&self) -> String {
            format!("voicevox/{}", self.url)
        }
        fn native_speed(&self) -> bool {
            true
        }
        fn synthesize(&self, req: &VoiceRequest, text: &str) -> Result<AudioBuffer> {
            let speaker = req.speaker.as_deref().unwrap_or("3");
            let mut q = self.query(text, speaker)?;
            q["speedScale"] = req.speed.into();
            let mut wav = Vec::new();
            std::io::Read::read_to_end(
                &mut self
                    .agent
                    .post(&format!("{}/synthesis", self.url))
                    .query("speaker", speaker)
                    .query("enable_katakana_english", "false")
                    .send_json(q)
                    .map_err(err)?
                    .into_reader(),
                &mut wav,
            )?;
            Ok(mmm_audio::decode::decode_bytes(wav, "wav")?)
        }
        fn voices(&self) -> Vec<String> {
            let Ok(resp) = self.agent.get(&format!("{}/speakers", self.url)).call() else {
                return Vec::new();
            };
            let Ok(list) = resp.into_json::<Value>() else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for s in list.as_array().into_iter().flatten() {
                for st in s["styles"].as_array().into_iter().flatten() {
                    out.push(format!(
                        "{}（{}）= {}",
                        s["name"].as_str().unwrap_or("?"),
                        st["name"].as_str().unwrap_or("?"),
                        st["id"]
                    ));
                }
            }
            out
        }
    }
}
