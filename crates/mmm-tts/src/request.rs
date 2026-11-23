use mmm_core::{CharacterMaster, Line, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineKind {
    /// VOICEVOX エンジン（HTTP、既定）。
    Voicevox,
    /// Kokoro-82M。
    Kokoro,
    /// 推定の長さの無音（モデルなしの動作確認・CI 用）。
    Silent,
}

impl EngineKind {
    pub const ALL: [Self; 3] = [Self::Voicevox, Self::Kokoro, Self::Silent];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "voicevox" => Some(Self::Voicevox),
            "kokoro" => Some(Self::Kokoro),
            "silent" | "none" => Some(Self::Silent),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Voicevox => "voicevox",
            Self::Kokoro => "kokoro",
            Self::Silent => "silent",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Voicevox => "VOICEVOX（既定・要エンジン起動）",
            Self::Kokoro => "Kokoro-82M（軽量・オフライン）",
            Self::Silent => "無音（タイミング確認用）",
        }
    }

    /// 言語ごとの既定話者。
    pub fn default_speaker(self, language: &str) -> Option<&'static str> {
        let ja = language.starts_with("ja");
        match self {
            Self::Voicevox => Some("3"),
            Self::Kokoro => Some(if ja { "jf_alpha" } else { "af_heart" }),
            Self::Silent => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceRequest {
    pub engine: EngineKind,
    pub text: String,
    pub speaker: Option<String>,
    /// `ja` / `en` など。
    pub language: Option<String>,
    pub instruct: Option<String>,
    pub speed: f64,
    pub volume: f64,
    /// 声の高さの倍率（1.0 = そのまま）。
    pub pitch: f64,
}

/// かな・漢字を含めば日本語、それ以外は英語とみなす。
pub fn detect_language(text: &str) -> &'static str {
    let japanese = text.chars().any(|c| {
        matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xff66..=0xff9f)
    });
    if japanese { "ja" } else { "en" }
}

/// 台本の 1 セリフから合成要求を作る。キャラクター定義にない話者は `None`（Python 版と同じく飛ばす）。
///
/// エンジンの優先順位: `forced`（CLI の `--engine` 等）→ キャラクターの `voice.engine` → `default`。
pub fn request_for_line(
    master: &CharacterMaster,
    scene: &Scene,
    line: &Line,
    forced: Option<EngineKind>,
    default: EngineKind,
) -> Option<VoiceRequest> {
    let character = master.get(&line.speaker)?;
    let voice = character.voice.clone().unwrap_or_default();
    let configured = voice.engine.as_deref().and_then(EngineKind::parse);
    let text = if line.read.is_empty() {
        &line.text
    } else {
        &line.read
    };
    let engine = forced.or(configured).unwrap_or(default);
    let language = voice
        .language
        .clone()
        .filter(|l| l != "auto")
        .unwrap_or_else(|| detect_language(text).to_owned());
    // エンジンを上書きしたときは、別エンジン用の話者名を使わない
    let own_voice = configured.is_none_or(|c| c == engine);
    let speaker = match engine {
        EngineKind::Voicevox => voice
            .speaker
            .clone()
            .filter(|s| own_voice && s.parse::<u32>().is_ok())
            .or_else(|| character.speaker_id.map(|i| i.to_string())),
        _ => voice.speaker.clone().filter(|_| own_voice),
    }
    .or_else(|| engine.default_speaker(&language).map(str::to_owned));
    // VOICEVOX は Python 版と同じく既定 1.2 倍速・音量 1.2 倍
    let (base_speed, base_volume) = if engine == EngineKind::Voicevox {
        (1.2, 1.2)
    } else {
        (1.0, 1.0)
    };
    let speed =
        scene.speed.unwrap_or(base_speed) * voice.speed.unwrap_or(1.0) * line.speed.unwrap_or(1.0);
    let _ = (&line.instruct, &voice.instruct);
    Some(VoiceRequest {
        engine,
        text: text.clone(),
        speaker,
        language: Some(language),
        instruct: None,
        speed,
        volume: base_volume * voice.volume.unwrap_or(1.0) * line.volume.unwrap_or(1.0),
        pitch: voice.pitch.unwrap_or(1.0) * line.pitch.unwrap_or(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmm_core::parse_script;

    fn master() -> CharacterMaster {
        serde_json::from_str(
            r#"{"z": {"speaker_id": 3, "voice": {"speed": 1.1}},
                "e": {"voice": {"engine": "kokoro", "speaker": "af_bella"}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn resolves_engine_speaker_language_and_speed() {
        let p = parse_script(
            "# Scene\n- speed: 1.2\n## Lines\nz: こんにちは\ne: Hello there\nx: 不明\nz: ｜詐欺《さぎ》 {pitch: 1.2, volume: 0.5, speed: 2}\n",
        );
        let s = &p.script.scenes[0];
        let m = master();
        let z = request_for_line(&m, s, &s.lines[0], None, EngineKind::Voicevox).unwrap();
        assert_eq!(z.engine, EngineKind::Voicevox);
        assert_eq!(z.speaker.as_deref(), Some("3"));
        assert_eq!(z.language.as_deref(), Some("ja"));
        assert!((z.speed - 1.32).abs() < 1e-9);
        let e = request_for_line(&m, s, &s.lines[1], None, EngineKind::Voicevox).unwrap();
        assert_eq!(
            (e.engine, e.speaker.as_deref(), e.language.as_deref()),
            (EngineKind::Kokoro, Some("af_bella"), Some("en"))
        );
        // Kokoro へ強制すると既定話者（日本語なら jf_alpha）になる
        let k = request_for_line(
            &m,
            s,
            &s.lines[0],
            Some(EngineKind::Kokoro),
            EngineKind::Voicevox,
        )
        .unwrap();
        assert_eq!(k.speaker.as_deref(), Some("jf_alpha"));
        // Kokoro 指定のキャラクターを VOICEVOX へ強制すると既定話者 ID ("3") になる
        let v = request_for_line(
            &m,
            s,
            &s.lines[1],
            Some(EngineKind::Voicevox),
            EngineKind::Voicevox,
        )
        .unwrap();
        assert_eq!(v.speaker.as_deref(), Some("3"));
        assert!(request_for_line(&m, s, &s.lines[2], None, EngineKind::Voicevox).is_none());
        let r = request_for_line(&m, s, &s.lines[3], None, EngineKind::Voicevox).unwrap();
        assert_eq!(r.text, "さぎ");
        assert_eq!((r.pitch, r.volume), (1.2, 0.6));
        assert!((r.speed - 2.64).abs() < 1e-9);
    }
}
