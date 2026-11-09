use std::mem::zeroed;
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, GetStockObject,
    GetTextExtentPoint32W, RoundRect, SelectObject, SetBkMode, SetTextColor, HBRUSH, HDC,
    NULL_BRUSH, NULL_PEN, PAINTSTRUCT, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::gp::{argb, Gfx, Image};
use crate::wide;

const DT_VCENTER: u32 = 0x04;
const DT_SINGLELINE: u32 = 0x20;
const DT_NOCLIP: u32 = 0x100;
const DT_NOPREFIX: u32 = 0x800;
const AA_QUALITY: u32 = 4;
const BK_TRANSPARENT: i32 = 1;

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
        "rose" => Palette {
            bg: (28, 12, 20),
            fg: (252, 236, 242),
            dim: (176, 132, 150),
            acc: (255, 105, 160),
            dark: (66, 28, 46),
            glass: 0,
        },
        "mint" => Palette {
            bg: (8, 24, 20),
            fg: (232, 252, 244),
            dim: (120, 164, 150),
            acc: (60, 220, 160),
            dark: (20, 56, 46),
            glass: 0,
        },
        "violet" => Palette {
            bg: (18, 10, 32),
            fg: (240, 232, 255),
            dim: (146, 128, 180),
            acc: (160, 110, 255),
            dark: (44, 28, 78),
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
    pub rgb: [u32; 5],
    pub pal: Palette,
    pub gp: Option<Gfx>,
}

pub fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn colorref(t: Rgb) -> u32 {
    t.0 as u32 | (t.1 as u32) << 8 | (t.2 as u32) << 16
}

unsafe fn brush(t: Rgb) -> HBRUSH {
    CreateSolidBrush(t.0 as u32 | (t.1 as u32) << 8 | (t.2 as u32) << 16)
}

impl Ctx {
    pub fn s(&self, v: i32) -> i32 {
        v * self.sc / 100
    }

    pub fn f(&self, v: f32) -> f32 {
        v * self.sc as f32 / 100.0
    }

    pub fn col(&self, b: HBRUSH) -> (u8, u8, u8) {
        let p = &self.pal;
        let all = [(self.bg, p.bg), (self.fg, p.fg), (self.dim, p.dim), (self.acc, p.acc), (self.dark, p.dark)];
        all.iter().find(|(x, _)| *x == b).map(|(_, v)| *v).unwrap_or(p.fg)
    }

    pub fn flush(&self) {
        if let Some(g) = &self.gp {
            g.flush();
        }
    }

    unsafe fn free(&self) {
        for b in [self.bg, self.fg, self.dim, self.acc, self.dark] {
            DeleteObject(b);
        }
    }
}

fn em(cell: i32) -> i32 {
    (cell * 11 / 2).max(12)
}

unsafe fn make_font(px: i32) -> *mut core::ffi::c_void {
    let face = if px >= 28 {
        "Segoe UI Variable Display"
    } else {
        "Segoe UI Variable Text"
    };
    let weight = if px >= 28 { 300 } else { 400 };
    CreateFontW(
        -px,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        1,
        0,
        0,
        AA_QUALITY,
        0,
        wide(face).as_ptr(),
    )
}

pub fn tw(s: &str, cell: i32) -> i32 {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.is_empty() {
        return 0;
    }
    unsafe {
        let dc = CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() {
            return u.len() as i32 * em(cell) / 2;
        }
        let font = make_font(em(cell));
        let old = SelectObject(dc, font);
        let mut sz: SIZE = zeroed();
        let ok = GetTextExtentPoint32W(dc, u.as_ptr(), u.len() as i32, &mut sz);
        SelectObject(dc, old);
        DeleteObject(font);
        DeleteDC(dc);
        if ok == 0 {
            u.len() as i32 * em(cell) / 2
        } else {
            sz.cx
        }
    }
}

fn color_of(c: &Ctx, b: HBRUSH) -> u32 {
    let all = [c.bg, c.fg, c.dim, c.acc, c.dark];
    all.iter()
        .position(|x| *x == b)
        .map(|i| c.rgb[i])
        .unwrap_or(c.rgb[1])
}

pub unsafe fn dot(c: &Ctx, b: HBRUSH, cx: i32, cy: i32, r: i32) {
    if let Some(g) = &c.gp {
        g.fill_dot(c.f(cx as f32), c.f(cy as f32), c.f(r as f32), argb(c.col(b), 255));
        return;
    }
    SelectObject(c.dc, b);
    Ellipse(c.dc, c.s(cx - r), c.s(cy - r), c.s(cx + r), c.s(cy + r));
}

pub fn disc(c: &Ctx, cx: f32, cy: f32, r: f32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        g.fill_dot(c.f(cx), c.f(cy), c.f(r), argb(rgb, a));
    }
}

pub fn ring(c: &Ctx, cx: f32, cy: f32, r: f32, w: f32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        g.ring(c.f(cx), c.f(cy), c.f(r), c.f(w), argb(rgb, a));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn arc(c: &Ctx, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, w: f32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        g.arc(c.f(cx), c.f(cy), c.f(r), start, sweep, c.f(w), argb(rgb, a));
    }
}

pub fn stroke(c: &Ctx, p1: (f32, f32), p2: (f32, f32), w: f32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        g.line((c.f(p1.0), c.f(p1.1)), (c.f(p2.0), c.f(p2.1)), c.f(w), argb(rgb, a));
    }
}

pub fn rrect(c: &Ctx, r: R, d: i32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        g.fill_round(c.f(r.0 as f32), c.f(r.1 as f32), c.f(r.2 as f32), c.f(r.3 as f32), c.f(d as f32), argb(rgb, a));
    }
}

pub fn rgrad(c: &Ctx, r: R, d: i32, top: (u8, u8, u8), bottom: (u8, u8, u8)) {
    if let Some(g) = &c.gp {
        let rect = (c.f(r.0 as f32), c.f(r.1 as f32), c.f(r.2 as f32), c.f(r.3 as f32));
        g.fill_round_grad(rect, c.f(d as f32), argb(top, 255), argb(bottom, 255));
    }
}

pub fn routline(c: &Ctx, r: R, d: i32, w: f32, rgb: (u8, u8, u8), a: u8) {
    if let Some(g) = &c.gp {
        let rect = (
            c.f(r.0 as f32) + c.f(w) / 2.0,
            c.f(r.1 as f32) + c.f(w) / 2.0,
            c.f(r.2 as f32) - c.f(w),
            c.f(r.3 as f32) - c.f(w),
        );
        g.stroke_round(rect, c.f(d as f32), c.f(w), argb(rgb, a));
    }
}

pub fn picture(c: &Ctx, img: &Image, r: R, d: i32) {
    if let Some(g) = &c.gp {
        g.image_round(img, (c.s(r.0), c.s(r.1), c.s(r.2), c.s(r.3)), c.f(d as f32));
    }
}

pub unsafe fn text(c: &Ctx, s: &str, x: i32, y: i32, cell: i32, b: HBRUSH) {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.is_empty() {
        return;
    }
    c.flush();
    let font = make_font(c.s(em(cell)));
    let old = SelectObject(c.dc, font);
    SetBkMode(c.dc, BK_TRANSPARENT);
    SetTextColor(c.dc, color_of(c, b));
    let mut rc = RECT {
        left: c.s(x),
        top: c.s(y),
        right: c.s(x + 2400),
        bottom: c.s(y + 7 * cell),
    };
    DrawTextW(
        c.dc,
        u.as_ptr(),
        u.len() as i32,
        &mut rc,
        DT_VCENTER | DT_SINGLELINE | DT_NOCLIP | DT_NOPREFIX,
    );
    SelectObject(c.dc, old);
    DeleteObject(font);
}

unsafe fn font_px(px: i32, bold: bool) -> *mut core::ffi::c_void {
    let face = if px >= 28 {
        "Segoe UI Variable Display"
    } else {
        "Segoe UI Variable Text"
    };
    CreateFontW(
        -px,
        0,
        0,
        0,
        if bold { 600 } else { 400 },
        0,
        0,
        0,
        1,
        0,
        0,
        AA_QUALITY,
        0,
        wide(face).as_ptr(),
    )
}

pub fn measure(s: &str, px: i32, bold: bool) -> i32 {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.is_empty() {
        return 0;
    }
    unsafe {
        let dc = CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() {
            return u.len() as i32 * px / 2;
        }
        let font = font_px(px, bold);
        let old = SelectObject(dc, font);
        let mut sz: SIZE = zeroed();
        let ok = GetTextExtentPoint32W(dc, u.as_ptr(), u.len() as i32, &mut sz);
        SelectObject(dc, old);
        DeleteObject(font);
        DeleteDC(dc);
        if ok == 0 {
            u.len() as i32 * px / 2
        } else {
            sz.cx
        }
    }
}

pub unsafe fn txt(c: &Ctx, s: &str, x: i32, cy: i32, px: i32, rgb: (u8, u8, u8), bold: bool) {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.is_empty() {
        return;
    }
    c.flush();
    let font = font_px(c.s(px), bold);
    let old = SelectObject(c.dc, font);
    SetBkMode(c.dc, BK_TRANSPARENT);
    SetTextColor(c.dc, colorref(rgb));
    let mut rc = RECT {
        left: c.s(x),
        top: c.s(cy) - c.s(px),
        right: c.s(x) + 4800,
        bottom: c.s(cy) + c.s(px),
    };
    DrawTextW(
        c.dc,
        u.as_ptr(),
        u.len() as i32,
        &mut rc,
        DT_VCENTER | DT_SINGLELINE | DT_NOCLIP | DT_NOPREFIX,
    );
    SelectObject(c.dc, old);
    DeleteObject(font);
}

pub unsafe fn title(c: &Ctx, word: &str, x: i32, y: i32, cell: i32) {
    text(c, word, x, y, cell, c.fg);
}

pub unsafe fn pill(c: &Ctx, x: i32, y: i32, w: i32, h: i32, b: HBRUSH) {
    if c.gp.is_some() {
        rrect(c, (x, y, w, h), h, c.col(b), 255);
        return;
    }
    SelectObject(c.dc, b);
    RoundRect(c.dc, c.s(x), c.s(y), c.s(x + w), c.s(y + h), c.s(h), c.s(h));
}

pub unsafe fn toggle(c: &Ctx, x: i32, y: i32, on: bool) {
    pill(c, x, y, 64, 30, if on { c.acc } else { c.dark });
    let kx = if on { x + 48 } else { x + 16 };
    if c.gp.is_some() {
        disc(c, kx as f32, y as f32 + 16.0, 12.0, (0, 0, 0), 40);
        disc(c, kx as f32, y as f32 + 15.0, 11.0, if on { c.pal.fg } else { c.pal.dim }, 255);
        return;
    }
    dot(c, if on { c.fg } else { c.dim }, kx, y + 15, 11);
}

pub unsafe fn button(c: &Ctx, r: R, label: &str, primary: bool) {
    pill(c, r.0, r.1, r.2, r.3, if primary { c.acc } else { c.dark });
    let fg = if primary { c.fg } else { c.dim };
    text(
        c,
        label,
        r.0 + (r.2 - tw(label, 3)) / 2,
        r.1 + (r.3 - 21) / 2,
        3,
        fg,
    );
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
        rgb: [
            colorref(pal.bg),
            colorref(pal.fg),
            colorref(pal.dim),
            colorref(pal.acc),
            colorref(pal.dark),
        ],
        pal: *pal,
        gp: Gfx::new(mem),
    };
    SelectObject(mem, GetStockObject(NULL_PEN));
    FillRect(mem, &rc, c.bg);
    f(&c, rc.right * 100 / scale, rc.bottom * 100 / scale);
    c.flush();
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, GetStockObject(NULL_BRUSH));
    SelectObject(mem, old);
    c.free();
    drop(c);
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}

pub unsafe fn round(c: &Ctx, x: i32, y: i32, w: i32, h: i32, r: i32, b: HBRUSH) {
    if c.gp.is_some() {
        rrect(c, (x, y, w, h), r, c.col(b), 255);
        return;
    }
    SelectObject(c.dc, b);
    RoundRect(c.dc, c.s(x), c.s(y), c.s(x + w), c.s(y + h), c.s(r), c.s(r));
}

pub unsafe fn fill(c: &Ctx, x: i32, y: i32, w: i32, h: i32, r: i32, t: (u8, u8, u8)) {
    if c.gp.is_some() {
        rrect(c, (x, y, w, h), r, t, 255);
        return;
    }
    let b = brush(t);
    round(c, x, y, w, h, r, b);
    SelectObject(c.dc, GetStockObject(NULL_BRUSH));
    DeleteObject(b);
}
