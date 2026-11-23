//! `mmm` — 台本（Markdown）から解説動画を作る CLI。Python 版 `s2v` の後継。
mod tools;

use clap::{Args, Parser, Subcommand};
use mmm_pipeline::{
    Backend, Options, Progress, Project, available_engines, build_timeline, mixer, render_audio,
    render_images, render_video, synthesize_all,
};
use mmm_tts::EngineKind;
use std::{io::Write, path::PathBuf, process::ExitCode, sync::atomic::AtomicBool};

#[derive(Parser)]
#[command(
    name = "mmm",
    version,
    about = "台本（Markdown）から音声合成つきの解説動画（MP4）を作ります"
)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Subcommand)]
enum Command {
    /// 台本から動画・音声・静止画を作る（サブコマンド省略時と同じ）
    Render(Box<RenderArgs>),
    /// エンジンの話者一覧を表示する
    Voices {
        #[arg(long, default_value = "voicevox")]
        engine: String,
        #[command(flatten)]
        engine_opts: EngineArgs,
    },
    /// CSV（image1_path,image2_path,output_path,duration）から 2 枚交互の GIF を作る
    Gif { csv: PathBuf },
    /// 画像（PNG/GIF）を跳ねさせる GIF を作る
    BounceGif {
        input: PathBuf,
        #[arg(long, default_value = "bouncing.gif")]
        out: PathBuf,
        #[arg(long, default_value_t = 20)]
        height: u32,
        #[arg(long, default_value_t = 0.5)]
        duration: f64,
    },
    /// 既存のバリエーションの画像を複製して新しいバリエーションを登録する
    Variation(tools::VariationArgs),
    /// フォルダー内の画像を左右反転して `<名前>_left.<拡張子>` で保存する（ImageMagick 不要）
    Flip {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, default_value = "_left")]
        suffix: String,
        #[arg(long, default_value = "png")]
        ext: String,
    },
}

#[derive(Args, Clone)]
struct EngineArgs {
    /// TTS を動かすデバイス（auto / cpu / cuda:0）
    #[arg(long, default_value = "auto")]
    device: String,
    /// Kokoro のモデル（Hugging Face ID かローカルディレクトリ）
    #[arg(long, default_value = "hexgrad/Kokoro-82M")]
    kokoro_model: String,
    #[arg(long, default_value = "http://127.0.0.1:50021")]
    voicevox_url: String,
}

impl EngineArgs {
    fn settings(&self) -> mmm_tts::EngineSettings {
        mmm_tts::EngineSettings {
            kokoro_model: self.kokoro_model.clone(),
            device: self.device.clone(),
            voicevox_url: self.voicevox_url.clone(),
        }
    }
}

#[derive(Args, Clone)]
struct RenderArgs {
    /// 台本ファイル。複数指定すると順につなげて 1 本の動画にする
    scripts: Vec<PathBuf>,
    /// 出力ファイル（.mp4 / --audio-only は .mp3・.wav / --image-only は連番 PNG の名前）
    #[arg(long, short, default_value = "output.mp4")]
    out: PathBuf,
    /// キャラクター定義（既定: <root>/data/character_master.json）
    #[arg(long)]
    chars: Option<PathBuf>,
    /// 字幕フォント（既定: 埋め込みの M+ 1 Bold）
    #[arg(long = "font", alias = "font_path")]
    font: Option<PathBuf>,
    /// 全体 BGM（既定: <root>/data/bgm/default_bgm.mp3 があれば使う）
    #[arg(long = "bgm", alias = "bgm_path")]
    bgm: Option<String>,
    /// 全体 BGM を使わない
    #[arg(long)]
    no_bgm: bool,
    /// BGM の音量（Python 版と同じ 0.1）
    #[arg(long, default_value_t = 0.1)]
    bgm_volume: f64,
    /// 台本中の相対パスの基準（既定: data/character_master.json を持つ祖先ディレクトリ）
    #[arg(long)]
    root: Option<PathBuf>,
    /// 全キャラクターに使う TTS エンジン（voicevox / kokoro / silent）。省略時はキャラクター定義に従う
    #[arg(long)]
    engine: Option<String>,
    /// キャラクター定義でエンジン未指定のときのエンジン
    #[arg(long, default_value = "voicevox")]
    default_engine: String,
    #[command(flatten)]
    engine_opts: EngineArgs,
    /// 解像度（幅x高さ）
    #[arg(long, default_value = "1920x1080")]
    size: String,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    /// 動画エンコーダ（auto / software / mf）。auto は Windows で Media Foundation（GPU）
    #[arg(long, default_value = "auto")]
    encoder: String,
    /// 純 Rust エンコーダを速さ優先にする（下書き向け。ファイルは大きくなる）
    #[arg(long)]
    fast_encode: bool,
    /// 合成音声のキャッシュ先
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    /// キャッシュを使わない
    #[arg(long)]
    no_cache: bool,
    /// 音声だけを書き出す（MP3、拡張子 .wav なら WAV）
    #[arg(long = "audio-only", alias = "audio_only")]
    audio_only: bool,
    /// セリフごとの静止画（PNG）を書き出す（音声合成しない）
    #[arg(long = "image-only", alias = "image_only")]
    image_only: bool,
}

fn engine(s: &str) -> Result<EngineKind, String> {
    let kind = EngineKind::parse(s)
        .ok_or_else(|| format!("未知のエンジン: {s}（voicevox / kokoro / silent）"))?;
    if !available_engines().contains(&kind) {
        return Err(format!("エンジン {s} はこのビルドに含まれていません"));
    }
    Ok(kind)
}

fn options(a: &RenderArgs, root: &std::path::Path) -> Result<Options, String> {
    let (w, h) = a
        .size
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|&(w, h): &(u32, u32)| w >= 16 && h >= 16)
        .ok_or_else(|| format!("解像度は 1920x1080 の形式で指定してください: {}", a.size))?;
    let default_bgm = "data/bgm/default_bgm.mp3";
    let bgm = if a.no_bgm {
        None
    } else {
        a.bgm.clone().or_else(|| {
            root.join(default_bgm)
                .exists()
                .then(|| default_bgm.to_owned())
        })
    };
    Ok(Options {
        width: w,
        height: h,
        fps: a.fps.clamp(1, 120),
        force_engine: a.engine.as_deref().map(engine).transpose()?,
        // 既定エンジンは合成するときに使えるか確認する（静止画の書き出しでは不要）
        default_engine: EngineKind::parse(&a.default_engine)
            .ok_or_else(|| format!("未知のエンジン: {}", a.default_engine))?,
        engine: a.engine_opts.settings(),
        bgm,
        bgm_gain: a.bgm_volume,
        font: a.font.clone(),
        fast_encode: a.fast_encode,
        backend: Backend::parse(&a.encoder)
            .ok_or_else(|| format!("未知のエンコーダ: {}", a.encoder))?,
        cache_dir: if a.no_cache {
            None
        } else {
            a.cache_dir.clone().or_else(mmm_pipeline::default_cache_dir)
        },
    })
}

fn progress(p: Progress) {
    let mut err = std::io::stderr();
    match p {
        Progress::Synth { done, total, text } if done < total => {
            let _ = writeln!(err, "[音声 {}/{total}] {text}", done + 1);
        }
        Progress::Synth { .. } => {}
        Progress::Frames { done, total } => {
            let _ = write!(
                err,
                "\r[映像] {done}/{total} フレーム ({:.0}%)",
                done as f64 * 100.0 / total.max(1) as f64
            );
            if done >= total {
                let _ = writeln!(err);
            }
        }
        Progress::Message(m) => {
            let _ = writeln!(err, "{m}");
        }
    }
}

fn run_render(a: RenderArgs) -> Result<(), String> {
    let Some(first) = a.scripts.first() else {
        return Err("台本ファイルを指定してください（mmm --help）".into());
    };
    let root = a
        .root
        .clone()
        .unwrap_or_else(|| mmm_pipeline::find_project_root(first));
    let mut text = String::new();
    for s in &a.scripts {
        text.push_str(&std::fs::read_to_string(s).map_err(|e| format!("{}: {e}", s.display()))?);
        text.push('\n');
    }
    let project =
        Project::from_text(&text, root.clone(), a.chars.as_deref()).map_err(|e| e.to_string())?;
    for d in &project.parsed.diagnostics {
        eprintln!("警告: {} 行目: {}", d.line + 1, d.message);
    }
    for (si, scene) in project.parsed.script.scenes.iter().enumerate() {
        for line in &scene.lines {
            if !project.master.contains_key(&line.speaker) {
                eprintln!(
                    "警告: シーン {} のキャラクター `{}` が定義にないため飛ばします",
                    si + 1,
                    line.speaker
                );
            }
        }
    }
    let opts = options(&a, &root)?;
    let renderer = project.renderer(&opts).map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    if a.image_only {
        let (timeline, lipsync) = build_timeline(&project, &Default::default(), &opts, true);
        let files = render_images(&project, &renderer, &timeline, &lipsync, &a.out)
            .map_err(|e| e.to_string())?;
        for f in files {
            println!("{}", f.display());
        }
        return Ok(());
    }
    let synth = project.synthesizer(&opts);
    let voices =
        synthesize_all(&project, &synth, &opts, &progress, &cancel).map_err(|e| e.to_string())?;
    for ((s, l), e) in &voices.failures {
        eprintln!(
            "エラー: シーン {} のセリフ {} を合成できません: {e}",
            s + 1,
            l + 1
        );
    }
    if voices.clips.is_empty() && !voices.failures.is_empty() {
        return Err("音声を 1 つも合成できませんでした".into());
    }
    let (timeline, lipsync) = build_timeline(&project, &voices, &opts, false);
    let mix = mixer(&project, &timeline, &voices);
    for w in &mix.warnings {
        eprintln!("警告: {w}");
    }
    if a.audio_only {
        let out = if a.out.extension().is_some_and(|e| e == "mp4") {
            a.out.with_extension("mp3")
        } else {
            a.out.clone()
        };
        render_audio(&mix, &out).map_err(|e| e.to_string())?;
        println!("{}", out.display());
        return Ok(());
    }
    render_video(
        &project, &timeline, &lipsync, &mix, &renderer, &opts, &a.out, &progress, &cancel,
    )
    .map_err(|e| e.to_string())?;
    for m in renderer.assets.missing() {
        eprintln!("警告: 画像を読めません: {m}");
    }
    println!("{}", a.out.display());
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        None => run_render(cli.render),
        Some(Command::Render(a)) => run_render(*a),
        Some(Command::Voices {
            engine: e,
            engine_opts,
        }) => engine(&e).and_then(|kind| {
            let synth = mmm_tts::Synthesizer::new(engine_opts.settings(), Default::default(), None);
            let engine = synth.engine(kind).map_err(|e| e.to_string())?;
            for v in engine.voices() {
                println!("{v}");
            }
            Ok(())
        }),
        Some(Command::Gif { csv }) => tools::gifs_from_csv(&csv),
        Some(Command::BounceGif {
            input,
            out,
            height,
            duration,
        }) => tools::bounce_gif(&input, &out, height, duration),
        Some(Command::Variation(a)) => tools::variation(&a),
        Some(Command::Flip { dir, suffix, ext }) => tools::flip(&dir, &suffix, &ext),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("エラー: {e}");
            ExitCode::FAILURE
        }
    }
}
