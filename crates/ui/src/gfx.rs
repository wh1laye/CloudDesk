use std::mem::zeroed;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, Ellipse, EndPaint, FillRect, GetStockObject, RoundRect, SelectObject, HBRUSH,
    HDC, NULL_BRUSH, NULL_PEN, PAINTSTRUCT, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::font::glyph;

pub type R = (i32, i32, i32, i32);
type Rgb = (u8, u8, u8);

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Rgb,
    pub fg: Rgb,
    pub dim: Rgb,
    pub acc: Rgb,
    pub dark: Rgb,
    pub glass: u8,
}

pub fn surface(c: &Ctx) -> HBRUSH {
    if crate::glass::level() > 0 {
        c.bg
    } else {
        c.dark
    }
}

pub fn palette(name: &str) -> Palette {
    match name {
        "grey" => Palette {
            bg: (34, 34, 36),
            fg: (236, 236, 236),
            dim: (140, 140, 144),
            acc: (226, 38, 45),
            dark: (60, 60, 64),
            glass: 0,
        },
        "burgundy" => Palette {
            bg: (24, 6, 10),
            fg: (240, 226, 228),
            dim: (150, 110, 116),
            acc: (214, 48, 66),
            dark: (62, 18, 26),
            glass: 0,
        },
        "light" => Palette {
            bg: (238, 238, 234),
            fg: (20, 20, 20),
            dim: (110, 110, 110),
            acc: (214, 40, 45),
            dark: (208, 208, 202),
            glass: 0,
        },
        "glass" => Palette {
            bg: (0, 0, 0),
            fg: (248, 248, 252),
            dim: (178, 182, 196),
            acc: (96, 176, 255),
            dark: (34, 38, 50),
            glass: 1,
        },
        "frost" => Palette {
            bg: (0, 0, 0),
            fg: (18, 20, 30),
            dim: (84, 88, 102),
            acc: (0, 112, 232),
            dark: (226, 230, 238),
            glass: 2,
        },
        "ocean" => Palette {
            bg: (6, 16, 30),
            fg: (232, 242, 252),
            dim: (118, 142, 168),
            acc: (64, 170, 255),
            dark: (18, 40, 66),
            glass: 0,
        },
        _ => Palette {
            bg: (10, 10, 10),
            fg: (240, 240, 240),
            dim: (130, 130, 130),
            acc: (226, 38, 45),
            dark: (44, 44, 44),
            glass: 0,
        },
    }
}

pub struct Ctx {
    pub dc: HDC,
    pub sc: i32,
    pub bg: HBRUSH,
    pub fg: HBRUSH,
    pub dim: HBRUSH,
    pub acc: HBRUSH,
    pub dark: HBRUSH,
}

unsafe fn brush(t: Rgb) -> HBRUSH {
    CreateSolidBrush(t.0 as u32 | (t.1 as u32) << 8 | (t.2 as u32) << 16)
}

impl Ctx {
    pub fn s(&self, v: i32) -> i32 {
        v * self.sc / 100
    }

    unsafe fn free(&self) {
        for b in [self.bg, self.fg, self.dim, self.acc, self.dark] {
            DeleteObject(b);
        }
    }
}

pub fn tw(s: &str, cell: i32) -> i32 {
    s.len() as i32 * 6 * cell
}

pub unsafe fn dot(c: &Ctx, b: HBRUSH, cx: i32, cy: i32, r: i32) {
    SelectObject(c.dc, b);
    Ellipse(c.dc, c.s(cx - r), c.s(cy - r), c.s(cx + r), c.s(cy + r));
}

pub unsafe fn text(c: &Ctx, s: &str, x: i32, y: i32, cell: i32, b: HBRUSH) {
    let r = (cell * 4 / 10).max(1);
    for (i, ch) in s.chars().enumerate() {
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    let cx = x + i as i32 * 6 * cell + col * cell + cell / 2;
                    let cy = y + row as i32 * cell + cell / 2;
                    dot(c, b, cx, cy, r);
                }
            }
        }
    }
}

pub unsafe fn title(c: &Ctx, word: &str, x: i32, y: i32, cell: i32) {
    text(c, word, x, y, cell, c.fg);
    let end = x + tw(word, cell);
    dot(c, c.acc, end - cell / 2, y + 6 * cell + cell / 2, (cell * 4 / 10).max(2));
}

pub unsafe fn pill(c: &Ctx, x: i32, y: i32, w: i32, h: i32, b: HBRUSH) {
    SelectObject(c.dc, b);
    RoundRect(c.dc, c.s(x), c.s(y), c.s(x + w), c.s(y + h), c.s(h), c.s(h));
}

pub unsafe fn toggle(c: &Ctx, x: i32, y: i32, on: bool) {
    pill(c, x, y, 64, 30, if on { c.acc } else { c.dark });
    let kx = if on { x + 48 } else { x + 16 };
    dot(c, if on { c.fg } else { c.dim }, kx, y + 15, 11);
}

pub unsafe fn button(c: &Ctx, r: R, label: &str, primary: bool) {
    pill(c, r.0, r.1, r.2, r.3, if primary { c.acc } else { c.dark });
    let fg = if primary { c.fg } else { c.dim };
    text(c, label, r.0 + (r.2 - tw(label, 3)) / 2, r.1 + (r.3 - 21) / 2, 3, fg);
}

pub unsafe fn paint(hwnd: HWND, scale: i32, pal: &Palette, f: impl FnOnce(&Ctx, i32, i32)) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old = SelectObject(mem, bmp);
    let c = Ctx {
        dc: mem,
        sc: scale,
        bg: brush(pal.bg),
        fg: brush(pal.fg),
        dim: brush(pal.dim),
        acc: brush(pal.acc),
        dark: brush(pal.dark),
    };
    SelectObject(mem, GetStockObject(NULL_PEN));
    FillRect(mem, &rc, c.bg);
    f(&c, rc.right * 100 / scale, rc.bottom * 100 / scale);
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, GetStockObject(NULL_BRUSH));
    SelectObject(mem, old);
    c.free();
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}
