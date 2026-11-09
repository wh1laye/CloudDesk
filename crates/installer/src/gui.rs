use std::env;
use std::fs;
use std::path::PathBuf;
use std::ptr;
use std::sync::{Mutex, MutexGuard};
use std::thread;

use clouddesk_ui::gfx::{button, dot, paint, palette, pill, text, title, toggle, tw, Ctx, R};
use clouddesk_ui::{create, glass, init_dpi, open, pump, register, wide};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::System::Console::FreeConsole;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DefWindowProcW, DestroyWindow, GetSystemMetrics, PostMessageW,
    PostQuitMessage, SetTimer, SetWindowTextW, ShowWindow, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WM_TIMER, WS_CAPTION,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

use crate::{install, local_appdata, uninstall, Log, Mode, Opts, APP, MARKER};

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
const KEYS: [&str; 8] = [
    "desktop",
    "island",
    "search",
    "visualizer",
    "themes",
    "music",
    "widgets",
    "lockscreen",
];

const BACK: R = (540, 450, 140, 44);
const NEXT: R = (700, 450, 140, 44);
const AUTO: R = (50, 404, 360, 56);

#[derive(Clone)]
struct Gui {
    page: u8,
    sw: [bool; 8],
    auto: bool,
    tick: i32,
    scale: i32,
    update: bool,
    removed: bool,
    result: Option<Result<(), String>>,
}

static GUI: Mutex<Gui> = Mutex::new(Gui {
    page: 0,
    sw: [true; 8],
    auto: true,
    tick: 0,
    scale: 100,
    update: false,
    removed: false,
    result: None,
});

fn st() -> MutexGuard<'static, Gui> {
    GUI.lock().unwrap_or_else(|e| e.into_inner())
}

fn sw_rect(i: usize) -> R {
    let i = i as i32;
    (50 + (i / 4) * 400, 134 + (i % 4) * 64, 360, 56)
}

fn hit(r: R, x: i32, y: i32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn wrap(s: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
    chars
        .chunks(n)
        .take(4)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

fn switch_row(c: &Ctx, r: R, label: &str, on: bool) {
    unsafe {
        text(c, label, r.0 + 10, r.1 + 14, 4, c.fg);
        toggle(c, r.0 + r.2 - 74, r.1 + 13, on);
    }
}

unsafe fn draw(c: &Ctx, g: &Gui) {
    dot(c, c.acc, 46, 38, 5);
    text(c, "CloudDesk", 62, 24, 3, c.dim);
    let step = format!("Step {} of 3", (g.page + 1).min(3));
    text(c, &step, 820 - tw(&step, 3), 24, 3, c.dim);
    match g.page {
        0 => {
            title(c, if g.update { "Update" } else { "Welcome" }, 60, 96, 9);
            text(
                c,
                "Installs for the current user. No administrator rights.",
                60,
                230,
                4,
                c.fg,
            );
            text(c, "Every change is backed up first.", 60, 270, 4, c.dim);
            text(
                c,
                "Remove it any time from the same installer.",
                60,
                310,
                4,
                c.dim,
            );
            if g.update {
                button(c, BACK, "Remove", false);
            }
            button(c, NEXT, "Continue", true);
        }
        1 => {
            title(c, "Components", 60, 62, 6);
            let n = g.sw.iter().filter(|v| **v).count();
            let cnt = format!("{n} of 8");
            text(c, &cnt, 820 - tw(&cnt, 4), 70, 4, c.dim);
            for (i, name) in NAMES.iter().enumerate() {
                switch_row(c, sw_rect(i), name, g.sw[i]);
            }
            switch_row(c, AUTO, "Start with Windows", g.auto);
            button(c, BACK, "Back", false);
            button(c, NEXT, if g.update { "Update" } else { "Install" }, true);
        }
        2 => {
            title(
                c,
                if g.removed { "Removing" } else { "Installing" },
                60,
                96,
                9,
            );
            pill(c, 60, 270, 760, 8, c.dark);
            let span = 560;
            let t = (g.tick * 14) % (span * 2);
            let off = if t > span { span * 2 - t } else { t };
            pill(c, 60 + off, 270, 200, 8, c.acc);
            text(
                c,
                "Working. Please do not close this window.",
                60,
                310,
                4,
                c.dim,
            );
        }
        _ => {
            let ok = matches!(g.result, Some(Ok(())));
            let head = match (ok, g.removed) {
                (true, true) => "Removed",
                (true, false) => "Done",
                _ => "Failed",
            };
            title(c, head, 60, 96, 9);
            if let Some(Err(e)) = &g.result {
                for (i, line) in wrap(e, 70).iter().enumerate() {
                    text(c, line, 60, 230 + i as i32 * 34, 4, c.dim);
                }
            } else if g.removed {
                text(
                    c,
                    "CloudDesk was removed from this account.",
                    60,
                    230,
                    4,
                    c.fg,
                );
            } else {
                text(c, "CloudDesk is installed.", 60, 230, 4, c.fg);
                text(
                    c,
                    "Open Settings from the menu bar to adjust it.",
                    60,
                    270,
                    4,
                    c.dim,
                );
            }
            if ok && !g.removed {
                button(c, BACK, "Close", false);
                button(c, NEXT, "Launch", true);
            } else {
                button(c, NEXT, "Close", true);
            }
        }
    }
}

unsafe fn paint_win(hwnd: HWND) {
    let g = st().clone();
    let pal = palette("black");
    glass::chrome(hwnd, pal.bg, pal.fg);
    paint(hwnd, g.scale, &pal, |c, _, _| draw(c, &g));
}

fn write_config(sw: &[bool; 8]) -> Result<(), String> {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is not set")?;
    let dir = base.join(APP);
    let file = dir.join("config.toml");
    if file.exists() {
        return Ok(());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("config dir: {e}"))?;
    let mut body = String::from("[components]\n");
    for (k, v) in KEYS.iter().zip(sw.iter()) {
        body.push_str(&format!("{k} = {v}\n"));
    }
    fs::write(&file, body).map_err(|e| format!("config: {e}"))
}

fn opts(mode: Mode, auto: bool) -> Opts {
    Opts {
        mode,
        full: false,
        silent: true,
        dir: None,
        payload: None,
        autostart: auto,
        purge: false,
        skip_checks: false,
        log: None,
    }
}

unsafe fn begin(hwnd: HWND, removing: bool) {
    let (sw, auto, update) = {
        let mut g = st();
        g.page = 2;
        g.tick = 0;
        g.removed = removing;
        (g.sw, g.auto, g.update)
    };
    let h = hwnd as usize;
    thread::spawn(move || {
        let log = Log::new(None);
        let r = if removing {
            uninstall(&opts(Mode::Uninstall, false), &log)
        } else {
            let mode = if update { Mode::Update } else { Mode::Install };
            let r = install(&opts(mode, auto), &log, update);
            if r.is_ok() {
                if let Err(e) = write_config(&sw) {
                    log.say(&format!("warning: {e}"));
                }
            }
            r
        };
        st().result = Some(r);
        PostMessageW(h as HWND, WM_APP, 0, 0);
    });
}

fn launch() {
    if let Ok(base) = local_appdata() {
        let exe = base.join("Programs").join(APP).join("clouddesk.exe");
        if exe.is_file() {
            open(&exe.to_string_lossy());
        }
    }
}

unsafe fn click(hwnd: HWND, px: i32, py: i32) {
    let mut start: Option<bool> = None;
    let mut close = false;
    let mut run = false;
    {
        let mut g = st();
        let x = px * 100 / g.scale;
        let y = py * 100 / g.scale;
        match g.page {
            0 => {
                if hit(NEXT, x, y) {
                    g.page = 1;
                } else if g.update && hit(BACK, x, y) {
                    start = Some(true);
                }
            }
            1 => {
                if hit(BACK, x, y) {
                    g.page = 0;
                } else if hit(NEXT, x, y) {
                    start = Some(false);
                } else if let Some(i) = (0..8).find(|&i| hit(sw_rect(i), x, y)) {
                    g.sw[i] = !g.sw[i];
                } else if hit(AUTO, x, y) {
                    g.auto = !g.auto;
                }
            }
            3 => {
                let ok = matches!(g.result, Some(Ok(()))) && !g.removed;
                if hit(NEXT, x, y) {
                    close = true;
                    run = ok;
                } else if ok && hit(BACK, x, y) {
                    close = true;
                }
            }
            _ => {}
        }
    }
    if let Some(removing) = start {
        begin(hwnd, removing);
    }
    if run {
        launch();
    }
    if close {
        DestroyWindow(hwnd);
    }
    InvalidateRect(hwnd, ptr::null(), 0);
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
        WM_TIMER => {
            let busy = {
                let mut g = st();
                if g.page == 2 {
                    g.tick += 1;
                }
                g.page == 2
            };
            if busy {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_APP => {
            st().page = 3;
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_CLOSE => {
            let busy = st().page == 2;
            if !busy {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run() -> Result<(), String> {
    let installed = local_appdata()
        .map(|p| p.join("Programs").join(APP).join(MARKER).is_file())
        .unwrap_or(false);
    unsafe {
        FreeConsole();
        let scale = init_dpi();
        {
            let mut g = st();
            g.scale = scale;
            g.update = installed;
        }
        let class = wide("CloudDeskSetup");
        if !register(&class, Some(wndproc)) {
            return Err("cannot register window class".into());
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
            return Err("cannot create window".into());
        }
        SetWindowTextW(hwnd, wide("CloudDesk setup").as_ptr());
        ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, 1, 90, None);
        pump();
    }
    Ok(())
}
