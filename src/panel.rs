//! The per-monitor setup panel shown while adjusting the trim.

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject, DrawTextW, DRAW_TEXT_FORMAT, FillRect,
    FrameRect, HDC, HFONT, OUT_DEFAULT_PRECIS, SRCCOPY, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows::core::w;

pub const WIDTH: i32 = 480;
pub const HEIGHT: i32 = 252;

const BG: u32 = rgb(32, 33, 36);
const TEXT: u32 = rgb(241, 243, 244);
const MUTED: u32 = rgb(154, 160, 166);
const BUTTON: u32 = rgb(60, 64, 67);
const ACCENT: u32 = rgb(26, 115, 232);
pub const OVERLAY: u32 = rgb(255, 45, 85);

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

#[derive(Clone, Copy, PartialEq)]
pub enum Button {
    Minus10,
    Minus1,
    Plus1,
    Plus10,
    Cancel,
    Save,
}

pub struct View<'a> {
    pub title: &'a str,
    pub height: i32,
    pub active: bool,
    pub dpi: u32,
}

fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

fn rect(l: i32, t: i32, r: i32, b: i32, dpi: u32) -> RECT {
    RECT { left: scale(l, dpi), top: scale(t, dpi), right: scale(r, dpi), bottom: scale(b, dpi) }
}

fn buttons(dpi: u32) -> [(Button, RECT, &'static str); 6] {
    let (top, bottom) = (164, 200);
    let step = |i: i32| rect(20 + i * 60, top, 72 + i * 60, bottom, dpi);
    [
        (Button::Minus10, step(0), "−10"),
        (Button::Minus1, step(1), "−1"),
        (Button::Plus1, step(2), "+1"),
        (Button::Plus10, step(3), "+10"),
        (Button::Cancel, rect(WIDTH - 204, top, WIDTH - 114, bottom, dpi), "Cancel"),
        (Button::Save, rect(WIDTH - 106, top, WIDTH - 20, bottom, dpi), "Save"),
    ]
}

pub fn hit_test(x: i32, y: i32, dpi: u32) -> Option<Button> {
    buttons(dpi)
        .into_iter()
        .find(|(_, r, _)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
        .map(|(b, _, _)| b)
}

pub fn paint(hdc: HDC, view: &View) {
    let dpi = view.dpi;
    let (w, h) = (scale(WIDTH, dpi), scale(HEIGHT, dpi));
    unsafe {
        // Off-screen buffer: holding an arrow key repaints many times a second.
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old_bmp = SelectObject(mem, bmp.into());
        SetBkMode(mem, TRANSPARENT);

        fill(mem, &RECT { left: 0, top: 0, right: w, bottom: h }, BG);
        if view.active {
            let brush = CreateSolidBrush(COLORREF(ACCENT));
            for i in 0..scale(2, dpi) {
                FrameRect(mem, &RECT { left: i, top: i, right: w - i, bottom: h - i }, brush);
            }
            let _ = DeleteObject(brush.into());
        }

        text(mem, view.title, rect(20, 14, WIDTH - 20, 38, dpi), 15, 600, MUTED, DT_SINGLELINE | DT_END_ELLIPSIS, dpi);
        let value = if view.height == 0 { "Off".to_string() } else { format!("{} px", view.height) };
        text(mem, &value, rect(20, 38, WIDTH - 20, 100, dpi), 50, 600, TEXT, DT_SINGLELINE | DT_VCENTER, dpi);
        text(
            mem,
            "Raise it until a red bar shows at the bottom of this screen, then lower it until the red is just gone.",
            rect(20, 108, WIDTH - 20, 154, dpi),
            14,
            400,
            TEXT,
            DT_WORDBREAK,
            dpi,
        );

        for (b, r, label) in buttons(dpi) {
            fill(mem, &r, if b == Button::Save { ACCENT } else { BUTTON });
            text(mem, label, r, 14, 600, TEXT, DT_SINGLELINE | DT_CENTER | DT_VCENTER, dpi);
        }

        text(
            mem,
            "↑ ↓ or scroll: 1 px  ·  Shift: 10 px  ·  Enter: save  ·  Esc: cancel",
            rect(20, 214, WIDTH - 20, 238, dpi),
            12,
            400,
            MUTED,
            DT_SINGLELINE | DT_VCENTER,
            dpi,
        );

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
    }
}

fn fill(hdc: HDC, r: &RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        FillRect(hdc, r, brush);
        let _ = DeleteObject(brush.into());
    }
}

#[allow(clippy::too_many_arguments)]
fn text(hdc: HDC, s: &str, mut r: RECT, size: i32, weight: i32, color: u32, fmt: DRAW_TEXT_FORMAT, dpi: u32) {
    unsafe {
        let font: HFONT = CreateFontW(
            -scale(size, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let old = SelectObject(hdc, font.into());
        SetTextColor(hdc, COLORREF(color));
        let mut buf: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(hdc, &mut buf, &mut r, fmt);
        SelectObject(hdc, old);
        let _ = DeleteObject(font.into());
    }
}

pub fn title(label: &str, width: i32, height: i32, primary: bool) -> String {
    let main = if primary { "  ·  main" } else { "" };
    format!("{label}  ·  {width}×{height}{main}")
}
