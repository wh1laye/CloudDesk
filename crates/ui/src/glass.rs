use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicU8, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};
use windows_sys::Win32::UI::Controls::MARGINS;

const DARK_MODE: u32 = 20;
const CORNERS: u32 = 33;
const BACKDROP: u32 = 38;
const ROUND: i32 = 2;
const SQUARE: i32 = 1;
const ACRYLIC: i32 = 3;

static LEVEL: AtomicU8 = AtomicU8::new(0);

pub fn set(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

unsafe fn attr(hwnd: HWND, id: u32, v: i32) -> bool {
    DwmSetWindowAttribute(
        hwnd,
        id,
        &v as *const i32 as *const c_void,
        size_of::<i32>() as u32,
    ) >= 0
}

pub unsafe fn square(hwnd: HWND) {
    if !hwnd.is_null() {
        attr(hwnd, CORNERS, SQUARE);
    }
}

pub unsafe fn apply(hwnd: HWND) -> bool {
    let lv = level();
    if lv == 0 || hwnd.is_null() {
        return false;
    }
    attr(hwnd, DARK_MODE, if lv == 1 { 1 } else { 0 });
    attr(hwnd, CORNERS, ROUND);
    if !attr(hwnd, BACKDROP, ACRYLIC) {
        return false;
    }
    let m = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    DwmExtendFrameIntoClientArea(hwnd, &m) >= 0
}

const BORDER_COLOR: u32 = 34;
const CAPTION_COLOR: u32 = 35;
const TEXT_COLOR: u32 = 36;

fn colorref(t: (u8, u8, u8)) -> i32 {
    (t.0 as u32 | (t.1 as u32) << 8 | (t.2 as u32) << 16) as i32
}

pub unsafe fn chrome(hwnd: HWND, bg: (u8, u8, u8), fg: (u8, u8, u8)) {
    if hwnd.is_null() {
        return;
    }
    let dark = (bg.0 as u32 * 3 + bg.1 as u32 * 6 + bg.2 as u32) / 10 < 128;
    attr(hwnd, DARK_MODE, dark as i32);
    attr(hwnd, CAPTION_COLOR, colorref(bg));
    attr(hwnd, BORDER_COLOR, colorref(bg));
    attr(hwnd, TEXT_COLOR, colorref(fg));
}
