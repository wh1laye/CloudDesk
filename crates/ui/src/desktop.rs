use std::mem::{size_of, zeroed};
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Shell::{SHAppBarMessage, APPBARDATA};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetWindowLongPtrW, SendMessageTimeoutW,
    SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_STYLE, HWND_BOTTOM,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_CHILD, WS_POPUP,
};

use crate::config::Config;
use crate::wide;

const SPAWN_WORKER: u32 = 0x052C;
const ABM_GETSTATE: u32 = 4;
const ABM_SETSTATE: u32 = 10;
const ABS_AUTOHIDE: u32 = 1;
const SW_HIDE_CMD: i32 = 0;
const SW_SHOWNA_CMD: i32 = 8;

unsafe extern "system" fn find_worker(top: HWND, lp: LPARAM) -> i32 {
    let view = FindWindowExW(
        top,
        ptr::null_mut(),
        wide("SHELLDLL_DefView").as_ptr(),
        ptr::null(),
    );
    if view.is_null() {
        return 1;
    }
    let worker = FindWindowExW(ptr::null_mut(), top, wide("WorkerW").as_ptr(), ptr::null());
    *(lp as *mut usize) = worker as usize;
    0
}

pub unsafe fn layer() -> HWND {
    let progman = FindWindowW(wide("Progman").as_ptr(), ptr::null());
    if progman.is_null() {
        return ptr::null_mut();
    }
    let mut res = 0usize;
    SendMessageTimeoutW(progman, SPAWN_WORKER, 0xD, 1, 0, 1000, &mut res);
    let child = FindWindowExW(progman, ptr::null_mut(), wide("WorkerW").as_ptr(), ptr::null());
    if !child.is_null() {
        return child;
    }
    let mut found = 0usize;
    EnumWindows(Some(find_worker), &mut found as *mut usize as LPARAM);
    found as HWND
}

pub unsafe fn attach(win: HWND, layer: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    if win.is_null() || layer.is_null() {
        return false;
    }
    let style = GetWindowLongPtrW(win, GWL_STYLE);
    SetWindowLongPtrW(win, GWL_STYLE, (style & !(WS_POPUP as isize)) | WS_CHILD as isize);
    if SetParent(win, layer).is_null() {
        SetWindowLongPtrW(win, GWL_STYLE, style);
        return false;
    }
    let mut pt = POINT { x, y };
    ScreenToClient(layer, &mut pt);
    SetWindowPos(
        win,
        HWND_BOTTOM,
        pt.x,
        pt.y,
        w,
        h,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    true
}

unsafe fn tray_windows() -> Vec<HWND> {
    let mut v = Vec::new();
    let main = FindWindowW(wide("Shell_TrayWnd").as_ptr(), ptr::null());
    if !main.is_null() {
        v.push(main);
    }
    let mut prev: HWND = ptr::null_mut();
    loop {
        let next = FindWindowExW(
            ptr::null_mut(),
            prev,
            wide("Shell_SecondaryTrayWnd").as_ptr(),
            ptr::null(),
        );
        if next.is_null() {
            break;
        }
        v.push(next);
        prev = next;
    }
    v
}

unsafe fn appbar_data() -> APPBARDATA {
    let mut abd: APPBARDATA = zeroed();
    abd.cbSize = size_of::<APPBARDATA>() as u32;
    abd.hWnd = FindWindowW(wide("Shell_TrayWnd").as_ptr(), ptr::null());
    abd
}

unsafe fn state() -> u32 {
    let mut abd = appbar_data();
    SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32
}

unsafe fn set_state(v: u32) {
    let mut abd = appbar_data();
    abd.lParam = v as LPARAM;
    SHAppBarMessage(ABM_SETSTATE, &mut abd);
}

pub unsafe fn hide_taskbar() {
    let cur = state();
    let mut c = Config::load();
    if c.taskbar_autohide.is_none() {
        c.taskbar_autohide = Some(cur & ABS_AUTOHIDE != 0);
        if c.save().is_err() {
            return;
        }
    }
    if cur & ABS_AUTOHIDE == 0 {
        set_state(cur | ABS_AUTOHIDE);
    }
    keep_hidden();
}

pub unsafe fn keep_hidden() {
    for w in tray_windows() {
        ShowWindow(w, SW_HIDE_CMD);
    }
}

pub unsafe fn restore_taskbar() {
    let mut c = Config::load();
    if let Some(orig) = c.taskbar_autohide {
        let cur = state();
        set_state(if orig {
            cur | ABS_AUTOHIDE
        } else {
            cur & !ABS_AUTOHIDE
        });
        c.taskbar_autohide = None;
        let _ = c.save();
    }
    for w in tray_windows() {
        ShowWindow(w, SW_SHOWNA_CMD);
    }
}
