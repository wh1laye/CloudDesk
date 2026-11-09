use std::cell::RefCell;
use std::env;
use std::mem::{size_of, zeroed};
use std::ptr;

use clouddesk_ui::config::Config;
use clouddesk_ui::gfx::{paint, pill, surface, text, tw, Palette};
use clouddesk_ui::{wide, HOST_CLASS};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, FindWindowW, GetClientRect, GetSystemMetrics, PostMessageW,
    PrivateExtractIconsW, SetWindowPos, DI_NORMAL, HICON, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_APP,
};

pub const WIN_H: i32 = 76;
pub const STRIP_H: i32 = 88;

const BASE: i32 = 44;
const PEAK: i32 = 64;
const SPAN: i32 = 110;
const GAP: i32 = 8;
const PAD: i32 = 14;
const SLACK: i32 = 120;
const ICON_SRC: i32 = 128;

struct Dock {
    key: String,
    icons: Vec<usize>,
    mouse: Option<i32>,
    tracking: bool,
    shape: (i32, i32, i32),
}

thread_local! {
    static DK: RefCell<Dock> = const {
        RefCell::new(Dock {
            key: String::new(),
            icons: Vec::new(),
            mouse: None,
            tracking: false,
            shape: (0, 0, 0),
        })
    };
}

fn base_total(n: usize) -> i32 {
    n as i32 * (BASE + GAP) - GAP + 2 * PAD
}

fn layout(n: usize, lw: i32, mouse: Option<i32>) -> (i32, i32, Vec<(i32, i32)>) {
    let cx = lw / 2;
    let first = cx - base_total(n) / 2 + PAD + BASE / 2;
    let sizes: Vec<i32> = (0..n)
        .map(|i| {
            let xi = first + i as i32 * (BASE + GAP);
            match mouse {
                Some(m) => {
                    let d = (m - xi).abs();
                    if d >= SPAN {
                        BASE
                    } else {
                        BASE + (PEAK - BASE) * (SPAN - d) / SPAN
                    }
                }
                None => BASE,
            }
        })
        .collect();
    let total = sizes.iter().sum::<i32>() + (n as i32 - 1).max(0) * GAP + 2 * PAD;
    let pill_x = cx - total / 2;
    let mut x = pill_x + PAD;
    let mut out = Vec::with_capacity(n);
    for sz in sizes {
        out.push((x, sz));
        x += sz + GAP;
    }
    (pill_x, total, out)
}

unsafe fn load_icon(target: &str) -> usize {
    let full = if target.contains('\\') || target.contains(':') {
        target.to_string()
    } else {
        format!("{}\\{}", env::var("SystemRoot").unwrap_or_default(), target)
    };
    let w = wide(&full);
    if full.to_lowercase().ends_with(".exe") {
        let mut h: HICON = ptr::null_mut();
        let mut id = 0u32;
        let n = PrivateExtractIconsW(w.as_ptr(), 0, ICON_SRC, ICON_SRC, &mut h, &mut id, 1, 0);
        if n >= 1 && !h.is_null() {
            return h as usize;
        }
    }
    let mut fi: SHFILEINFOW = zeroed();
    SHGetFileInfoW(
        w.as_ptr(),
        0,
        &mut fi,
        size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON,
    );
    fi.hIcon as usize
}

pub unsafe fn place(hwnd: HWND, scale: i32, n: usize) {
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let w = ((base_total(n) + SLACK) * scale / 100).min(sw);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        (sw - w) / 2,
        sh - STRIP_H * scale / 100,
        w,
        WIN_H * scale / 100,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

pub unsafe fn draw(hwnd: HWND, scale: i32, pal: &Palette, items: &[(String, String)]) {
    let key = items
        .iter()
        .map(|i| i.1.as_str())
        .collect::<Vec<_>>()
        .join("|");
    let (icons, mouse) = DK.with(|d| {
        let mut d = d.borrow_mut();
        if d.key != key {
            for &i in &d.icons {
                if i != 0 {
                    DestroyIcon(i as HICON);
                }
            }
            d.icons = items.iter().map(|i| load_icon(&i.1)).collect();
            d.key = key;
        }
        (d.icons.clone(), d.mouse)
    });
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    if !items.is_empty() {
        let (rx, rw, _) = layout(items.len(), rc.right * 100 / scale, mouse);
        let shape = (rx, rw, scale);
        let changed = DK.with(|d| {
            let mut d = d.borrow_mut();
            let diff = d.shape != shape;
            d.shape = shape;
            diff
        });
        if changed {
            let r = 44 * scale / 100;
            SetWindowRgn(
                hwnd,
                CreateRoundRectRgn(
                    rx * scale / 100,
                    0,
                    (rx + rw) * scale / 100 + 1,
                    WIN_H * scale / 100 + 1,
                    r,
                    r,
                ),
                1,
            );
        }
    }
    paint(hwnd, scale, pal, |c, lw, _| {
        if items.is_empty() {
            return;
        }
        let (px, pw, pos) = layout(items.len(), lw, mouse);
        pill(c, px, 0, pw, WIN_H, surface(c));
        for (i, &(x, sz)) in pos.iter().enumerate() {
            let y = WIN_H - 8 - sz;
            let icon = icons.get(i).copied().unwrap_or(0);
            if icon != 0 {
                DrawIconEx(
                    c.dc,
                    c.s(x),
                    c.s(y),
                    icon as HICON,
                    c.s(sz),
                    c.s(sz),
                    0,
                    ptr::null_mut(),
                    DI_NORMAL,
                );
            } else {
                pill(c, x, y, sz, sz, c.acc);
                let first: String = items[i].0.chars().take(1).collect();
                text(
                    c,
                    &first,
                    x + (sz - tw(&first, 4)) / 2,
                    y + (sz - 28) / 2,
                    4,
                    c.fg,
                );
            }
        }
    });
}

pub unsafe fn on_move(hwnd: HWND, scale: i32, px: i32) {
    let arm = DK.with(|d| {
        let mut d = d.borrow_mut();
        d.mouse = Some(px * 100 / scale);
        let first = !d.tracking;
        d.tracking = true;
        first
    });
    if arm {
        let mut e = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut e);
    }
    InvalidateRect(hwnd, ptr::null(), 0);
}

pub unsafe fn on_leave(hwnd: HWND) {
    DK.with(|d| {
        let mut d = d.borrow_mut();
        d.mouse = None;
        d.tracking = false;
    });
    InvalidateRect(hwnd, ptr::null(), 0);
}

pub unsafe fn hit(hwnd: HWND, scale: i32, n: usize, px: i32) -> Option<usize> {
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let x = px * 100 / scale;
    let (_, _, pos) = layout(n, rc.right * 100 / scale, Some(x));
    pos.iter().position(|&(ix, sz)| x >= ix && x < ix + sz)
}

fn notify() {
    unsafe {
        let host = FindWindowW(wide(HOST_CLASS).as_ptr(), ptr::null());
        if !host.is_null() {
            PostMessageW(host, WM_APP + 1, 0, 0);
        }
    }
}

pub fn pin(name: &str, path: &str) {
    let mut c = Config::load();
    if c.dock.iter().any(|d| d.1.eq_ignore_ascii_case(path)) {
        return;
    }
    c.dock.push((name.to_string(), path.to_string()));
    if c.save().is_ok() {
        notify();
    }
}

pub fn unpin(i: usize) {
    let mut c = Config::load();
    if i < c.dock.len() && c.dock.len() > 1 {
        c.dock.remove(i);
        if c.save().is_ok() {
            notify();
        }
    }
}
