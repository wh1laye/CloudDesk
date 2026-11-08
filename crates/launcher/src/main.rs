#![windows_subsystem = "windows"]

use std::env;
use std::fs;
use std::mem::zeroed;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{exit, Command};
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

const WINLOGON: &str = r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon";
const HOST: &str = "clouddesk.exe";
const SELF_EXE: &str = "clouddesk-launcher.exe";
const BACKUP: &str = "shell-backup.txt";
const NO_WINDOW: u32 = 0x0800_0000;
const MAX_FAST_FAILS: u32 = 3;
const VK_E: u32 = 0x45;

fn run_hidden(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .creation_flags(NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn backup_path() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|a| PathBuf::from(a).join("CloudDesk").join(BACKUP))
}

fn shell_is_ours() -> bool {
    Command::new("reg")
        .args(["query", WINLOGON, "/v", "Shell"])
        .creation_flags(NO_WINDOW)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains(SELF_EXE)
        })
        .unwrap_or(false)
}

fn restore_shell() {
    let saved = backup_path().and_then(|p| fs::read_to_string(p).ok());
    match saved.as_deref().and_then(|t| t.trim().split_once('|')) {
        Some((kind, value)) => {
            run_hidden(
                "reg",
                &["add", WINLOGON, "/v", "Shell", "/t", kind, "/d", value, "/f"],
            );
        }
        None => {
            if shell_is_ours() {
                run_hidden("reg", &["delete", WINLOGON, "/v", "Shell", "/f"]);
            }
        }
    }
}

fn explorer_running() -> bool {
    Command::new("tasklist")
        .args(["/fi", "imagename eq explorer.exe", "/nh"])
        .creation_flags(NO_WINDOW)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains("explorer.exe")
        })
        .unwrap_or(false)
}

fn start_explorer() {
    if !explorer_running() {
        let _ = Command::new("explorer.exe").spawn();
    }
}

fn recover() {
    run_hidden("taskkill", &["/im", HOST, "/f", "/t"]);
    restore_shell();
    start_explorer();
}

fn hotkey_guard() {
    thread::spawn(|| unsafe {
        let ok = RegisterHotKey(
            ptr::null_mut(),
            1,
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            VK_E,
        );
        if ok == 0 {
            return;
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                recover();
                exit(0);
            }
        }
    });
}

fn main() {
    if env::args().any(|a| a == "--recover") {
        recover();
        return;
    }
    hotkey_guard();
    let host = match env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(HOST)))
    {
        Some(h) if h.is_file() => h,
        _ => {
            recover();
            exit(1);
        }
    };
    let mut fails = 0u32;
    loop {
        let started = Instant::now();
        let clean = Command::new(&host)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if clean {
            start_explorer();
            return;
        }
        if started.elapsed() < Duration::from_secs(20) {
            fails += 1;
        } else {
            fails = 0;
        }
        if fails >= MAX_FAST_FAILS {
            recover();
            exit(1);
        }
        thread::sleep(Duration::from_secs(1));
    }
}
