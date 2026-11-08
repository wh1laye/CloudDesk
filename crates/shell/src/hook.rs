use std::cell::Cell;
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN,
    VK_SHIFT,
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
    static HELD: Cell<u32> = const { Cell::new(0) };
    static SEARCH: Cell<bool> = const { Cell::new(false) };
    static SPACES: Cell<bool> = const { Cell::new(false) };
}

unsafe fn win_down() -> bool {
    GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0
}

unsafe fn others_down() -> bool {
    GetAsyncKeyState(VK_CONTROL as i32) < 0
        || GetAsyncKeyState(VK_MENU as i32) < 0
        || GetAsyncKeyState(VK_SHIFT as i32) < 0
}

fn action(vk: u32) -> Option<usize> {
    if vk == VK_D && SEARCH.with(|s| s.get()) {
        Some(1)
    } else if (0x31..=0x39).contains(&vk) && SPACES.with(|s| s.get()) {
        Some(10 + (vk - 0x30) as usize)
    } else {
        None
    }
}

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lp as *const KBDLLHOOKSTRUCT);
        if k.flags & LLKHF_INJECTED == 0 {
            if let Some(id) = action(k.vkCode) {
                let m = wp as u32;
                let up = m == WM_KEYUP || m == WM_SYSKEYUP;
                let held = HELD.with(|h| h.get());
                if up {
                    if held == k.vkCode {
                        HELD.with(|h| h.set(0));
                        return 1;
                    }
                } else if held == k.vkCode {
                    return 1;
                } else if win_down() && !others_down() {
                    HELD.with(|h| h.set(k.vkCode));
                    keybd_event(FILLER, 0, 0, 0);
                    keybd_event(FILLER, 0, KEYEVENTF_KEYUP, 0);
                    let target = TARGET.with(|t| t.get()) as HWND;
                    if !target.is_null() {
                        PostMessageW(target, WM_HOTKEY, id, 0);
                    }
                    return 1;
                }
            }
        }
    }
    CallNextHookEx(ptr::null_mut(), code, wp, lp)
}

pub fn configure(search: bool, spaces: bool) {
    SEARCH.with(|s| s.set(search));
    SPACES.with(|s| s.set(spaces));
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
        HELD.with(|x| x.set(0));
    }
}
