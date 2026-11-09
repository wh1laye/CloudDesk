#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::env;
use std::ptr;
use std::thread;

use clouddesk_ui::config::{file_name, Config, THEMES};
use clouddesk_ui::gfx::{button, dot, fill, paint, palette, pill, text, title, toggle, Ctx, R};
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
    "Desktop",
    "Island",
    "Search",
    "Visualizer",
    "Colors",
    "Music",
    "Widgets",
    "Lock screen",
];
const IMPL: [bool; 8] = [true, true, true, true, true, false, true, true];
const TABS: [&str; 4] = ["Components", "Look", "Desktop", "Wallpaper"];
const START: R = (16, 440, 168, 36);
const STOP: R = (16, 484, 168, 36);
const PREV: R = (232, 130, 140, 44);
const NEXT: R = (388, 130, 140, 44);
const RESTORE: R = (232, 190, 220, 44);
const FOLDER: R = (468, 190, 180, 44);
const VIS_BTN: R = (690, 192, 150, 36);

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
    Spaces,
    Taskbar,
    Wall(i32),
    Restore,
    Folder,
    Start,
    Stop,
}

fn row(i: usize) -> R {
    (232, 84 + i as i32 * 50, 608, 44)
}

fn card(i: usize) -> R {
    (232 + (i % 4) as i32 * 152, 84 + (i / 4) as i32 * 132, 148, 92)
}

fn tab_rect(i: usize) -> R {
    (16, 96 + i as i32 * 48, 168, 40)
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
    if let Some(i) = (0..TABS.len()).find(|&i| hit(tab_rect(i), x, y)) {
        return Act::Tab(i as u8);
    }
    match tab {
        0 => {
            if let Some(i) = (0..8usize).find(|&i| hit(row(i), x, y)) {
                return Act::Toggle(i);
            }
        }
        1 => {
            if let Some(i) = (0..THEMES.len()).find(|&i| {
                let r = card(i);
                hit((r.0, r.1, r.2, r.3 + 28), x, y)
            }) {
                return Act::Theme(i);
            }
        }
        2 => {
            if hit(row(0), x, y) {
                return Act::Taskbar;
            }
            if hit(row(1), x, y) {
                return Act::Spaces;
            }
            if hit(row(2), x, y) {
                return Act::Mode;
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
        Act::Toggle(_) => set_msg("Not built yet"),
        Act::Taskbar => {
            let cfg = ST.with(|s| {
                let mut s = s.borrow_mut();
                s.cfg.hide_taskbar = !s.cfg.hide_taskbar;
                s.cfg.clone()
            });
            persist(&cfg);
        }
        Act::Spaces => {
            let cfg = ST.with(|s| {
                let mut s = s.borrow_mut();
                s.cfg.spaces = !s.cfg.spaces;
                s.cfg.clone()
            });
            persist(&cfg);
        }
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

fn cap(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

unsafe fn switch_row(c: &Ctx, i: usize, label: &str, on: bool, live: bool) {
    let r = row(i);
    text(c, label, r.0 + 8, r.1 + 8, 4, if live { c.fg } else { c.dim });
    toggle(c, r.0 + r.2 - 72, r.1 + 6, live && on);
    pill(c, r.0, r.1 + r.3 + 2, r.2, 2, c.dark);
}

unsafe fn draw(c: &Ctx, s: &St) {
    dot(c, c.acc, 36, 38, 6);
    text(c, "CloudDesk", 52, 24, 4, c.fg);
    for (i, n) in TABS.iter().enumerate() {
        let r = tab_rect(i);
        let on = s.tab as usize == i;
        if on {
            pill(c, r.0, r.1, r.2, r.3, c.dark);
        }
        text(c, n, r.0 + 18, r.1 + 8, 3, if on { c.fg } else { c.dim });
    }
    match s.tab {
        0 => {
            title(c, "Components", 232, 22, 5);
            for (i, name) in NAMES.iter().enumerate() {
                switch_row(c, i, name, s.cfg.comps[i], IMPL[i]);
            }
        }
        1 => {
            title(c, "Look", 232, 22, 5);
            for (i, n) in THEMES.iter().enumerate() {
                let r = card(i);
                let p = palette(n);
                let sel = s.cfg.theme == *n;
                fill(c, r.0, r.1, r.2, r.3, 26, if sel { p.acc } else { p.dark });
                fill(c, r.0 + 4, r.1 + 4, r.2 - 8, r.3 - 8, 22, p.bg);
                fill(c, r.0 + 18, r.1 + 22, 64, 8, 8, p.fg);
                fill(c, r.0 + 18, r.1 + 40, 40, 6, 6, p.dim);
                fill(c, r.0 + r.2 - 44, r.1 + r.3 - 38, 24, 24, 24, p.acc);
                text(c, &cap(n), r.0 + 6, r.1 + r.3 + 4, 3, if sel { c.fg } else { c.dim });
            }
        }
        2 => {
            title(c, "Desktop", 232, 22, 5);
            switch_row(c, 0, "Hide Windows taskbar", s.cfg.hide_taskbar, true);
            switch_row(c, 1, "Virtual desktops (Win + 1..9)", s.cfg.spaces, true);
            let r = row(2);
            text(c, "Visualizer style", r.0 + 8, r.1 + 8, 4, c.fg);
            button(
                c,
                (VIS_BTN.0, r.1 + 4, VIS_BTN.2, VIS_BTN.3),
                if s.cfg.contour { "Contour" } else { "Dots" },
                false,
            );
        }
        _ => {
            title(c, "Wallpaper", 232, 22, 5);
            let l = wall::list();
            let name = l
                .get(s.cfg.wall_index)
                .map(|p| file_name(p))
                .unwrap_or_else(|| "None".to_string());
            let line = format!(
                "{} / {}   {}",
                s.cfg.wall_index.min(l.len().saturating_sub(1)) + 1,
                l.len(),
                name
            );
            text(c, &line, 232, 84, 4, c.fg);
            button(c, PREV, "Previous", false);
            button(c, NEXT, "Next", false);
            button(c, RESTORE, "Restore original", false);
            button(c, FOLDER, "Open folder", false);
            text(c, "Put BMP, JPG or PNG files into the folder.", 232, 262, 3, c.dim);
        }
    }
    let running = !host_window().is_null();
    let status = if s.msg.is_empty() {
        if running {
            "Shell is running"
        } else {
            "Shell is stopped"
        }
    } else {
        s.msg.as_str()
    };
    text(c, status, 232, 490, 3, c.dim);
    button(c, START, "Start", !running);
    button(c, STOP, "Stop", false);
}

unsafe fn paint_win(hwnd: HWND) {
    let s = ST.with(|s| s.borrow().clone());
    let pal = palette(&s.cfg.theme);
    clouddesk_ui::glass::chrome(hwnd, pal.bg, pal.fg);
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
        clouddesk_ui::glass::set(palette(&boot.theme).glass);
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
            bottom: 540 * scale / 100,
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
