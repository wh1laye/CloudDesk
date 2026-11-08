#![allow(clippy::missing_safety_doc)]

pub mod config;
pub mod font;
pub mod gfx;
pub mod wall;

use std::mem::{size_of, zeroed};
use std::ptr;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, LoadCursorW, RegisterClassExW,
    SetProcessDPIAware, TranslateMessage, IDC_ARROW, MSG, SW_SHOWNORMAL, WNDCLASSEXW, WNDPROC,
};

pub const HOST_CLASS: &str = "CloudDeskHost";

const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

pub struct Now {
    pub hm: String,
    pub date: String,
    pub sec: u32,
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn init_dpi() -> i32 {
    unsafe {
        SetProcessDPIAware();
        (GetDpiForSystem() as i32 * 100 / 96).max(100)
    }
}

pub unsafe fn register(class: &[u16], proc: WNDPROC) -> bool {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: GetModuleHandleW(ptr::null()),
        hIcon: ptr::null_mut(),
        hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class.as_ptr(),
        hIconSm: ptr::null_mut(),
    };
    RegisterClassExW(&wc) != 0
}

pub unsafe fn create(class: &[u16], ex: u32, style: u32, x: i32, y: i32, w: i32, h: i32) -> HWND {
    CreateWindowExW(
        ex,
        class.as_ptr(),
        class.as_ptr(),
        style,
        x,
        y,
        w,
        h,
        ptr::null_mut(),
        ptr::null_mut(),
        GetModuleHandleW(ptr::null()),
        ptr::null(),
    )
}

pub fn open(target: &str) {
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            wide("open").as_ptr(),
            wide(target).as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn now() -> Now {
    let mut t = unsafe { zeroed() };
    unsafe { GetLocalTime(&mut t) };
    Now {
        hm: format!("{:02}:{:02}", t.wHour, t.wMinute),
        date: format!(
            "{} {:02} {}",
            DAYS[t.wDayOfWeek as usize % 7],
            t.wDay,
            MONTHS[(t.wMonth as usize + 11) % 12]
        ),
        sec: (t.wSecond as u32).min(59),
    }
}

pub fn pump() {
    unsafe {
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
