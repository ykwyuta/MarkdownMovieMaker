//! YMM4 風のタイムライン表示（GDI、ダブルバッファ）。
//!
//! 上から「シーン・セリフ・立ち絵・アイテム・音声」のトラックに帯を並べ、再生位置を赤い線で示す。
//! クリック・ドラッグで移動、ホイールで拡大縮小（Shift+ホイールで左右に送る）、帯のダブルクリックで
//! 台本のその行へ移動する。
use crate::studio::{Bar, TRACKS, fmt_time};
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

pub struct View {
    /// 1 秒あたりのピクセル。
    pub pps: f64,
    /// 左端の時刻。
    pub scroll: f64,
    pub dragging: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            pps: 60.0,
            scroll: 0.0,
            dragging: false,
        }
    }
}

pub struct Metrics {
    pub label_w: i32,
    pub ruler_h: i32,
    pub lane_h: i32,
}

impl Metrics {
    pub fn new(dpi: u32) -> Self {
        let s = |v: i32| v * dpi as i32 / 96;
        Self {
            label_w: s(64),
            ruler_h: s(20),
            lane_h: s(22),
        }
    }
}

/// 重なる帯を段に振り分ける（トラックごと、最大 3 段）。戻り値は各帯の (トラック内の段)。
fn lanes(bars: &[Bar]) -> (Vec<usize>, [usize; TRACKS.len()]) {
    let mut ends: Vec<Vec<f64>> = vec![Vec::new(); TRACKS.len()];
    let mut lane = Vec::with_capacity(bars.len());
    for b in bars {
        let track = &mut ends[b.track];
        let i = match track.iter().position(|&e| e <= b.start + 1e-6) {
            Some(i) => i,
            None if track.len() < 3 => {
                track.push(0.0);
                track.len() - 1
            }
            None => track.len() - 1,
        };
        track[i] = b.end;
        lane.push(i);
    }
    let counts = std::array::from_fn(|t| ends[t].len().max(1));
    (lane, counts)
}

fn color(key: &str) -> COLORREF {
    let h = key.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let hue = (h % 360) as f32;
    // 淡い色（HSV: 彩度 0.35、明度 0.95）
    let (s, v) = (0.35, 0.95);
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0) as u32;
    COLORREF(to(r) | to(g) << 8 | to(b) << 16)
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

unsafe fn text(dc: HDC, mut r: RECT, s: &str, c: COLORREF) {
    unsafe {
        SetTextColor(dc, c);
        let mut w: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut w,
            &mut r,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

impl View {
    pub fn x_of(&self, m: &Metrics, t: f64) -> i32 {
        m.label_w + ((t - self.scroll) * self.pps).round() as i32
    }

    pub fn time_at(&self, m: &Metrics, x: i32) -> f64 {
        (self.scroll + f64::from(x - m.label_w) / self.pps).max(0.0)
    }

    /// 再生位置が見えるように左右に送る。
    pub fn follow(&mut self, m: &Metrics, width: i32, time: f64) {
        let visible = f64::from((width - m.label_w).max(1)) / self.pps;
        if time < self.scroll || time > self.scroll + visible * 0.9 {
            self.scroll = (time - visible * 0.1).max(0.0);
        }
    }

    pub fn zoom(&mut self, factor: f64, anchor_time: f64, m: &Metrics, anchor_x: i32) {
        self.pps = (self.pps * factor).clamp(2.0, 800.0);
        self.scroll = (anchor_time - f64::from(anchor_x - m.label_w) / self.pps).max(0.0);
    }

    /// 帯の当たり判定（ダブルクリックで台本へ移動）。
    pub fn hit(&self, m: &Metrics, bars: &[Bar], x: i32, y: i32) -> Option<usize> {
        let (lane, counts) = lanes(bars);
        let mut top = m.ruler_h;
        let mut track_top = [0; TRACKS.len()];
        for (t, n) in counts.iter().enumerate() {
            track_top[t] = top;
            top += m.lane_h * *n as i32;
        }
        bars.iter().enumerate().rev().find_map(|(i, b)| {
            let y0 = track_top[b.track] + m.lane_h * lane[i] as i32;
            let (x0, x1) = (
                self.x_of(m, b.start),
                self.x_of(m, b.end).max(self.x_of(m, b.start) + 3),
            );
            (x >= x0 && x < x1 && y >= y0 && y < y0 + m.lane_h).then_some(i)
        })
    }

    /// ウィンドウ全体に描く。
    pub fn paint(
        &self,
        hwnd: HWND,
        font: HFONT,
        m: &Metrics,
        bars: &[Bar],
        duration: f64,
        time: f64,
    ) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let target = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right.max(1), rc.bottom.max(1));
            let dc = CreateCompatibleDC(Some(target));
            let bmp = CreateCompatibleBitmap(target, w, h);
            let old_bmp = SelectObject(dc, bmp.into());
            let old_font = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            fill(dc, rc, rgb(38, 40, 44));

            // トラック
            let (lane, counts) = lanes(bars);
            let mut top = m.ruler_h;
            let mut track_top = [0; TRACKS.len()];
            for (t, name) in TRACKS.iter().enumerate() {
                track_top[t] = top;
                let height = m.lane_h * counts[t] as i32;
                fill(
                    dc,
                    RECT {
                        left: 0,
                        top,
                        right: w,
                        bottom: top + height,
                    },
                    if t % 2 == 0 {
                        rgb(46, 48, 53)
                    } else {
                        rgb(42, 44, 48)
                    },
                );
                text(
                    dc,
                    RECT {
                        left: 6,
                        top,
                        right: m.label_w - 4,
                        bottom: top + m.lane_h,
                    },
                    name,
                    rgb(200, 200, 200),
                );
                top += height;
            }
            // 動画の範囲外
            let end_x = self.x_of(m, duration);
            if end_x < w {
                fill(
                    dc,
                    RECT {
                        left: end_x.max(m.label_w),
                        top: m.ruler_h,
                        right: w,
                        bottom: h,
                    },
                    rgb(30, 30, 32),
                );
            }
            // 帯
            for (i, b) in bars.iter().enumerate() {
                let x0 = self.x_of(m, b.start).max(m.label_w);
                let x1 = self.x_of(m, b.end).max(x0 + 3);
                if x1 < m.label_w || x0 > w {
                    continue;
                }
                let y0 = track_top[b.track] + m.lane_h * lane[i] as i32 + 2;
                let r = RECT {
                    left: x0,
                    top: y0,
                    right: x1 - 1,
                    bottom: y0 + m.lane_h - 4,
                };
                fill(dc, r, color(&b.color_key));
                text(dc, RECT { left: x0 + 4, ..r }, &b.label, rgb(20, 20, 20));
            }
            // 目盛り
            fill(
                dc,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: m.ruler_h,
                },
                rgb(28, 29, 32),
            );
            let visible = f64::from((w - m.label_w).max(1)) / self.pps;
            let step = [0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
                .into_iter()
                .find(|s| s * self.pps >= 70.0)
                .unwrap_or(600.0);
            let pen = CreatePen(PS_SOLID, 1, rgb(90, 90, 96));
            let old_pen = SelectObject(dc, pen.into());
            let mut t = (self.scroll / step).floor() * step;
            while t <= self.scroll + visible {
                let x = self.x_of(m, t);
                if x >= m.label_w {
                    let _ = MoveToEx(dc, x, m.ruler_h - 6, None);
                    let _ = LineTo(dc, x, m.ruler_h);
                    text(
                        dc,
                        RECT {
                            left: x + 3,
                            top: 0,
                            right: x + 80,
                            bottom: m.ruler_h,
                        },
                        &fmt_time(t),
                        rgb(170, 170, 170),
                    );
                }
                t += step;
            }
            // 再生位置
            let px = self.x_of(m, time);
            if px >= m.label_w {
                let red = CreatePen(PS_SOLID, 2, rgb(255, 70, 70));
                SelectObject(dc, red.into());
                let _ = MoveToEx(dc, px, 0, None);
                let _ = LineTo(dc, px, h);
                SelectObject(dc, pen.into());
                let _ = DeleteObject(red.into());
            }
            text(
                dc,
                RECT {
                    left: 4,
                    top: 0,
                    right: m.label_w,
                    bottom: m.ruler_h,
                },
                &fmt_time(time),
                rgb(255, 120, 120),
            );
            SelectObject(dc, old_pen);
            let _ = DeleteObject(pen.into());

            let _ = BitBlt(target, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
            SelectObject(dc, old_font);
            SelectObject(dc, old_bmp);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            let _ = EndPaint(hwnd, &ps);
        }
    }
}
