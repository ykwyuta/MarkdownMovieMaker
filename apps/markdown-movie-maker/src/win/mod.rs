//! Win32 の画面（PowerPoint のようなスライド形式）。
//!
//! ```text
//! ┌ メニュー ───────────────────────────────────────────────────┐
//! │ [音声を合成][このセリフ][書き出し][中止] エンジン▼   状態            │
//! ├ スライド ┬ プレビュー（選んだシーン） ─────┬ 台本エディタ ─────┤
//! │ ┌────┐ │                                  │ （yyeditor の     │
//! │ │ 1  │ ├ [◀][▶ このシーン][▶ 通して][▶] 時刻 │   Direct2D 部品）│
//! │ └────┘ ├ ノート（このシーンのセリフ） ──────┤                  │
//! │ ┌────┐ ├ タイムライン ──────────────────────┼ メッセージ ──────┤
//! └ ステータスバー ──────────────────────────────────────────────┘
//! ```
#![allow(unsafe_code)]
mod player;
mod preview;
mod slides;
mod timeline;

use crate::studio::{Event, Row, Slide, Studio, fmt_time};
use mmm_pipeline::{Backend, available_engines};
use mmm_tts::EngineKind;
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree,
            },
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Controls::*,
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
                SetProcessDpiAwarenessContext,
            },
            Input::KeyboardAndMouse::*,
            Shell::{
                Common::COMDLG_FILTERSPEC, FileOpenDialog, FileSaveDialog, IFileOpenDialog,
                IFileSaveDialog, SIGDN_FILESYSPATH, ShellExecuteW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, Result, w},
};
use yy_core::{Selection, SelectionSet};
use yy_ui_win::editor::Editor;

const APP_NAME: &str = "MarkdownMovieMaker";
/// 実行ファイルに埋め込んだアイコンのリソース ID（build.rs と同じ）。
const APP_ICON_ID: u16 = 1;
const FRAME: PCWSTR = w!("MarkdownMovieMaker");
const PREVIEW: PCWSTR = w!("MarkdownMovieMakerPreview");
const TIMELINE: PCWSTR = w!("MarkdownMovieMakerTimeline");
const EDITOR: PCWSTR = w!("MarkdownMovieMakerEditor");
const SLIDES: PCWSTR = w!("MarkdownMovieMakerSlides");

/// バックグラウンド処理（合成・書き出し・再生の準備）の通知。
const WM_NOTIFY_APP: u32 = WM_APP + 1;
/// プレビューのフレームができた（一覧などは作り直さず、プレビューだけ描き直す）。
const WM_FRAME_READY: u32 = WM_APP + 2;
/// サムネイルができた。
const WM_THUMBS_READY: u32 = WM_APP + 3;

/// `msg` を投げる通知。前の通知が処理される前に何度呼ばれても 1 回にまとめる。
fn poster(
    hwnd_raw: isize,
    msg: u32,
) -> (
    Arc<dyn Fn() + Send + Sync>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let pending = Arc::new(AtomicBool::new(false));
    let flag = pending.clone();
    let post: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if !flag.swap(true, Ordering::AcqRel) {
            let _ =
                unsafe { PostMessageW(Some(HWND(hwnd_raw as *mut _)), msg, WPARAM(0), LPARAM(0)) };
        }
    });
    (post, pending)
}
const TIMER_TICK: usize = 1;

// ツールバー・再生操作・一覧
const ID_PLAY_SCENE: u16 = 100;
const ID_PLAY_ALL: u16 = 101;
const ID_PREV: u16 = 102;
const ID_SYNTH: u16 = 103;
const ID_SYNTH_LINE: u16 = 104;
const ID_EXPORT: u16 = 105;
const ID_CANCEL: u16 = 106;
const ID_ENGINE: u16 = 107;
const ID_TIME: u16 = 108;
const ID_NEXT: u16 = 109;
const ID_NOTES: u16 = 110;
const ID_LOG: u16 = 111;
const ID_STATUS: u16 = 112;
// メニュー
const M_NEW: u16 = 201;
const M_OPEN: u16 = 202;
const M_SAVE: u16 = 203;
const M_SAVE_AS: u16 = 204;
const M_EXPORT: u16 = 205;
const M_EXIT: u16 = 206;
const M_RELOAD: u16 = 207;
const M_CHARS: u16 = 208;
const M_SYNTAX: u16 = 209;
const M_HELP: u16 = 210;
const M_SHORTCUTS: u16 = 211;
const M_ABOUT: u16 = 212;

/// 同梱のヘルプ（yyeditor と同じく Markdown を WebView2 で表示する）。
const HELP_MD: &str = include_str!("../../help/help.md");
const M_AUDITION: u16 = 220; // + キャラクター番号
const M_SIZE: u16 = 300; // + 0..3
const M_FPS: u16 = 310; // + 0..4
const M_ENCODER: u16 = 320; // + 0..3
const M_DEVICE: u16 = 330; // + 0..3

const SIZES: [(u32, u32); 3] = [(1920, 1080), (1280, 720), (3840, 2160)];
const FPS: [u32; 4] = [24, 30, 60, 12];
const ENCODERS: [(Backend, &str); 3] = [
    (Backend::Auto, "自動（Media Foundation / GPU）"),
    (
        Backend::MediaFoundation,
        "Media Foundation（H.264/AAC, GPU）",
    ),
    (Backend::Software, "純 Rust（rusty_h264 + Opus）"),
];
const DEVICES: [&str; 3] = ["auto", "cuda:0", "cpu"];

#[derive(Clone)]
struct Controls {
    play_scene: HWND,
    play_all: HWND,
    status_text: HWND,
    engine: HWND,
    time: HWND,
    notes: HWND,
    log: HWND,
    status: HWND,
    preview: HWND,
    timeline: HWND,
    editor: HWND,
    slides: HWND,
    toolbar: Vec<HWND>,
    transport: Vec<HWND>,
}

struct App {
    hwnd: HWND,
    c: Controls,
    dpi: u32,
    font: HFONT,
    editor: Editor,
    editor_version: u64,
    edited_at: Option<Instant>,
    caret_line: usize,
    studio: Studio,
    rx: mpsc::Receiver<Event>,
    preview: preview::Worker,
    thumbs: preview::Thumbs,
    thumbs_generation: usize,
    thumb_keys: Vec<u64>,
    frame_pending: Arc<std::sync::atomic::AtomicBool>,
    thumbs_pending: Arc<std::sync::atomic::AtomicBool>,
    bars_generation: usize,
    requested: Option<(f64, usize)>,
    player: player::Player,
    playing: bool,
    /// シーンだけを再生しているとき、その終わりの時刻。
    play_end: Option<f64>,
    clock: Option<(Instant, f64)>,
    slides: Vec<Slide>,
    slides_view: slides::View,
    selected: usize,
    notes: Vec<Row>,
    bars: Vec<crate::studio::Bar>,
    view: timeline::View,
    engines: Vec<Option<EngineKind>>,
    missing: Vec<String>,
    log_len: usize,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<T>(f: impl FnOnce(&mut App) -> T) -> Option<T> {
    APP.with(|a| a.try_borrow_mut().ok()?.as_mut().map(f))
}

pub fn message_box(text: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(APP_NAME),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn send(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

fn set_text(h: HWND, s: &str) {
    unsafe {
        let _ = SetWindowTextW(h, &HSTRING::from(s));
    }
}

fn invalidate(h: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(h), None, false);
    }
}

fn client(h: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(h, &mut r);
    }
    r
}

/// STATIC の左寄せ・上下中央（`ellipsis` なら末尾を … で切る）。
fn static_style(ellipsis: bool) -> u32 {
    // SS_LEFT (0) | SS_CENTERIMAGE（| SS_ENDELLIPSIS）
    let s = 0x0200;
    if ellipsis { s | 0x4000 } else { s }
}

fn control_font(dpi: u32) -> HFONT {
    yy_ui_win::font::register_gdi();
    unsafe {
        CreateFontW(
            -(14 * dpi as i32 / 96),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_MODERN.0 as u32,
            &HSTRING::from(yy_ui_win::font::BUNDLED_FAMILY),
        )
    }
}

/// ファイルを開く・保存するダイアログ。`types` は (表示名, 拡張子) の並び。
fn file_dialog(
    hwnd: HWND,
    save: bool,
    types: &[(&str, &str)],
    default_name: Option<&str>,
) -> Option<PathBuf> {
    unsafe {
        let names: Vec<HSTRING> = types.iter().map(|(n, _)| HSTRING::from(*n)).collect();
        let specs: Vec<HSTRING> = types
            .iter()
            .map(|(_, e)| HSTRING::from(format!("*.{e}")))
            .collect();
        let filter: Vec<COMDLG_FILTERSPEC> = names
            .iter()
            .zip(&specs)
            .map(|(n, s)| COMDLG_FILTERSPEC {
                pszName: PCWSTR(n.as_ptr()),
                pszSpec: PCWSTR(s.as_ptr()),
            })
            .collect();
        let item = if save {
            let d: IFileSaveDialog =
                CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            d.SetFileTypes(&filter).ok()?;
            d.SetDefaultExtension(&HSTRING::from(types.first()?.1))
                .ok()?;
            if let Some(n) = default_name {
                d.SetFileName(&HSTRING::from(n)).ok()?;
            }
            d.Show(Some(hwnd)).ok()?;
            d.GetResult().ok()?
        } else {
            let d: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            d.SetFileTypes(&filter).ok()?;
            d.Show(Some(hwnd)).ok()?;
            d.GetResult().ok()?
        };
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0.cast()));
        path
    }
}

fn open_in_shell(path: &std::path::Path) {
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

// ---- 起動 -------------------------------------------------------------------

pub fn run(script: Option<PathBuf>) -> std::result::Result<(), String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        });
    }
    create().map_err(|e| e.to_string())?;
    if let Some(path) = script {
        open_script(&path);
    }
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_KEYDOWN && shortcut(msg.wParam.0 as u32) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// アプリ全体のショートカット。処理したら true。
fn shortcut(key: u32) -> bool {
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    let Some(hwnd) = with_app(|a| a.hwnd) else {
        return false;
    };
    let is = |v: VIRTUAL_KEY| key == u32::from(v.0);
    let cmd = match (ctrl, shift) {
        (false, false) if is(VK_F1) => M_HELP,
        (false, false) if is(VK_F5) => ID_PLAY_SCENE,
        (false, true) if is(VK_F5) => ID_PLAY_ALL,
        (false, false) if is(VK_F6) => ID_SYNTH,
        (false, false) if is(VK_F7) => ID_SYNTH_LINE,
        (true, _) if is(VK_PRIOR) => ID_PREV,
        (true, _) if is(VK_NEXT) => ID_NEXT,
        (true, false) if key == 'S' as u32 => M_SAVE,
        (true, false) if key == 'O' as u32 => M_OPEN,
        (true, false) if key == 'E' as u32 => M_EXPORT,
        (true, false) if key == 'N' as u32 => M_NEW,
        _ => return false,
    };
    command(hwnd, cmd);
    true
}

fn create() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let classes: [(PCWSTR, WNDPROC, SYS_COLOR_INDEX); 5] = [
            (FRAME, Some(frame_proc), COLOR_BTNFACE),
            (PREVIEW, Some(preview_proc), COLOR_WINDOW),
            (TIMELINE, Some(timeline_proc), COLOR_WINDOW),
            (EDITOR, Some(editor_proc), COLOR_WINDOW),
            (SLIDES, Some(slides_proc), COLOR_WINDOW),
        ];
        // MAKEINTRESOURCE(APP_ICON_ID)
        let icon = LoadIconW(
            Some(instance.into()),
            PCWSTR(APP_ICON_ID as usize as *const u16),
        )
        .unwrap_or_default();
        for (name, proc, brush) in classes {
            let class = WNDCLASSW {
                hIcon: icon,
                style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
                hInstance: instance.into(),
                lpszClassName: name,
                lpfnWndProc: proc,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: GetSysColorBrush(brush),
                ..Default::default()
            };
            RegisterClassW(&class);
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            FRAME,
            &HSTRING::from(APP_NAME),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1680,
            1000,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let dpi = GetDpiForWindow(hwnd).max(96);
        let font = control_font(dpi);
        let child = |class: PCWSTR,
                     label: &str,
                     id: u16,
                     style: u32,
                     ex: WINDOW_EX_STYLE|
         -> Result<HWND> {
            let h = CreateWindowExW(
                ex,
                class,
                &HSTRING::from(label),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                0,
                0,
                1,
                1,
                Some(hwnd),
                Some(HMENU(id as usize as *mut _)),
                Some(instance.into()),
                None,
            )?;
            send(h, WM_SETFONT, font.0 as usize, 1);
            Ok(h)
        };
        let button = |label: &str, id: u16| {
            child(
                w!("BUTTON"),
                label,
                id,
                BS_PUSHBUTTON as u32 | WS_TABSTOP.0,
                WINDOW_EX_STYLE::default(),
            )
        };
        let toolbar = vec![
            button("音声を合成 (F6)", ID_SYNTH)?,
            button("このセリフ (F7)", ID_SYNTH_LINE)?,
            button("書き出し…", ID_EXPORT)?,
            button("中止", ID_CANCEL)?,
        ];
        let play_scene = button("▶ このシーン (F5)", ID_PLAY_SCENE)?;
        let play_all = button("▶ 通して再生 (Shift+F5)", ID_PLAY_ALL)?;
        let transport = vec![
            button("◀ 前", ID_PREV)?,
            play_scene,
            play_all,
            button("次 ▶", ID_NEXT)?,
        ];
        let engine = child(
            w!("COMBOBOX"),
            "",
            ID_ENGINE,
            CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | WS_TABSTOP.0,
            WINDOW_EX_STYLE::default(),
        )?;
        let time = child(
            w!("STATIC"),
            "0:00.0 / 0:00.0",
            ID_TIME,
            static_style(false),
            WINDOW_EX_STYLE::default(),
        )?;
        let status_text = child(
            w!("STATIC"),
            "",
            0,
            static_style(true),
            WINDOW_EX_STYLE::default(),
        )?;
        let list_style =
            LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL.0 | WS_HSCROLL.0;
        let notes = child(w!("LISTBOX"), "", ID_NOTES, list_style, WS_EX_CLIENTEDGE)?;
        let log = child(w!("LISTBOX"), "", ID_LOG, list_style, WS_EX_CLIENTEDGE)?;
        let status = child(
            STATUSCLASSNAMEW,
            "",
            ID_STATUS,
            0,
            WINDOW_EX_STYLE::default(),
        )?;
        let slides_hwnd = child(SLIDES, "", 0, WS_TABSTOP.0, WS_EX_CLIENTEDGE)?;
        let preview_hwnd = child(PREVIEW, "", 0, 0, WINDOW_EX_STYLE::default())?;
        let timeline_hwnd = child(TIMELINE, "", 0, 0, WS_EX_CLIENTEDGE)?;
        let editor_hwnd = child(EDITOR, "", 0, WS_TABSTOP.0, WS_EX_CLIENTEDGE)?;

        let (tx, rx) = mpsc::channel();
        let hwnd_raw = hwnd.0 as isize;
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = PostMessageW(
                Some(HWND(hwnd_raw as *mut _)),
                WM_NOTIFY_APP,
                WPARAM(0),
                LPARAM(0),
            );
        });
        let (frame_notify, frame_pending) = poster(hwnd_raw, WM_FRAME_READY);
        let (thumbs_notify, thumbs_pending) = poster(hwnd_raw, WM_THUMBS_READY);
        let studio = Studio::new(tx, notify.clone());
        let mut editor = Editor::new(&studio.text, dpi)?;
        editor.set_language(Some("markdown"));
        let mut engines: Vec<Option<EngineKind>> = vec![None];
        send(
            engine,
            CB_ADDSTRING,
            0,
            HSTRING::from("音声: キャラクター定義に従う").as_ptr() as isize,
        );
        for e in available_engines() {
            send(
                engine,
                CB_ADDSTRING,
                0,
                HSTRING::from(format!("音声: {}", e.label())).as_ptr() as isize,
            );
            engines.push(Some(e));
        }
        send(engine, CB_SETCURSEL, 0, 0);
        let player = player::Player::new();
        let mut app = App {
            hwnd,
            c: Controls {
                play_scene,
                play_all,
                status_text,
                engine,
                time,
                notes,
                log,
                status,
                preview: preview_hwnd,
                timeline: timeline_hwnd,
                editor: editor_hwnd,
                slides: slides_hwnd,
                toolbar,
                transport,
            },
            dpi,
            font,
            editor_version: editor.document.version(),
            editor,
            edited_at: None,
            caret_line: 0,
            studio,
            rx,
            preview: preview::Worker::new(frame_notify),
            thumbs: preview::Thumbs::new(thumbs_notify),
            thumbs_generation: usize::MAX,
            thumb_keys: Vec::new(),
            frame_pending,
            thumbs_pending,
            bars_generation: usize::MAX,
            requested: None,
            player,
            playing: false,
            play_end: None,
            clock: None,
            slides: Vec::new(),
            slides_view: slides::View { scroll: 0 },
            selected: 0,
            notes: Vec::new(),
            bars: Vec::new(),
            view: timeline::View::default(),
            engines,
            missing: Vec::new(),
            log_len: 0,
        };
        if let Some(e) = app.player.error.clone() {
            app.studio.log(e);
        }
        APP.with(|a| *a.borrow_mut() = Some(app));
        SetMenu(hwnd, Some(build_menu()?))?;
        with_app(|a| {
            a.refresh_lists();
            a.refresh_title();
        });
        layout(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        SetTimer(Some(hwnd), TIMER_TICK, 16, None);
        let _ = SetFocus(Some(editor_hwnd));
        Ok(())
    }
}

fn build_menu() -> Result<HMENU> {
    unsafe {
        let root = CreateMenu()?;
        let add = |menu: HMENU, id: u16, label: &str| {
            AppendMenuW(menu, MF_STRING, id as usize, &HSTRING::from(label))
        };
        let file = CreatePopupMenu()?;
        add(file, M_NEW, "新規(&N)\tCtrl+N")?;
        add(file, M_OPEN, "開く(&O)…\tCtrl+O")?;
        add(file, M_SAVE, "保存(&S)\tCtrl+S")?;
        add(file, M_SAVE_AS, "名前を付けて保存(&A)…")?;
        AppendMenuW(file, MF_SEPARATOR, 0, None)?;
        add(file, M_EXPORT, "書き出し(&E)…\tCtrl+E")?;
        AppendMenuW(file, MF_SEPARATOR, 0, None)?;
        add(file, M_EXIT, "終了(&X)")?;
        AppendMenuW(root, MF_POPUP, file.0 as usize, w!("ファイル(&F)"))?;

        let play = CreatePopupMenu()?;
        add(play, ID_PLAY_SCENE, "このシーンを再生・停止(&P)\tF5")?;
        add(play, ID_PLAY_ALL, "通して再生・停止(&A)\tShift+F5")?;
        add(play, ID_PREV, "前のシーン(&B)\tCtrl+PageUp")?;
        add(play, ID_NEXT, "次のシーン(&N)\tCtrl+PageDown")?;
        AppendMenuW(root, MF_POPUP, play.0 as usize, w!("再生(&R)"))?;

        let project = CreatePopupMenu()?;
        add(project, M_RELOAD, "素材・キャラクター定義を読み直す(&R)")?;
        add(project, M_CHARS, "キャラクター定義を開く(&C)")?;
        let audition = CreatePopupMenu()?;
        let ids: Vec<(String, String)> = with_app(|a| {
            a.studio
                .project
                .as_ref()
                .map(|p| {
                    p.master
                        .iter()
                        .map(|(id, c)| (id.clone(), c.name.clone()))
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
        for (i, (id, name)) in ids.iter().enumerate().take(60) {
            add(audition, M_AUDITION + i as u16, &format!("{name}（{id}）"))?;
        }
        AppendMenuW(project, MF_POPUP, audition.0 as usize, w!("声を試聴(&T)"))?;
        AppendMenuW(root, MF_POPUP, project.0 as usize, w!("プロジェクト(&P)"))?;

        let settings = CreatePopupMenu()?;
        let size = CreatePopupMenu()?;
        for (i, (w, h)) in SIZES.iter().enumerate() {
            add(size, M_SIZE + i as u16, &format!("{w}×{h}"))?;
        }
        AppendMenuW(settings, MF_POPUP, size.0 as usize, w!("書き出しの解像度"))?;
        let fps = CreatePopupMenu()?;
        for (i, f) in FPS.iter().enumerate() {
            add(fps, M_FPS + i as u16, &format!("{f} fps"))?;
        }
        AppendMenuW(settings, MF_POPUP, fps.0 as usize, w!("フレームレート"))?;
        let enc = CreatePopupMenu()?;
        for (i, (_, label)) in ENCODERS.iter().enumerate() {
            add(enc, M_ENCODER + i as u16, label)?;
        }
        AppendMenuW(settings, MF_POPUP, enc.0 as usize, w!("動画エンコーダ"))?;
        let dev = CreatePopupMenu()?;
        for (i, d) in DEVICES.iter().enumerate() {
            add(
                dev,
                M_DEVICE + i as u16,
                &format!("音声合成のデバイス: {d}"),
            )?;
        }
        AppendMenuW(settings, MF_POPUP, dev.0 as usize, w!("音声合成のデバイス"))?;
        AppendMenuW(root, MF_POPUP, settings.0 as usize, w!("設定(&S)"))?;

        let help = CreatePopupMenu()?;
        add(help, M_HELP, "ヘルプ(&H)\tF1")?;
        add(help, M_SYNTAX, "台本の書き方(&S)")?;
        add(help, M_SHORTCUTS, "キーボードショートカット(&K)")?;
        AppendMenuW(help, MF_SEPARATOR, 0, PCWSTR::null())?;
        add(help, M_ABOUT, "バージョン情報(&A)")?;
        AppendMenuW(root, MF_POPUP, help.0 as usize, w!("ヘルプ(&H)"))?;
        Ok(root)
    }
}

fn layout(hwnd: HWND) {
    let Some((c, dpi)) = with_app(|a| (a.c.clone(), a.dpi)) else {
        return;
    };
    unsafe {
        let r = client(hwnd);
        send(c.status, WM_SIZE, 0, 0);
        let mut sr = RECT::default();
        let _ = GetWindowRect(c.status, &mut sr);
        let status_h = sr.bottom - sr.top;
        let s = |v: i32| v * dpi as i32 / 96;
        let (w, h) = (r.right, r.bottom - status_h);
        let bar_h = s(34);
        let mut x = s(6);
        for (b, bw) in c.toolbar.iter().zip([140, 130, 100, 60]) {
            let _ = MoveWindow(*b, x, s(4), s(bw), bar_h - s(8), true);
            x += s(bw) + s(4);
        }
        let _ = MoveWindow(c.engine, x + s(8), s(5), s(300), s(400), true);
        x += s(316);
        let _ = MoveWindow(
            c.status_text,
            x,
            s(4),
            (w - x - s(6)).max(0),
            bar_h - s(8),
            true,
        );

        let body_h = h - bar_h;
        let slides_w = s(250).min(w / 5);
        let editor_w = ((w - slides_w) as f32 * 0.38) as i32;
        let center_x = slides_w;
        let center_w = w - slides_w - editor_w;
        let _ = MoveWindow(c.slides, 0, bar_h, slides_w, body_h, true);
        // 中央: プレビュー / 再生操作 / ノート / タイムライン
        let transport_h = s(34);
        let notes_h = s(110).min(body_h / 5);
        let timeline_h = s(150).min(body_h / 4);
        let preview_h = (body_h - transport_h - notes_h - timeline_h).max(s(100));
        let _ = MoveWindow(c.preview, center_x, bar_h, center_w, preview_h, true);
        let ty = bar_h + preview_h;
        let mut tx = center_x + s(6);
        for (b, bw) in c.transport.iter().zip([60, 150, 190, 60]) {
            let _ = MoveWindow(*b, tx, ty + s(4), s(bw), transport_h - s(8), true);
            tx += s(bw) + s(4);
        }
        let _ = MoveWindow(
            c.time,
            tx + s(8),
            ty + s(4),
            (center_x + center_w - tx - s(14)).max(0),
            transport_h - s(8),
            true,
        );
        let _ = MoveWindow(c.notes, center_x, ty + transport_h, center_w, notes_h, true);
        let _ = MoveWindow(
            c.timeline,
            center_x,
            ty + transport_h + notes_h,
            center_w,
            timeline_h,
            true,
        );
        // 右: 台本エディタ / メッセージ
        let log_h = s(140).min(body_h / 4);
        let _ = MoveWindow(
            c.editor,
            center_x + center_w,
            bar_h,
            editor_w,
            body_h - log_h,
            true,
        );
        let _ = MoveWindow(
            c.log,
            center_x + center_w,
            bar_h + body_h - log_h,
            editor_w,
            log_h,
            true,
        );
    }
    with_app(|a| {
        a.requested = None;
        a.thumbs_generation = usize::MAX;
        a.thumb_keys.clear();
    });
}

// ---- 操作 -------------------------------------------------------------------

fn open_script(path: &std::path::Path) {
    with_app(|a| match a.studio.open(path) {
        Ok(()) => {
            if let Ok(e) = Editor::new(&a.studio.text, a.dpi) {
                a.editor = e;
                a.editor.set_language(Some("markdown"));
                a.editor_version = a.editor.document.version();
            }
            a.selected = 0;
            a.slides_view.scroll = 0;
            a.after_reload();
        }
        Err(e) => a.studio.log(e),
    });
    if let Some(hwnd) = with_app(|a| a.hwnd) {
        // キャラクターの一覧が変わるのでメニューを作り直す
        unsafe {
            if let Ok(menu) = build_menu() {
                let old = GetMenu(hwnd);
                let _ = SetMenu(hwnd, Some(menu));
                if !old.is_invalid() {
                    let _ = DestroyMenu(old);
                }
            }
        }
    }
}

fn confirm_discard(hwnd: HWND) -> bool {
    let modified = with_app(|a| a.editor.text() != a.saved_text()).unwrap_or(false);
    if !modified {
        return true;
    }
    match unsafe {
        MessageBoxW(
            Some(hwnd),
            w!("台本の変更を保存しますか？"),
            &HSTRING::from(APP_NAME),
            MB_YESNOCANCEL | MB_ICONQUESTION,
        )
    } {
        IDYES => save(hwnd, false),
        IDNO => true,
        _ => false,
    }
}

fn save(hwnd: HWND, ask: bool) -> bool {
    let current = with_app(|a| a.studio.path.clone()).flatten();
    let path = match (current, ask) {
        (Some(p), false) => Some(p),
        _ => file_dialog(
            hwnd,
            true,
            &[("台本 (Markdown)", "md"), ("テキスト", "txt")],
            Some("script.md"),
        ),
    };
    let Some(path) = path else { return false };
    with_app(|a| {
        let text = a.editor.text();
        match a.studio.save_as(&path, &text) {
            Ok(()) => {
                a.studio.saved = text;
                a.studio.log(format!("保存しました: {}", path.display()));
                a.after_reload();
                true
            }
            Err(e) => {
                a.studio.log(e);
                false
            }
        }
    })
    .unwrap_or(false)
}

fn command(hwnd: HWND, id: u16) {
    match id {
        ID_PLAY_SCENE => with_app(|a| a.play_scene()).unwrap_or(()),
        ID_PLAY_ALL => with_app(|a| a.play_all()).unwrap_or(()),
        ID_PREV => with_app(|a| a.step_scene(-1)).unwrap_or(()),
        ID_NEXT => with_app(|a| a.step_scene(1)).unwrap_or(()),
        ID_SYNTH => with_app(|a| a.studio.synthesize(None, false)).unwrap_or(()),
        ID_SYNTH_LINE => with_app(|a| {
            if let Some((s, Some(l))) = a.studio.item_at_line(a.caret_line) {
                a.studio.synthesize(Some((s, l)), true);
            } else {
                a.studio.log("カーソルをセリフの行に置いてください");
            }
        })
        .unwrap_or(()),
        ID_CANCEL => with_app(|a| a.studio.cancel_job()).unwrap_or(()),
        ID_EXPORT | M_EXPORT => {
            let name = with_app(|a| {
                a.studio
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| format!("{}.mp4", s.to_string_lossy()))
            })
            .flatten();
            let types = [
                ("MP4 動画", "mp4"),
                ("MP3 音声", "mp3"),
                ("WAV 音声", "wav"),
                ("PNG 静止画（セリフごと）", "png"),
            ];
            if let Some(path) = file_dialog(hwnd, true, &types, name.as_deref()) {
                with_app(|a| a.studio.export(path));
            }
        }
        M_NEW => {
            if confirm_discard(hwnd) {
                with_app(|a| {
                    a.studio.path = None;
                    a.studio.saved = String::new();
                    if let Ok(e) = Editor::new(crate::studio::TEMPLATE, a.dpi) {
                        a.editor = e;
                        a.editor.set_language(Some("markdown"));
                    }
                    a.studio.set_text(crate::studio::TEMPLATE.into());
                    a.selected = 0;
                    a.after_reload();
                });
            }
        }
        M_OPEN => {
            if confirm_discard(hwnd)
                && let Some(p) = file_dialog(
                    hwnd,
                    false,
                    &[("台本 (Markdown)", "md"), ("テキスト", "txt")],
                    None,
                )
            {
                open_script(&p);
            }
        }
        M_SAVE => {
            save(hwnd, false);
        }
        M_SAVE_AS => {
            save(hwnd, true);
        }
        M_EXIT => unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        },
        M_RELOAD => with_app(|a| {
            a.studio.audio_cache.lock().unwrap().clear();
            a.studio.forget_assets();
            a.studio.project = None;
            a.studio.reload();
            a.after_reload();
        })
        .unwrap_or(()),
        M_CHARS => {
            if let Some(p) = with_app(|a| a.studio.master_path()) {
                open_in_shell(&p);
            }
        }
        M_HELP => show_help(Some(help_section())),
        M_SYNTAX => show_help(Some("syntax")),
        M_SHORTCUTS => show_help(Some("shortcuts")),
        M_ABOUT => message_box(&format!(
            "{APP_NAME} {}\n\nMarkdown の台本から解説動画を作るアプリケーションです。",
            env!("CARGO_PKG_VERSION")
        )),
        id if (M_AUDITION..M_AUDITION + 60).contains(&id) => with_app(|a| {
            let ids: Vec<String> = a
                .studio
                .project
                .as_ref()
                .map(|p| p.master.keys().cloned().collect())
                .unwrap_or_default();
            if let Some(cid) = ids.get(usize::from(id - M_AUDITION)) {
                a.studio.audition(cid);
            }
        })
        .unwrap_or(()),
        id if (M_SIZE..M_SIZE + SIZES.len() as u16).contains(&id) => with_app(|a| {
            a.studio
                .set_export(Some(SIZES[usize::from(id - M_SIZE)]), None, None);
            a.studio.log(format!(
                "書き出しの解像度: {}×{}",
                a.studio.export_opts.width, a.studio.export_opts.height
            ));
        })
        .unwrap_or(()),
        id if (M_FPS..M_FPS + FPS.len() as u16).contains(&id) => with_app(|a| {
            a.studio
                .set_export(None, Some(FPS[usize::from(id - M_FPS)]), None);
            a.studio
                .log(format!("フレームレート: {} fps", a.studio.export_opts.fps));
        })
        .unwrap_or(()),
        id if (M_ENCODER..M_ENCODER + ENCODERS.len() as u16).contains(&id) => with_app(|a| {
            let (b, label) = ENCODERS[usize::from(id - M_ENCODER)];
            a.studio.set_export(None, None, Some(b));
            a.studio.log(format!("動画エンコーダ: {label}"));
        })
        .unwrap_or(()),
        id if (M_DEVICE..M_DEVICE + DEVICES.len() as u16).contains(&id) => with_app(|a| {
            let d = DEVICES[usize::from(id - M_DEVICE)];
            a.studio.set_device(d);
            a.studio.log(format!("音声合成のデバイス: {d}"));
        })
        .unwrap_or(()),
        _ => {}
    }
    with_app(|a| a.refresh_lists());
}

impl App {
    fn saved_text(&self) -> String {
        if self.studio.path.is_some() {
            self.studio.saved.clone()
        } else {
            crate::studio::TEMPLATE.to_owned()
        }
    }

    fn after_reload(&mut self) {
        self.requested = None;
        self.refresh_lists();
        self.refresh_title();
        invalidate(self.c.editor);
        invalidate(self.c.timeline);
        invalidate(self.c.slides);
    }

    fn refresh_title(&self) {
        let name = self
            .studio
            .path
            .as_ref()
            .map_or("（未保存）".into(), |p| p.display().to_string());
        set_text(self.hwnd, &format!("{name} - {APP_NAME}"));
        set_text(
            self.c.status,
            &format!("素材の基準: {}", self.studio.root.display()),
        );
    }

    /// スライド・ノート・メッセージ・タイムラインの帯を作り直す。
    fn refresh_lists(&mut self) {
        let slides = self.studio.slides();
        if slides != self.slides {
            self.slides = slides;
            self.selected = self.selected.min(self.slides.len().saturating_sub(1));
            invalidate(self.c.slides);
        }
        let notes = self.studio.scene_rows(self.selected);
        if notes != self.notes {
            send(self.c.notes, WM_SETREDRAW, 0, 0);
            send(self.c.notes, LB_RESETCONTENT, 0, 0);
            for r in &notes {
                send(
                    self.c.notes,
                    LB_ADDSTRING,
                    0,
                    HSTRING::from(r.label.trim_start()).as_ptr() as isize,
                );
            }
            send(self.c.notes, WM_SETREDRAW, 1, 0);
            invalidate(self.c.notes);
            self.notes = notes;
        }
        if self.bars_generation != self.studio.generation {
            self.bars_generation = self.studio.generation;
            self.bars = self.studio.bars();
        }
        // メッセージ（台本の警告 → 読めない素材 → 処理の記録）
        let mut lines: Vec<String> = Vec::new();
        if let Some(e) = &self.studio.load_error {
            lines.push(format!("エラー: {e}"));
        }
        if let Some(p) = &self.studio.project {
            for d in &p.parsed.diagnostics {
                lines.push(format!("{} 行目: {}", d.line + 1, d.message));
            }
        }
        for m in &self.missing {
            lines.push(format!("素材: {m}"));
        }
        lines.extend(self.studio.log.iter().cloned());
        if lines.len() != self.log_len || self.studio.load_error.is_some() {
            send(self.c.log, LB_RESETCONTENT, 0, 0);
            for l in &lines {
                send(
                    self.c.log,
                    LB_ADDSTRING,
                    0,
                    HSTRING::from(l.as_str()).as_ptr() as isize,
                );
            }
            send(self.c.log, LB_SETTOPINDEX, lines.len().saturating_sub(1), 0);
            self.log_len = lines.len();
        }
        invalidate(self.c.timeline);
    }

    fn seek(&mut self, t: f64) {
        self.studio.time = t.clamp(0.0, self.studio.timeline.duration.max(0.0));
        if self.playing {
            self.clock = Some((Instant::now(), self.studio.time));
            self.player.seek(self.studio.time);
        }
        invalidate(self.c.timeline);
    }

    /// スライド（シーン）を選ぶ。`seek` ならそのシーンの頭へ移り、台本もその行へ。
    fn select_scene(&mut self, scene: usize, seek: bool) {
        if scene >= self.slides.len() {
            return;
        }
        let changed = scene != self.selected;
        self.selected = scene;
        if seek {
            if let Some((start, _)) = self.studio.scene_span(scene) {
                self.seek(start);
                let m = timeline::Metrics::new(self.dpi);
                self.view.follow(&m, client(self.c.timeline).right, start);
            }
            self.jump_to_line(self.slides[scene].source_line);
        }
        let r = client(self.c.slides);
        self.slides_view
            .reveal(&slides::Metrics::new(self.dpi), r.right, r.bottom, scene);
        if changed || seek {
            self.refresh_lists();
            invalidate(self.c.slides);
        }
    }

    fn step_scene(&mut self, dir: i32) {
        if self.playing {
            self.stop();
        }
        if let Some(s) = self.studio.neighbor_scene(self.selected, dir) {
            self.select_scene(s, true);
        }
    }

    fn stop(&mut self) {
        self.playing = false;
        self.play_end = None;
        self.clock = None;
        self.player.stop();
        set_text(self.c.play_scene, "▶ このシーン (F5)");
        set_text(self.c.play_all, "▶ 通して再生 (Shift+F5)");
        invalidate(self.c.slides);
    }

    fn start(&mut self, end: Option<f64>) {
        self.playing = true;
        self.play_end = end;
        self.clock = Some((Instant::now(), self.studio.time));
        self.studio.prepare_playback();
        set_text(self.c.play_scene, "■ 停止 (F5)");
        set_text(self.c.play_all, "■ 停止 (Shift+F5)");
        invalidate(self.c.slides);
    }

    /// 選んだシーンだけを頭から再生する（再生中なら止める）。
    fn play_scene(&mut self) {
        if self.playing {
            self.stop();
            return;
        }
        let Some((start, end)) = self.studio.scene_span(self.selected) else {
            self.studio
                .log("このシーンは動画に含まれません（セリフか間が必要です）");
            return;
        };
        self.studio.time = start;
        self.start(Some(end));
    }

    /// 今の位置から最後まで通して再生する（再生中なら止める）。
    fn play_all(&mut self) {
        if self.playing {
            self.stop();
            return;
        }
        if self.studio.time >= self.studio.timeline.duration - 0.05 {
            self.studio.time = 0.0;
        }
        self.start(None);
    }

    /// エディタのキャレットを `line` 行目の先頭へ。
    fn jump_to_line(&mut self, line: usize) {
        let text = self.editor.text();
        let offset: usize = text.split('\n').take(line).map(|l| l.len() + 1).sum();
        let offset = offset.min(text.len()) as u64;
        self.editor
            .document
            .set_selections(SelectionSet::single(Selection::caret(offset)));
        self.editor.follow_caret = true;
        self.caret_line = line;
        invalidate(self.c.editor);
    }

    /// 16ms ごと: 台本の再解析・キャレット追従・再生の時計・プレビューとサムネイルの要求・表示の更新。
    fn tick(&mut self) {
        // 編集が止まって 300ms で台本を読み直す
        let version = self.editor.document.version();
        if version != self.editor_version {
            self.editor_version = version;
            self.edited_at = Some(Instant::now());
        }
        if self
            .edited_at
            .is_some_and(|t| t.elapsed() > Duration::from_millis(300))
        {
            self.edited_at = None;
            if self.studio.set_text(self.editor.text()) {
                self.requested = None;
                self.refresh_lists();
                if self.playing {
                    self.studio.prepare_playback();
                }
            }
        }
        // キャレットの行へプレビューを合わせる（再生中は除く）。行番号は文書の木から引く
        let head = self.editor.document.selections().primary().head;
        let line = self.editor.document.snapshot().line_of_offset(head).line as usize;
        if line != self.caret_line {
            self.caret_line = line;
            if !self.playing
                && let Some(t) = self.studio.time_for_line(line)
            {
                self.studio.time = t;
                invalidate(self.c.timeline);
            }
        }
        // 再生の時計（音声デバイスがあればその位置）。シーン再生はシーンの終わりで止める
        if self.playing {
            if self.player.playing() {
                self.studio.time = self.player.position();
            } else if let Some((start, from)) = self.clock {
                self.studio.time = from + start.elapsed().as_secs_f64();
            }
            let end = self.play_end.unwrap_or(self.studio.timeline.duration);
            if self.studio.time >= end {
                // シーンの最後の絵を残す
                self.studio.time = (end - 1e-3).max(0.0);
                self.stop();
            }
            self.view.follow(
                &timeline::Metrics::new(self.dpi),
                client(self.c.timeline).right,
                self.studio.time,
            );
            invalidate(self.c.timeline);
        }
        // 今の時刻のシーンをスライドで選ぶ
        if let Some((scene, _)) = self.studio.current()
            && scene != self.selected
        {
            self.select_scene(scene, false);
        }
        // プレビュー
        let key = (self.studio.time, self.studio.generation);
        if self.requested != Some(key)
            && let Some(project) = self.studio.project.clone()
        {
            self.requested = Some(key);
            let rc = client(self.c.preview);
            // 表示の大きさに合わせて描く（最大 1280×720）
            let width = ((rc.right.max(160) as u32).min(1280) & !1).max(160);
            let height = (width * 9 / 16) & !1;
            self.preview.request(preview::Request {
                project,
                timeline: self.studio.timeline.clone(),
                lipsync: self.studio.lipsync.clone(),
                time: self.studio.time,
                width,
                height,
            });
        }
        // サムネイル（編集が落ち着いてから。内容の変わったシーンだけ描き直す）
        if self.edited_at.is_none() && self.thumbs_generation != self.studio.generation {
            self.thumbs_generation = self.studio.generation;
            let keys = self.studio.scene_keys();
            if keys != self.thumb_keys
                && let Some(project) = self.studio.project.clone()
            {
                self.thumb_keys = keys.clone();
                let (tw, _) = slides::Metrics::new(self.dpi).thumb(client(self.c.slides).right);
                let width = ((tw.max(64) as u32).min(480) & !1).max(64);
                self.thumbs.request(preview::ThumbRequest {
                    project,
                    timeline: self.studio.timeline.clone(),
                    lipsync: self.studio.lipsync.clone(),
                    generation: self.studio.generation,
                    keys,
                    width,
                    height: (width * 9 / 16) & !1,
                });
            }
        }
        let span = self.studio.scene_span(self.selected);
        let scene_time = span.map_or(String::new(), |(a, b)| {
            format!(
                "   シーン {}: {} / {}",
                self.selected + 1,
                fmt_time(self.studio.time - a),
                fmt_time(b - a)
            )
        });
        set_text(
            self.c.time,
            &format!(
                "{} / {}{scene_time}",
                fmt_time(self.studio.time),
                fmt_time(self.studio.timeline.duration)
            ),
        );
        let status = match &self.studio.job {
            Some(j) => format!("{}: {:.0}%  {}", j.label, j.progress * 100.0, j.status),
            None if self.playing && !self.player.has_device() => {
                format!("{}（音声出力なし）", self.studio.describe_current())
            }
            None => self.studio.describe_current(),
        };
        set_text(self.c.status_text, &status);
    }

    fn on_notify(&mut self) {
        let mut changed = false;
        let mut play: Option<Option<(usize, usize)>> = None;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Mixer(source, from) => {
                    if self.playing {
                        let t = if (self.studio.time - from).abs() < 0.5 {
                            self.studio.time
                        } else {
                            from
                        };
                        self.player.play(source, t);
                        self.clock = Some((Instant::now(), t));
                    }
                }
                Event::Audition(audio) => {
                    if self.playing {
                        self.stop();
                    }
                    self.player.play(audio, 0.0);
                }
                ev => {
                    let (c, p) = self.studio.apply(ev);
                    changed |= c;
                    if p.is_some() {
                        play = p;
                    }
                }
            }
        }
        if changed {
            self.studio.rebuild_timeline();
            self.requested = None;
            if self.playing {
                self.studio.prepare_playback();
            }
        }
        // 1 つのセリフを合成し終えた: そのセリフからシーンの終わりまで再生する
        if let Some(target) = play {
            if self.playing {
                self.stop();
            }
            let mut end = None;
            if let Some((s, l)) = target
                && let Some(t) = self.studio.timeline.line_start(s, l)
            {
                self.studio.time = t;
                end = self.studio.scene_span(s).map(|(_, e)| e);
            }
            self.start(end);
        }
        self.refresh_lists();
        invalidate(self.c.slides);
    }

    /// プレビューのフレームができた: プレビューだけ描き直す（読めない素材の一覧が変わったときだけメッセージも）。
    fn on_frame(&mut self) {
        self.frame_pending
            .store(false, std::sync::atomic::Ordering::Release);
        let missing = self
            .preview
            .latest
            .lock()
            .unwrap()
            .as_ref()
            .map(|f| f.missing.clone());
        if let Some(m) = missing
            && m != self.missing
        {
            self.missing = m;
            self.refresh_lists();
        }
        invalidate(self.c.preview);
    }
}

// ---- ウィンドウプロシージャ ---------------------------------------------------

unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_TIMER if w.0 == TIMER_TICK => {
            with_app(|a| a.tick());
            LRESULT(0)
        }
        WM_NOTIFY_APP => {
            with_app(|a| a.on_notify());
            LRESULT(0)
        }
        WM_FRAME_READY => {
            with_app(|a| a.on_frame());
            LRESULT(0)
        }
        WM_THUMBS_READY => {
            with_app(|a| {
                a.thumbs_pending
                    .store(false, std::sync::atomic::Ordering::Release);
                invalidate(a.c.slides);
            });
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (w.0 & 0xffff) as u16;
            let code = ((w.0 >> 16) & 0xffff) as u32;
            match id {
                ID_NOTES if code == LBN_SELCHANGE => {
                    with_app(|a| {
                        let i = send(a.c.notes, LB_GETCURSEL, 0, 0);
                        if let Some(row) = usize::try_from(i)
                            .ok()
                            .and_then(|i| a.notes.get(i))
                            .cloned()
                        {
                            if a.playing {
                                a.stop();
                            }
                            if let Some(t) = row
                                .line
                                .and_then(|l| a.studio.timeline.line_start(row.scene, l))
                            {
                                a.seek(t);
                            }
                            a.jump_to_line(row.source_line);
                        }
                    });
                }
                ID_NOTES if code == LBN_DBLCLK => {
                    with_app(|a| {
                        let i = send(a.c.notes, LB_GETCURSEL, 0, 0);
                        if let Some(row) = usize::try_from(i)
                            .ok()
                            .and_then(|i| a.notes.get(i))
                            .cloned()
                            && let Some(l) = row.line
                        {
                            a.studio.synthesize(Some((row.scene, l)), true);
                        }
                    });
                }
                ID_LOG if code == LBN_DBLCLK => {
                    // 「n 行目: …」の行へ移動する
                    with_app(|a| {
                        let i = send(a.c.log, LB_GETCURSEL, 0, 0);
                        let len = send(a.c.log, LB_GETTEXTLEN, i as usize, 0);
                        if i >= 0 && len > 0 {
                            let mut buf = vec![0u16; len as usize + 1];
                            send(a.c.log, LB_GETTEXT, i as usize, buf.as_mut_ptr() as isize);
                            let s = String::from_utf16_lossy(&buf[..len as usize]);
                            if let Some(n) = s
                                .split(" 行目")
                                .next()
                                .and_then(|n| n.parse::<usize>().ok())
                            {
                                a.jump_to_line(n.saturating_sub(1));
                                unsafe {
                                    let _ = SetFocus(Some(a.c.editor));
                                }
                            }
                        }
                    });
                }
                ID_ENGINE if code == CBN_SELCHANGE => {
                    with_app(|a| {
                        let i = send(a.c.engine, CB_GETCURSEL, 0, 0);
                        if let Some(e) = usize::try_from(i)
                            .ok()
                            .and_then(|i| a.engines.get(i))
                            .copied()
                        {
                            a.studio.set_engine(e);
                            a.requested = None;
                            a.refresh_lists();
                        }
                    });
                }
                id => command(hwnd, id),
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if confirm_discard(hwnd) {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_TICK);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

/// スライド一覧: クリックで選択、ダブルクリック・Enter でそのシーンを再生、↑↓で前後のシーン。
unsafe extern "system" fn slides_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let y = (l.0 >> 16) as i16 as i32;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            with_app(|a| {
                let m = slides::Metrics::new(a.dpi);
                let playing = a.playing.then_some(a.selected);
                let thumbs = a.thumbs.images.lock().unwrap();
                a.slides_view
                    .paint(hwnd, a.font, &m, &a.slides, &thumbs, a.selected, playing);
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            unsafe {
                let _ = SetFocus(Some(hwnd));
            }
            with_app(|a| {
                let m = slides::Metrics::new(a.dpi);
                if let Some(i) = a.slides_view.hit(&m, client(hwnd).right, y, a.slides.len()) {
                    if a.playing {
                        a.stop();
                    }
                    a.select_scene(i, true);
                    if msg == WM_LBUTTONDBLCLK {
                        a.play_scene();
                    }
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = w.0 as u32;
            with_app(|a| {
                if key == u32::from(VK_UP.0) {
                    a.step_scene(-1);
                } else if key == u32::from(VK_DOWN.0) {
                    a.step_scene(1);
                } else if key == u32::from(VK_RETURN.0) || key == u32::from(VK_SPACE.0) {
                    a.play_scene();
                }
            });
            LRESULT(0)
        }
        WM_GETDLGCODE => LRESULT(DLGC_WANTARROWS as isize),
        WM_MOUSEWHEEL => {
            let delta = (w.0 >> 16) as i16 as i32;
            with_app(|a| {
                let m = slides::Metrics::new(a.dpi);
                let r = client(hwnd);
                a.slides_view.scroll -= delta * m.item_h(r.right) / 240;
                a.slides_view.clamp(&m, r.right, r.bottom, a.slides.len());
            });
            invalidate(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

unsafe extern "system" fn preview_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let rc = client(hwnd);
                let bg = CreateSolidBrush(COLORREF(0x00303030));
                FillRect(dc, &rc, bg);
                let _ = DeleteObject(bg.into());
                with_app(|a| {
                    if let Some(f) = a.preview.latest.lock().unwrap().as_ref() {
                        // 16:9 を保って余白付きで表示する（スライドのように）
                        let pad = 12 * a.dpi as i32 / 96;
                        let (aw, ah) = ((rc.right - 2 * pad).max(1), (rc.bottom - 2 * pad).max(1));
                        let scale = (aw as f32 / f.width as f32).min(ah as f32 / f.height as f32);
                        let (dw, dh) = (
                            (f.width as f32 * scale) as i32,
                            (f.height as f32 * scale) as i32,
                        );
                        let (dx, dy) = ((rc.right - dw) / 2, (rc.bottom - dh) / 2);
                        let info = BITMAPINFO {
                            bmiHeader: BITMAPINFOHEADER {
                                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                                biWidth: f.width as i32,
                                biHeight: -(f.height as i32),
                                biPlanes: 1,
                                biBitCount: 32,
                                biCompression: BI_RGB.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        SetStretchBltMode(dc, HALFTONE);
                        StretchDIBits(
                            dc,
                            dx,
                            dy,
                            dw,
                            dh,
                            0,
                            0,
                            f.width as i32,
                            f.height as i32,
                            Some(f.bgra.as_ptr().cast()),
                            &info,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                    }
                });
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        // スライドをクリックするとそのシーンを再生・停止
        WM_LBUTTONDOWN => {
            with_app(|a| a.play_scene());
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

unsafe extern "system" fn timeline_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let x = l.0 as i16 as i32;
    let y = (l.0 >> 16) as i16 as i32;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            with_app(|a| {
                let m = timeline::Metrics::new(a.dpi);
                a.view.paint(
                    hwnd,
                    a.font,
                    &m,
                    &a.bars,
                    a.studio.timeline.duration,
                    a.studio.time,
                );
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            unsafe {
                SetCapture(hwnd);
            }
            with_app(|a| {
                a.view.dragging = true;
                let t = a.view.time_at(&timeline::Metrics::new(a.dpi), x);
                a.seek(t);
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            with_app(|a| {
                if a.view.dragging {
                    let t = a.view.time_at(&timeline::Metrics::new(a.dpi), x);
                    a.seek(t);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            unsafe {
                let _ = ReleaseCapture();
            }
            with_app(|a| a.view.dragging = false);
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            with_app(|a| {
                let m = timeline::Metrics::new(a.dpi);
                if let Some(i) = a.view.hit(&m, &a.bars, x, y) {
                    let line = a.bars[i].source_line;
                    a.jump_to_line(line);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (w.0 >> 16) as i16 as f64 / 120.0;
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            let mut pt = POINT { x, y };
            unsafe {
                let _ = ScreenToClient(hwnd, &mut pt);
            }
            with_app(|a| {
                let m = timeline::Metrics::new(a.dpi);
                if shift {
                    a.view.scroll = (a.view.scroll - delta * 100.0 / a.view.pps).max(0.0);
                } else {
                    let anchor = a.view.time_at(&m, pt.x);
                    a.view.zoom(1.25f64.powf(delta), anchor, &m, pt.x);
                }
            });
            invalidate(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

/// yydbmanager の SQL エディタと同じく yy-ui-win の `Editor` をそのまま載せる。
unsafe extern "system" fn editor_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let handled = with_app(|a| {
        let editor = &mut a.editor;
        match msg {
            WM_PAINT => {
                unsafe {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut ps);
                    let r = client(hwnd);
                    let _ = editor.paint(hwnd, r.right.max(1) as u32, r.bottom.max(1) as u32);
                    let _ = EndPaint(hwnd, &ps);
                }
                true
            }
            WM_ERASEBKGND => true,
            WM_GETDLGCODE => {
                return Some(LRESULT(
                    (DLGC_WANTALLKEYS | DLGC_WANTCHARS | DLGC_WANTARROWS) as isize,
                ));
            }
            WM_CHAR => {
                editor.character(w.0 as u16);
                invalidate(hwnd);
                true
            }
            WM_KEYDOWN => {
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                editor.key(hwnd, w.0 as u32, ctrl, shift);
                invalidate(hwnd);
                true
            }
            WM_LBUTTONDOWN => {
                unsafe {
                    let _ = SetFocus(Some(hwnd));
                }
                editor.click(
                    l.0 as i16 as i32,
                    (l.0 >> 16) as i16 as i32,
                    unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0,
                );
                invalidate(hwnd);
                true
            }
            WM_MOUSEWHEEL => {
                let delta = (w.0 >> 16) as i16 as i64 / 120;
                editor.scroll_rows(-delta * 3);
                invalidate(hwnd);
                true
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                invalidate(hwnd);
                false
            }
            WM_IME_COMPOSITION => {
                let update = yy_ui_win::ime::read_composition(hwnd, l.0 as u32);
                if let Some(result) = update.result {
                    editor.document.insert_text(&result, false);
                }
                editor.composition =
                    update
                        .composing
                        .filter(|(s, _)| !s.is_empty())
                        .map(|(text, cursor)| yy_ui_win::render::Composition {
                            offsets: vec![editor.document.selections().primary().head],
                            text,
                            cursor,
                        });
                invalidate(hwnd);
                true
            }
            WM_IME_ENDCOMPOSITION => {
                editor.composition = None;
                invalidate(hwnd);
                true
            }
            _ => false,
        }
        .then_some(LRESULT(0))
    })
    .flatten();
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, w, l) })
}

/// フォーカスのある場所に応じたヘルプの項目。
fn help_section() -> &'static str {
    let focus = unsafe { GetFocus() };
    let within =
        |h: HWND| !h.is_invalid() && (focus == h || unsafe { IsChild(h, focus) }.as_bool());
    with_app(|a| {
        if within(a.c.editor) {
            "syntax"
        } else if within(a.c.slides) {
            "slides"
        } else if within(a.c.timeline) {
            "timeline"
        } else if within(a.c.notes) {
            "notes"
        } else if within(a.c.log) {
            "messages"
        } else if within(a.c.preview) {
            "preview"
        } else {
            "start"
        }
    })
    .unwrap_or("start")
}

fn show_help(section: Option<&str>) {
    if let Err(e) = yy_ui_win::help::show(
        HELP_MD,
        &format!("{APP_NAME} ヘルプ"),
        section,
        "markdown-movie-maker-help",
    ) {
        message_box(&format!("ヘルプを表示できません: {e}"));
    }
}
