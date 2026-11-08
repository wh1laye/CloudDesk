use std::ptr;
use std::thread;
use std::time::Duration;

use clouddesk_ui::wide;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_CONTROL, VK_LEFT,
    VK_LWIN, VK_RIGHT, VK_RWIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_SPACES: u32 = WM_APP + 20;
const MAX: usize = 9;
const VK_D: u8 = 0x44;
const STEP_MS: u64 = 140;
const SETTLE_MS: u64 = 350;
const IDS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";
const SESSIONS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo";

fn read_bin(path: &str, name: &str) -> Option<Vec<u8>> {
    unsafe {
        let mut key: HKEY = ptr::null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            KEY_READ,
            &mut key,
        ) != 0
        {
            return None;
        }
        let n = wide(name);
        let mut len: u32 = 0;
        let mut out = None;
        if RegQueryValueExW(
            key,
            n.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut len,
        ) == 0
            && len > 0
            && len <= 4096
        {
            let mut buf = vec![0u8; len as usize];
            if RegQueryValueExW(
                key,
                n.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut len,
            ) == 0
            {
                buf.truncate(len as usize);
                out = Some(buf);
            }
        }
        RegCloseKey(key);
        out
    }
}

fn session() -> u32 {
    let mut sid = 0u32;
    unsafe {
        if ProcessIdToSessionId(GetCurrentProcessId(), &mut sid) == 0 {
            return 1;
        }
    }
    sid
}

pub fn state() -> (usize, usize) {
    let ids = read_bin(IDS, "VirtualDesktopIDs").unwrap_or_default();
    let count = ids.len() / 16;
    if count == 0 {
        return (0, 0);
    }
    let per_session = format!(r"{SESSIONS}\{}\VirtualDesktops", session());
    let cur = read_bin(&per_session, "CurrentVirtualDesktop")
        .or_else(|| read_bin(IDS, "CurrentVirtualDesktop"));
    let idx = cur
        .filter(|c| c.len() >= 16)
        .and_then(|c| ids.as_chunks::<16>().0.iter().position(|g| g == &c[..16]))
        .map(|i| i + 1)
        .unwrap_or(0);
    (count.min(MAX), idx)
}

fn win_held() -> bool {
    unsafe { GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0 }
}

fn combo(vk: u8, extended: bool) {
    let f = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
    let own_win = !win_held();
    unsafe {
        if own_win {
            keybd_event(VK_LWIN as u8, 0, 0, 0);
        }
        keybd_event(VK_CONTROL as u8, 0, 0, 0);
        keybd_event(vk, 0, f, 0);
        keybd_event(vk, 0, f | KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL as u8, 0, KEYEVENTF_KEYUP, 0);
        if own_win {
            keybd_event(VK_LWIN as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    thread::sleep(Duration::from_millis(STEP_MS));
}

fn run(n: usize) {
    let (count, mut cur) = state();
    if count == 0 || cur == 0 || n == 0 || n > MAX {
        return;
    }
    let mut have = count;
    while have < n {
        combo(VK_D, false);
        have += 1;
        cur = have;
    }
    while cur != n {
        if cur < n {
            combo(VK_RIGHT as u8, true);
            cur += 1;
        } else {
            combo(VK_LEFT as u8, true);
            cur -= 1;
        }
    }
}

pub fn goto(n: usize, notify: HWND) {
    let target = notify as usize;
    thread::spawn(move || {
        run(n);
        thread::sleep(Duration::from_millis(SETTLE_MS));
        unsafe {
            PostMessageW(target as HWND, WM_SPACES, 0, 0);
        }
    });
}
