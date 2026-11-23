//! MarkdownMovieMaker — 台本の編集・プレビュー・タイムライン・音声合成・書き出しを行う Windows アプリ。
//!
//! 画面は yyeditor の Win32 部品（Direct2D/DirectWrite のエディタ・IME・同梱フォント）と
//! Win32 の標準コントロールで作り、GUI フレームワークは使わない。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#[allow(dead_code)]
mod studio;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    let script = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    if let Err(e) = win::run(script) {
        win::message_box(&format!("起動できません: {e}"));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "MarkdownMovieMaker は Windows 専用です。ほかの OS では CLI（mmm）を使ってください。"
    );
    std::process::exit(1);
}
