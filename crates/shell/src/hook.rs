use std::cell::Cell;
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYEVENTF_KEYUP, VK_LWIN, VK_RWIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_HOTKEY, WM_KEYUP, WM_SYSKEYUP,
};

const VK_D: u32 = 0x44;
const FILLER: u8 = 0xE8;

thread_local! {
    static TARGET: Cell<usize> = const { Cell::new(0) };
    static HOOK: Cell<usize> = const { Cell::new(0) };
    static HELD: Cell<bool> = const { Cell::new(false) };
}

unsafe fn win_down() -> bool {
    GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0
}

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lp as *const KBDLLHOOKSTRUCT);
        if k.vkCode == VK_D && k.flags & LLKHF_INJECTED == 0 {
            let m = wp as u32;
            let up = m == WM_KEYUP || m == WM_SYSKEYUP;
            if up {
                if HELD.with(|h| h.replace(false)) {
                    return 1;
                }
            } else if HELD.with(|h| h.get()) {
                return 1;
            } else if win_down() {
                HELD.with(|h| h.set(true));
                keybd_event(FILLER, 0, 0, 0);
                keybd_event(FILLER, 0, KEYEVENTF_KEYUP, 0);
                let target = TARGET.with(|t| t.get()) as HWND;
                if !target.is_null() {
                    PostMessageW(target, WM_HOTKEY, 1, 0);
                }
                return 1;
            }
        }
    }
    CallNextHookEx(ptr::null_mut(), code, wp, lp)
}

pub unsafe fn enable(on: bool, target: HWND) {
    let cur = HOOK.with(|h| h.get());
    if on && cur == 0 {
        TARGET.with(|t| t.set(target as usize));
        let h = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(proc),
            GetModuleHandleW(ptr::null()),
            0,
        );
        HOOK.with(|x| x.set(h as usize));
    } else if !on && cur != 0 {
        UnhookWindowsHookEx(cur as HHOOK);
        HOOK.with(|x| x.set(0));
        HELD.with(|x| x.set(false));
    }
}
