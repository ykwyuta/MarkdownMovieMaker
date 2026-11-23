//! PowerPoint のスライド一覧のような、シーンのサムネイルの縦並び（GDI、ダブルバッファ）。
//!
//! クリックでそのシーンを選んで大きく表示し、ダブルクリックでそのシーンだけを再生する。
use super::preview::Frame;
use crate::studio::Slide;
use std::collections::HashMap;
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::GetClientRect};

pub struct View {
    pub scroll: i32,
}

pub struct Metrics {
    pub margin: i32,
    pub number_w: i32,
    pub text_h: i32,
}

impl Metrics {
    pub fn new(dpi: u32) -> Self {
        let s = |v: i32| v * dpi as i32 / 96;
        Self {
            margin: s(8),
            number_w: s(22),
            text_h: s(36),
        }
    }

    /// サムネイルの大きさ（16:9）。
    pub fn thumb(&self, width: i32) -> (i32, i32) {
        let w = (width - self.number_w - self.margin * 2).max(32);
        (w, w * 9 / 16)
    }

    /// 1 枚の高さ（サムネイル + 文字 + 余白）。
    pub fn item_h(&self, width: i32) -> i32 {
        self.thumb(width).1 + self.text_h + self.margin
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16)
}

unsafe fn fill(dc: HDC, r: RECT, c: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(c);
        FillRect(dc, &r, brush);
        let _ = DeleteObject(brush.into());
    }
}

unsafe fn text(dc: HDC, mut r: RECT, s: &str, c: COLORREF, flags: DRAW_TEXT_FORMAT) {
    unsafe {
        SetTextColor(dc, c);
        let mut w: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut w,
            &mut r,
            flags | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

impl View {
    /// クライアント座標 `y` にあるスライドの番号。
    pub fn hit(&self, m: &Metrics, width: i32, y: i32, count: usize) -> Option<usize> {
        let i = (y + self.scroll - m.margin).max(0) / m.item_h(width).max(1);
        (i >= 0 && (i as usize) < count).then_some(i as usize)
    }

    /// 選んだスライドが見えるように送る。
    pub fn reveal(&mut self, m: &Metrics, width: i32, height: i32, index: usize) {
        let item = m.item_h(width);
        let top = m.margin + item * index as i32;
        if top < self.scroll {
            self.scroll = top - m.margin;
        } else if top + item > self.scroll + height {
            self.scroll = top + item - height;
        }
        self.scroll = self.scroll.max(0);
    }

    pub fn clamp(&mut self, m: &Metrics, width: i32, height: i32, count: usize) {
        let total = m.margin + m.item_h(width) * count as i32;
        self.scroll = self.scroll.min((total - height).max(0)).max(0);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        hwnd: HWND,
        font: HFONT,
        m: &Metrics,
        slides: &[Slide],
        thumbs: &HashMap<usize, (u64, Frame)>,
        selected: usize,
        playing: Option<usize>,
    ) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let target = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right.max(1), rc.bottom.max(1));
            let dc = CreateCompatibleDC(Some(target));
            let bmp = CreateCompatibleBitmap(target, w, h);
            let old_bmp = SelectObject(dc, bmp.into());
            let old_font = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetStretchBltMode(dc, HALFTONE);
            fill(dc, rc, rgb(232, 232, 236));
            let (tw, th) = m.thumb(w);
            let item = m.item_h(w);
            for (i, slide) in slides.iter().enumerate() {
                let top = m.margin + item * i as i32 - self.scroll;
                if top + item < 0 || top > h {
                    continue;
                }
                let left = m.number_w + m.margin;
                // 選択枠
                if i == selected {
                    fill(
                        dc,
                        RECT {
                            left: left - 4,
                            top: top - 4,
                            right: left + tw + 4,
                            bottom: top + th + 4,
                        },
                        rgb(40, 110, 230),
                    );
                }
                text(
                    dc,
                    RECT {
                        left: 0,
                        top,
                        right: m.number_w + 4,
                        bottom: top + m.text_h / 2,
                    },
                    &(i + 1).to_string(),
                    rgb(80, 80, 90),
                    DT_CENTER,
                );
                if playing == Some(i) {
                    text(
                        dc,
                        RECT {
                            left: 0,
                            top: top + m.text_h / 2,
                            right: m.number_w + 4,
                            bottom: top + m.text_h,
                        },
                        "▶",
                        rgb(220, 40, 40),
                        DT_CENTER,
                    );
                }
                let frame = RECT {
                    left,
                    top,
                    right: left + tw,
                    bottom: top + th,
                };
                match thumbs.get(&slide.scene) {
                    Some((_, f)) if slide.span.is_some() => {
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
                        StretchDIBits(
                            dc,
                            left,
                            top,
                            tw,
                            th,
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
                    _ => {
                        fill(dc, frame, rgb(60, 62, 68));
                        let label = if slide.span.is_some() {
                            "描画中…"
                        } else {
                            "（表示なし）"
                        };
                        text(dc, frame, label, rgb(200, 200, 200), DT_CENTER | DT_VCENTER);
                    }
                }
                let ty = top + th + 2;
                text(
                    dc,
                    RECT {
                        left,
                        top: ty,
                        right: left + tw,
                        bottom: ty + m.text_h / 2,
                    },
                    &slide.title,
                    rgb(20, 20, 24),
                    DT_LEFT,
                );
                text(
                    dc,
                    RECT {
                        left,
                        top: ty + m.text_h / 2,
                        right: left + tw,
                        bottom: ty + m.text_h,
                    },
                    &slide.subtitle(),
                    rgb(100, 100, 110),
                    DT_LEFT,
                );
            }
            let _ = BitBlt(target, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
            SelectObject(dc, old_font);
            SelectObject(dc, old_bmp);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            let _ = EndPaint(hwnd, &ps);
        }
    }
}
