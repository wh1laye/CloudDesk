#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::env;
use std::ptr;
use std::thread;

use clouddesk_ui::config::{file_name, Config, THEMES};
use clouddesk_ui::gfx::{button, dot, paint, palette, text, title, toggle, tw, Ctx, R};
use clouddesk_ui::{create, init_dpi, open, pump, register, wall, wide, HOST_CLASS};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DefWindowProcW, FindWindowW, GetSystemMetrics, PostMessageW,
    PostQuitMessage, SetForegroundWindow, SetWindowTextW, ShowWindow, SM_CXSCREEN, SM_CYSCREEN,
    SW_SHOW, WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WS_CAPTION,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

const CLASS: &str = "CloudDeskSettings";
const NAMES: [&str; 8] = [
    "DESKTOP",
    "ISLAND",
    "SEARCH",
    "VISUALIZER",
    "COLORS",
    "MUSIC",
    "WIDGETS",
    "LOCK",
];
const IMPL: [bool; 8] = [true, true, true, true, true, false, true, true];
const VIS_MODE: R = (50, 392, 360, 44);
const TABS: [&str; 3] = ["SWITCHES", "LOOK", "WALLPAPER"];
const START: R = (540, 450, 140, 44);
const STOP: R = (700, 450, 140, 44);
const PREV: R = (60, 300, 140, 48);
const NEXT: R = (220, 300, 140, 48);
const RESTORE: R = (380, 300, 260, 48);
const FOLDER: R = (60, 370, 220, 48);

#[derive(Clone)]
struct St {
    cfg: Config,
    tab: u8,
    scale: i32,
    msg: String,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St {
        cfg: Config::new(),
        tab: 0,
        scale: 100,
        msg: String::new(),
    });
}

enum Act {
    None,
    Tab(u8),
    Toggle(usize),
    Theme(usize),
    Mode,
    Wall(i32),
    Restore,
    Folder,
    Start,
    Stop,
}

fn row(i: usize) -> R {
    let i = i as i32;
    (50 + (i / 4) * 400, 126 + (i % 4) * 64, 360, 56)
}

fn hit(r: R, x: i32, y: i32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn decide(tab: u8, x: i32, y: i32) -> Act {
    if hit(START, x, y) {
        return Act::Start;
    }
    if hit(STOP, x, y) {
        return Act::Stop;
    }
    for i in 0..3i32 {
        if hit((60 + i * 190, 66, 170, 36), x, y) {
            return Act::Tab(i as u8);
        }
    }
    match tab {
        0 => {
            if hit(VIS_MODE, x, y) {
                return Act::Mode;
            }
            if let Some(i) = (0..8usize).find(|&i| hit(row(i), x, y)) {
                return Act::Toggle(i);
            }
        }
        1 => {
            if let Some(i) = (0..THEMES.len()).find(|&i| hit((60 + (i % 4) as i32 * 190, 220 + (i / 4) as i32 * 70, 170, 56), x, y)) {
                return Act::Theme(i);
            }
        }
        _ => {
            if hit(PREV, x, y) {
                return Act::Wall(-1);
            }
            if hit(NEXT, x, y) {
                return Act::Wall(1);
            }
            if hit(RESTORE, x, y) {
                return Act::Restore;
            }
            if hit(FOLDER, x, y) {
                return Act::Folder;
            }
        }
    }
    Act::None
}

unsafe fn host_window() -> HWND {
    FindWindowW(wide(HOST_CLASS).as_ptr(), ptr::null())
}

unsafe fn notify() {
    let h = host_window();
    if !h.is_null() {
        PostMessageW(h, WM_APP + 1, 0, 0);
    }
}

fn set_msg(m: &str) {
    ST.with(|s| s.borrow_mut().msg = m.to_string());
}

unsafe fn persist(cfg: &Config) {
    match cfg.save() {
        Ok(()) => notify(),
        Err(e) => set_msg(&e),
    }
}

fn host_exe() -> String {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("clouddesk.exe")))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

unsafe fn click(hwnd: HWND, px: i32, py: i32) {
    let act = ST.with(|s| {
        let s = s.borrow();
        decide(s.tab, px * 100 / s.scale, py * 100 / s.scale)
    });
    match act {
        Act::None => {}
        Act::Tab(t) => ST.with(|s| s.borrow_mut().tab = t),
        Act::Toggle(i) if IMPL[i] => {
            let cfg = ST.with(|s| {
                let mut s = s.borrow_mut();
                s.cfg.comps[i] = !s.cfg.comps[i];
                s.cfg.clone()
            });
            persist(&cfg);
        }
        Act::Toggle(_) => set_msg("NOT BUILT YET"),
        Act::Mode => {
            let cfg = ST.with(|s| {
                let mut s = s.borrow_mut();
                s.cfg.contour = !s.cfg.contour;
                s.cfg.clone()
            });
            persist(&cfg);
        }
        Act::Theme(i) => {
            let cfg = ST.with(|s| {
                let mut s = s.borrow_mut();
                s.cfg.theme = THEMES[i].to_string();
                s.cfg.clone()
            });
            persist(&cfg);
        }
        Act::Wall(d) => {
            let mut cfg = ST.with(|s| s.borrow().cfg.clone());
            match wall::step(&mut cfg, d) {
                Ok(()) => {
                    ST.with(|s| s.borrow_mut().cfg = cfg);
                    notify();
                }
                Err(e) => set_msg(&e),
            }
        }
        Act::Restore => {
            let cfg = ST.with(|s| s.borrow().cfg.clone());
            if let Err(e) = wall::restore(&cfg) {
                set_msg(&e);
            }
        }
        Act::Folder => {
            if wall::ensure().is_ok() {
                open(&wall::dir().to_string_lossy());
            }
        }
        Act::Start => {
            if host_window().is_null() {
                open(&host_exe());
            }
        }
        Act::Stop => {
            let h = host_window();
            if !h.is_null() {
                PostMessageW(h, WM_CLOSE, 0, 0);
            }
        }
    }
    InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe fn draw(c: &Ctx, s: &St) {
    dot(c, c.acc, 46, 38, 4);
    text(c, "CLOUDDESK", 60, 28, 3, c.dim);
    text(c, "SETTINGS", 820 - tw("SETTINGS", 3), 28, 3, c.dim);
    for (i, n) in TABS.iter().enumerate() {
        button(c, (60 + i as i32 * 190, 66, 170, 36), n, s.tab as usize == i);
    }
    match s.tab {
        0 => {
            for (i, name) in NAMES.iter().enumerate() {
                let r = row(i);
                text(c, name, r.0 + 10, r.1 + 16, 3, if IMPL[i] { c.fg } else { c.dim });
                toggle(c, r.0 + 250, r.1 + 12, IMPL[i] && s.cfg.comps[i]);
            }
            let mode = if s.cfg.contour { "VISUALIZER: CONTOUR" } else { "VISUALIZER: DOTS" };
            button(c, VIS_MODE, mode, false);
        }
        1 => {
            title(c, "YOUR COLORS", 60, 130, 6);
            for (i, n) in THEMES.iter().enumerate() {
                button(
                    c,
                    (60 + (i % 4) as i32 * 190, 220 + (i / 4) as i32 * 70, 170, 56),
                    &n.to_uppercase(),
                    s.cfg.theme == *n,
                );
            }
            if !s.cfg.comps[4] {
                text(c, "COLORS SWITCH IS OFF", 60, 430, 3, c.dim);
            }
        }
        _ => {
            title(c, "WALLPAPERS", 60, 130, 6);
            let l = wall::list();
            let name = l
                .get(s.cfg.wall_index)
                .map(|p| file_name(p))
                .unwrap_or_else(|| "NONE".to_string());
            let line = format!("{}/{} {}", s.cfg.wall_index.min(l.len().saturating_sub(1)) + 1, l.len(), name);
            text(c, &line, 60, 224, 4, c.fg);
            button(c, PREV, "PREV", false);
            button(c, NEXT, "NEXT", false);
            button(c, RESTORE, "RESTORE ORIGINAL", false);
            button(c, FOLDER, "OPEN FOLDER", false);
            text(c, "DROP BMP JPG PNG INTO THE FOLDER.", 60, 430, 3, c.dim);
        }
    }
    let running = !host_window().is_null();
    let status = if s.msg.is_empty() {
        if running { "HOST: RUNNING" } else { "HOST: STOPPED" }
    } else {
        s.msg.as_str()
    };
    text(c, status, 60, 462, 3, c.dim);
    button(c, START, "START", !running);
    button(c, STOP, "STOP", false);
}

unsafe fn paint_win(hwnd: HWND) {
    let s = ST.with(|s| s.borrow().clone());
    let pal = palette(if s.cfg.comps[4] { &s.cfg.theme } else { "black" });
    paint(hwnd, s.scale, &pal, |c, _, _| draw(c, &s));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_win(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            click(
                hwnd,
                (lp & 0xffff) as u16 as i16 as i32,
                ((lp >> 16) & 0xffff) as u16 as i16 as i32,
            );
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    unsafe {
        let boot = Config::load();
        clouddesk_ui::glass::set(palette(if boot.comps[4] { &boot.theme } else { "black" }).glass);
        let class = wide(CLASS);
        let existing = FindWindowW(class.as_ptr(), ptr::null());
        if !existing.is_null() {
            SetForegroundWindow(existing);
            return;
        }
        let scale = init_dpi();
        ST.with(|s| {
            let mut s = s.borrow_mut();
            s.cfg = Config::load();
            s.scale = scale;
        });
        if !register(&class, Some(wndproc)) {
            std::process::exit(1);
        }
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 880 * scale / 100,
            bottom: 520 * scale / 100,
        };
        AdjustWindowRectEx(&mut rc, style, 0, 0);
        let ww = rc.right - rc.left;
        let wh = rc.bottom - rc.top;
        let x = (GetSystemMetrics(SM_CXSCREEN) - ww) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - wh) / 2;
        let hwnd = create(&class, 0, style, x, y, ww, wh);
        if hwnd.is_null() {
            std::process::exit(1);
        }
        SetWindowTextW(hwnd, wide("CloudDesk settings").as_ptr());
        ShowWindow(hwnd, SW_SHOW);
        thread::spawn(|| {
            let _ = wall::ensure();
        });
        pump();
    }
}
