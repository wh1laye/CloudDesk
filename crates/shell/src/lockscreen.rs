use std::cell::{Cell, RefCell};
use std::env;
use std::ptr;

use clouddesk_ui::gfx::{dot, paint, pill, text, tw, Palette};
use clouddesk_ui::{create, now, register, wide};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::Security::LogonUserW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SetFocus, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetSystemMetrics, SetForegroundWindow, SetTimer, ShowWindow,
    SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOW, WM_CHAR, WM_ERASEBKGND, WM_PAINT, WM_TIMER, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: &str = "CloudDeskLock";
const VK_L: u32 = 0x4C;
const MAX_PW: usize = 128;

struct Lock {
    hwnd: usize,
    pal: Option<Palette>,
    scale: i32,
    pw: Vec<u16>,
    wrong: bool,
    cx: i32,
    cy: i32,
}

thread_local! {
    static LK: RefCell<Lock> = const {
        RefCell::new(Lock {
            hwnd: 0,
            pal: None,
            scale: 100,
            pw: Vec::new(),
            wrong: false,
            cx: 0,
            cy: 0,
        })
    };
    static REG: Cell<bool> = const { Cell::new(false) };
}

fn verify(pw: &[u16]) -> bool {
    let user = env::var("USERNAME").unwrap_or_default();
    if user.is_empty() {
        return true;
    }
    let user = wide(&user);
    let domain = wide(&env::var("USERDOMAIN").unwrap_or_default());
    let mut p: Vec<u16> = pw.to_vec();
    p.push(0);
    let mut token: HANDLE = ptr::null_mut();
    let ok =
        unsafe { LogonUserW(user.as_ptr(), domain.as_ptr(), p.as_ptr(), 2, 0, &mut token) } != 0;
    p.iter_mut().for_each(|c| *c = 0);
    if ok && !token.is_null() {
        unsafe {
            CloseHandle(token);
        }
    }
    ok
}

unsafe fn draw(hwnd: HWND) -> bool {
    let snap = LK.with(|l| {
        let l = l.borrow();
        l.pal.map(|p| (p, l.scale, l.pw.len(), l.wrong, l.cx, l.cy))
    });
    let Some((pal, scale, n, wrong, cx, cy)) = snap else {
        return false;
    };
    let t = now();
    paint(hwnd, scale, &pal, |c, _, _| {
        text(c, &t.hm, cx - tw(&t.hm, 14) / 2, cy - 190, 14, c.fg);
        text(c, &t.date, cx - tw(&t.date, 4) / 2, cy - 60, 4, c.dim);
        pill(c, cx - 180, cy + 10, 360, 56, c.dark);
        if n == 0 {
            text(c, "PASSWORD", cx - tw("PASSWORD", 3) / 2, cy + 27, 3, c.dim);
        } else {
            let shown = n.min(16) as i32;
            for i in 0..shown {
                dot(c, c.fg, cx - (shown - 1) * 9 + i * 18, cy + 38, 6);
            }
        }
        if wrong {
            text(
                c,
                "WRONG PASSWORD",
                cx - tw("WRONG PASSWORD", 3) / 2,
                cy + 90,
                3,
                c.acc,
            );
        }
    });
    true
}

unsafe fn unlock(hwnd: HWND) {
    LK.with(|l| {
        let mut l = l.borrow_mut();
        l.pw.iter_mut().for_each(|c| *c = 0);
        l.pw.clear();
        l.hwnd = 0;
    });
    DestroyWindow(hwnd);
}

unsafe fn on_char(hwnd: HWND, ch: u32) {
    let mut submit: Option<Vec<u16>> = None;
    LK.with(|l| {
        let mut l = l.borrow_mut();
        l.wrong = false;
        match ch {
            8 => {
                l.pw.pop();
            }
            27 => l.pw.clear(),
            13 => submit = Some(l.pw.clone()),
            32..=0xFFFF if l.pw.len() < MAX_PW => {
                l.pw.push(ch as u16);
            }
            _ => {}
        }
    });
    if let Some(mut p) = submit {
        if verify(&p) {
            p.iter_mut().for_each(|c| *c = 0);
            unlock(hwnd);
            return;
        }
        p.iter_mut().for_each(|c| *c = 0);
        LK.with(|l| {
            let mut l = l.borrow_mut();
            l.wrong = true;
            l.pw.clear();
        });
    }
    InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if draw(hwnd) {
                0
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_ERASEBKGND => 1,
        WM_CHAR => {
            on_char(hwnd, wp as u32);
            0
        }
        WM_TIMER => {
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn init() -> bool {
    register(&wide(CLASS), Some(proc))
}

pub unsafe fn hotkey(on: bool, ctl: HWND) {
    if ctl.is_null() {
        return;
    }
    let registered = REG.with(|r| r.get());
    if on && !registered {
        let ok = RegisterHotKey(ctl, 2, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_L) != 0;
        REG.with(|r| r.set(ok));
    } else if !on && registered {
        UnregisterHotKey(ctl, 2);
        REG.with(|r| r.set(false));
    }
}

pub unsafe fn lock(pal: Palette, scale: i32) {
    if LK.with(|l| l.borrow().hwnd) != 0 {
        return;
    }
    let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let cx = (-vx + GetSystemMetrics(SM_CXSCREEN) / 2) * 100 / scale;
    let cy = (-vy + GetSystemMetrics(SM_CYSCREEN) / 2) * 100 / scale;
    let hwnd = create(
        &wide(CLASS),
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        WS_POPUP,
        vx,
        vy,
        vw,
        vh,
    );
    if hwnd.is_null() {
        return;
    }
    LK.with(|l| {
        let mut l = l.borrow_mut();
        l.hwnd = hwnd as usize;
        l.pal = Some(pal);
        l.scale = scale;
        l.pw.clear();
        l.wrong = false;
        l.cx = cx;
        l.cy = cy;
    });
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
    SetTimer(hwnd, 1, 10000, None);
}
