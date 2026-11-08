use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicU8, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};
use windows_sys::Win32::UI::Controls::MARGINS;

const DARK_MODE: i32 = 20;
const CORNERS: i32 = 33;
const BACKDROP: i32 = 38;
const ROUND: i32 = 2;
const ACRYLIC: i32 = 3;

static LEVEL: AtomicU8 = AtomicU8::new(0);

pub fn set(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

unsafe fn attr(hwnd: HWND, id: i32, v: i32) -> bool {
    DwmSetWindowAttribute(
        hwnd,
        id,
        &v as *const i32 as *const c_void,
        size_of::<i32>() as u32,
    ) >= 0
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
