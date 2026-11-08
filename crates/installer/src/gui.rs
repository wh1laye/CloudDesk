use std::env;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Mutex, MutexGuard};
use std::thread;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, Ellipse, EndPaint, FillRect, GetStockObject, InvalidateRect, RoundRect,
    SelectObject, HBRUSH, HDC, NULL_BRUSH, NULL_PEN, PAINTSTRUCT, SRCCOPY,
};
use windows_sys::Win32::System::Console::FreeConsole;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetSystemMetrics, LoadCursorW, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetProcessDPIAware, SetTimer, ShowWindow, TranslateMessage, IDC_ARROW, MSG,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND,
    WM_LBUTTONDOWN, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED,
    WS_SYSMENU,
};

use crate::font::glyph;
use crate::{install, local_appdata, Log, Mode, Opts, APP, MARKER};

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

type R = (i32, i32, i32, i32);
const BACK: R = (540, 450, 140, 44);
const NEXT: R = (700, 450, 140, 44);
const AUTO: R = (50, 404, 360, 56);
const ROWS: i32 = 5;

#[derive(Clone)]
struct Gui {
    page: u8,
    sw: [bool; 8],
    auto: bool,
    tick: i32,
    scale: i32,
    update: bool,
    result: Option<Result<(), String>>,
}

static GUI: Mutex<Gui> = Mutex::new(Gui {
    page: 0,
    sw: [true; 8],
    auto: true,
    tick: 0,
    scale: 100,
    update: false,
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

struct Ctx {
    dc: HDC,
    sc: i32,
    bg: HBRUSH,
    white: HBRUSH,
    dim: HBRUSH,
    red: HBRUSH,
    dark: HBRUSH,
}

impl Ctx {
    fn s(&self, v: i32) -> i32 {
        v * self.sc / 100
    }
}

fn tw(s: &str, cell: i32) -> i32 {
    s.len() as i32 * 6 * cell
}

unsafe fn dot(c: &Ctx, b: HBRUSH, cx: i32, cy: i32, r: i32) {
    SelectObject(c.dc, b);
    Ellipse(c.dc, c.s(cx - r), c.s(cy - r), c.s(cx + r), c.s(cy + r));
}

unsafe fn text(c: &Ctx, s: &str, x: i32, y: i32, cell: i32, b: HBRUSH) {
    let r = (cell * 4 / 10).max(1);
    for (i, ch) in s.chars().enumerate() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    let cx = x + i as i32 * 6 * cell + col * cell + cell / 2;
                    let cy = y + row as i32 * cell + cell / 2;
                    dot(c, b, cx, cy, r);
                }
            }
        }
    }
}

unsafe fn title(c: &Ctx, word: &str, x: i32, y: i32, cell: i32) {
    text(c, word, x, y, cell, c.white);
    let end = x + tw(word, cell);
    dot(c, c.red, end - cell / 2, y + 6 * cell + cell / 2, (cell * 4 / 10).max(2));
}

unsafe fn pill(c: &Ctx, x: i32, y: i32, w: i32, h: i32, b: HBRUSH) {
    SelectObject(c.dc, b);
    RoundRect(c.dc, c.s(x), c.s(y), c.s(x + w), c.s(y + h), c.s(h), c.s(h));
}

unsafe fn toggle(c: &Ctx, x: i32, y: i32, on: bool) {
    pill(c, x, y, 64, 30, if on { c.red } else { c.dark });
    let kx = if on { x + 48 } else { x + 16 };
    dot(c, if on { c.white } else { c.dim }, kx, y + 15, 11);
}

unsafe fn button(c: &Ctx, r: R, label: &str, primary: bool) {
    pill(c, r.0, r.1, r.2, r.3, if primary { c.red } else { c.dark });
    text(c, label, r.0 + (r.2 - tw(label, 3)) / 2, r.1 + (r.3 - 21) / 2, 3, c.white);
}

unsafe fn dotbar(c: &Ctx, x: i32, y: i32, cols: i32, cell: i32, lit: i32, b: HBRUSH) {
    for col in 0..cols {
        let h = ((col * 5 + 2) % ROWS) + 1;
        for row in 0..ROWS {
            let on = col < lit && (ROWS - 1 - row) < h;
            let br = if on { b } else { c.dark };
            dot(c, br, x + col * cell + cell / 2, y + row * cell + cell / 2, (cell * 4 / 10).max(1));
        }
    }
}

unsafe fn logo(c: &Ctx, cx: i32, cy: i32) {
    for i in 0..56 {
        let a = i as f64 * std::f64::consts::TAU / 56.0;
        dot(c, c.dim, cx + (a.cos() * 95.0) as i32, cy + (a.sin() * 95.0) as i32, 2);
    }
    text(c, "F", cx - 25, cy - 35, 10, c.white);
    dot(c, c.red, cx + 22, cy + 30, 8);
}

fn wrap(s: &str, n: usize) -> Vec<String> {
    let up: String = s
        .to_uppercase()
        .chars()
        .map(|ch| if glyph(ch) != [0u8; 7] { ch } else { ' ' })
        .collect();
    up.as_bytes()
        .chunks(n)
        .take(4)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect()
}

unsafe fn draw(c: &Ctx, g: &Gui) {
    dot(c, c.red, 46, 38, 4);
    text(c, "CLOUDDESK", 60, 28, 3, c.dim);
    let step = format!("STEP {}/4", g.page + 1);
    text(c, &step, 820 - tw(&step, 3), 28, 3, c.dim);
    match g.page {
        0 => {
            title(c, if g.update { "UPDATE" } else { "SWITCH" }, 60, 110, 12);
            text(c, "PER-USER. NO ADMIN.", 60, 250, 3, c.white);
            text(c, "EVERY CHANGE IS BACKED UP.", 60, 285, 3, c.dim);
            text(c, "REMOVE ANYTIME.", 60, 320, 3, c.dim);
            logo(c, 760, 250);
            button(c, NEXT, "NEXT", true);
        }
        1 => {
            title(c, "YOUR SWITCHES", 60, 75, 6);
            let n = g.sw.iter().filter(|v| **v).count();
            let cnt = format!("{n}/8");
            text(c, &cnt, 820 - tw(&cnt, 6), 75, 6, c.white);
            for (i, name) in NAMES.iter().enumerate() {
                let r = sw_rect(i);
                text(c, name, r.0 + 10, r.1 + 16, 3, c.white);
                toggle(c, r.0 + 250, r.1 + 12, g.sw[i]);
            }
            text(c, "AUTOSTART", AUTO.0 + 10, AUTO.1 + 16, 3, c.white);
            toggle(c, AUTO.0 + 250, AUTO.1 + 12, g.auto);
            button(c, BACK, "BACK", false);
            button(c, NEXT, if g.update { "UPDATE" } else { "INSTALL" }, true);
        }
        2 => {
            title(c, "INSTALLING", 60, 110, 10);
            dotbar(c, 60, 250, 52, 10, g.tick % 53, c.red);
            text(c, "WORKING. DO NOT CLOSE THIS WINDOW.", 60, 330, 3, c.dim);
        }
        _ => {
            let ok = matches!(g.result, Some(Ok(())));
            title(c, if ok { "DONE" } else { "FAILED" }, 60, 110, 12);
            if let Some(Err(e)) = &g.result {
                for (i, line) in wrap(e, 40).iter().enumerate() {
                    text(c, line, 60, 250 + i as i32 * 32, 3, c.dim);
                }
            } else {
                text(c, "ALL SWITCHES ARE SET.", 60, 250, 3, c.white);
            }
            button(c, NEXT, "CLOSE", true);
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    let g = st().clone();
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old = SelectObject(mem, bmp);
    let c = Ctx {
        dc: mem,
        sc: g.scale,
        bg: CreateSolidBrush(rgb(10, 10, 10)),
        white: CreateSolidBrush(rgb(240, 240, 240)),
        dim: CreateSolidBrush(rgb(130, 130, 130)),
        red: CreateSolidBrush(rgb(226, 38, 45)),
        dark: CreateSolidBrush(rgb(44, 44, 44)),
    };
    SelectObject(mem, GetStockObject(NULL_PEN));
    FillRect(mem, &rc, c.bg);
    draw(&c, &g);
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, GetStockObject(NULL_BRUSH));
    SelectObject(mem, old);
    for b in [c.bg, c.white, c.dim, c.red, c.dark] {
        DeleteObject(b);
    }
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
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

unsafe fn begin(hwnd: HWND) {
    let (sw, auto, update) = {
        let mut g = st();
        g.page = 2;
        g.tick = 0;
        (g.sw, g.auto, g.update)
    };
    let h = hwnd as usize;
    thread::spawn(move || {
        let o = Opts {
            mode: if update { Mode::Update } else { Mode::Install },
            silent: true,
            dir: None,
            payload: None,
            autostart: auto,
            purge: false,
            skip_checks: false,
            log: None,
        };
        let log = Log::new(None);
        let r = install(&o, &log, update);
        if r.is_ok() {
            if let Err(e) = write_config(&sw) {
                log.say(&format!("warning: {e}"));
            }
        }
        st().result = Some(r);
        PostMessageW(h as HWND, WM_APP, 0, 0);
    });
}

unsafe fn click(hwnd: HWND, px: i32, py: i32) {
    let mut start = false;
    let mut close = false;
    {
        let mut g = st();
        let x = px * 100 / g.scale;
        let y = py * 100 / g.scale;
        match g.page {
            0 => {
                if hit(NEXT, x, y) {
                    g.page = 1;
                }
            }
            1 => {
                if hit(BACK, x, y) {
                    g.page = 0;
                } else if hit(NEXT, x, y) {
                    start = true;
                } else if let Some(i) = (0..8).find(|&i| hit(sw_rect(i), x, y)) {
                    g.sw[i] = !g.sw[i];
                } else if hit(AUTO, x, y) {
                    g.auto = !g.auto;
                }
            }
            3 => {
                close = hit(NEXT, x, y);
            }
            _ => {}
        }
    }
    if start {
        begin(hwnd);
    }
    if close {
        DestroyWindow(hwnd);
    }
    InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
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
        SetProcessDPIAware();
        let scale = (GetDpiForSystem() as i32 * 100 / 96).max(100);
        {
            let mut g = st();
            g.scale = scale;
            g.update = installed;
        }
        let class = wide("CloudDeskSetup");
        let title = wide("CloudDesk setup");
        let inst = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: ptr::null_mut(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
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
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            ww,
            wh,
            ptr::null_mut(),
            ptr::null_mut(),
            inst,
            ptr::null(),
        );
        if hwnd.is_null() {
            return Err("cannot create window".into());
        }
        ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, 1, 90, None);
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
