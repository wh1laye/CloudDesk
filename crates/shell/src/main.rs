#![windows_subsystem = "windows"]

use std::cell::{Cell, RefCell};
use std::env;
use std::mem::{size_of, zeroed};
use std::ptr;
use std::thread;

use clouddesk_ui::config::Config;
use clouddesk_ui::gfx::{dot, paint, palette, pill, text, tw, Palette};
use clouddesk_ui::{create, init_dpi, now, open, pump, register, wall, wide, HOST_CLASS};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_SPACE,
};
use windows_sys::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABN_FULLSCREENAPP, APPBARDATA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetSystemMetrics, PostQuitMessage, SetTimer, SetWindowPos,
    HWND_BOTTOM, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_HOTKEY, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_PAINT, WM_TIMER, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetLayeredWindowAttributes, LWA_COLORKEY, WM_MOUSEMOVE, WM_RBUTTONDOWN, WS_EX_LAYERED,
};

mod dock;
mod hook;
mod island;
mod lockscreen;
mod visualizer;
mod search;
mod widgets;

const BAR_H: i32 = 36;
const DOCK_H: i32 = dock::STRIP_H;
const WID_W: i32 = 300;
const WID_H: i32 = 150;
const WM_RELOAD: u32 = WM_APP + 1;
const WM_BAR: u32 = WM_APP + 2;
const BAR_CLASS: &str = "CloudDeskBar";
const DOCK_CLASS: &str = "CloudDeskDock";
const WID_CLASS: &str = "CloudDeskWidget";
const MA_NOACTIVATE: LRESULT = 3;

struct Host {
    cfg: Config,
    bar: usize,
    dock: usize,
    wid: usize,
    scale: i32,
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host {
        cfg: Config::new(),
        bar: 0,
        dock: 0,
        wid: 0,
        scale: 100,
    });
}

thread_local! {
    static CTL: Cell<usize> = const { Cell::new(0) };
    static HOTKEY: Cell<bool> = const { Cell::new(false) };
}

unsafe fn sync_hotkey(on: bool) {
    let ctl = CTL.with(|c| c.get()) as HWND;
    if ctl.is_null() {
        return;
    }
    hook::enable(on, ctl);
    let registered = HOTKEY.with(|h| h.get());
    if on && !registered {
        let ok = RegisterHotKey(ctl, 1, MOD_ALT | MOD_NOREPEAT, VK_SPACE as u32) != 0;
        HOTKEY.with(|h| h.set(ok));
    } else if !on && registered {
        UnregisterHotKey(ctl, 1);
        HOTKEY.with(|h| h.set(false));
    }
}

fn snap() -> (Config, usize, usize, usize, i32) {
    HOST.with(|h| {
        let h = h.borrow();
        (h.cfg.clone(), h.bar, h.dock, h.wid, h.scale)
    })
}

fn pal(c: &Config) -> Palette {
    palette(if c.comps[4] { &c.theme } else { "black" })
}

fn settings_exe() -> String {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("clouddesk-settings.exe")))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn lo(lp: LPARAM) -> i32 {
    (lp & 0xffff) as u16 as i16 as i32
}

unsafe fn screen() -> (i32, i32) {
    (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
}

unsafe fn appbar_add(hwnd: HWND, edge: u32, h: i32) {
    let (sw, sh) = screen();
    let mut abd: APPBARDATA = zeroed();
    abd.cbSize = size_of::<APPBARDATA>() as u32;
    abd.hWnd = hwnd;
    abd.uCallbackMessage = WM_BAR;
    SHAppBarMessage(ABM_NEW, &mut abd);
    abd.uEdge = edge;
    abd.rc = RECT {
        left: 0,
        top: 0,
        right: sw,
        bottom: sh,
    };
    SHAppBarMessage(ABM_QUERYPOS, &mut abd);
    if edge == ABE_TOP {
        abd.rc.bottom = abd.rc.top + h;
    } else {
        abd.rc.top = abd.rc.bottom - h;
    }
    SHAppBarMessage(ABM_SETPOS, &mut abd);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        abd.rc.left,
        abd.rc.top,
        abd.rc.right - abd.rc.left,
        h,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe fn appbar_remove(hwnd: HWND) {
    let mut abd: APPBARDATA = zeroed();
    abd.cbSize = size_of::<APPBARDATA>() as u32;
    abd.hWnd = hwnd;
    SHAppBarMessage(ABM_REMOVE, &mut abd);
}

unsafe fn fullscreen(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    if wp == ABN_FULLSCREENAPP as usize {
        let z = if lp != 0 { HWND_BOTTOM } else { HWND_TOPMOST };
        SetWindowPos(hwnd, z, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

unsafe fn arm_timer(hwnd: HWND, first: bool) {
    let ms = if first {
        (60 - now().sec) * 1000 + 100
    } else {
        60000
    };
    SetTimer(hwnd, 1, ms, None);
}

unsafe fn make_bar(scale: i32) -> usize {
    let h = create(
        &wide(BAR_CLASS),
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
        WS_POPUP,
        0,
        0,
        screen().0,
        BAR_H * scale / 100,
    );
    if h.is_null() {
        return 0;
    }
    appbar_add(h, ABE_TOP, BAR_H * scale / 100);
    arm_timer(h, true);
    h as usize
}

unsafe fn make_dock(scale: i32) -> usize {
    let h = create(
        &wide(DOCK_CLASS),
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        WS_POPUP,
        0,
        0,
        screen().0,
        DOCK_H * scale / 100,
    );
    if h.is_null() {
        return 0;
    }
    SetLayeredWindowAttributes(h, 0x00FF00FF, 255, LWA_COLORKEY);
    appbar_add(h, ABE_BOTTOM, DOCK_H * scale / 100);
    h as usize
}

unsafe fn make_wid(scale: i32) -> usize {
    let w = WID_W * scale / 100;
    let h = WID_H * scale / 100;
    let win = create(
        &wide(WID_CLASS),
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        WS_POPUP,
        0,
        0,
        w,
        h,
    );
    if win.is_null() {
        return 0;
    }
    let r = 28 * scale / 100;
    SetWindowRgn(win, CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r), 0);
    let x = screen().0 - w - 24 * scale / 100;
    let y = (BAR_H + 24) * scale / 100;
    SetWindowPos(win, HWND_BOTTOM, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    arm_timer(win, true);
    win as usize
}

unsafe fn sync() {
    let (cfg, bar, dock, wid, scale) = snap();
    let (mut nb, mut nd, mut nw) = (bar, dock, wid);
    if cfg.comps[0] && bar == 0 {
        nb = make_bar(scale);
        nd = make_dock(scale);
    } else if !cfg.comps[0] && bar != 0 {
        DestroyWindow(bar as HWND);
        DestroyWindow(dock as HWND);
        nb = 0;
        nd = 0;
    }
    if cfg.comps[6] && wid == 0 {
        nw = make_wid(scale);
    } else if !cfg.comps[6] && wid != 0 {
        DestroyWindow(wid as HWND);
        nw = 0;
    }
    HOST.with(|h| {
        let mut h = h.borrow_mut();
        h.bar = nb;
        h.dock = nd;
        h.wid = nw;
    });
    for w in [nb, nd, nw] {
        if w != 0 {
            InvalidateRect(w as HWND, ptr::null(), 0);
        }
    }
    if nd != 0 {
        dock::place(nd as HWND, scale, cfg.dock.len());
    }
    let ctl = CTL.with(|c| c.get()) as HWND;
    island::sync(cfg.comps[1], pal(&cfg), scale, ctl);
    visualizer::set_contour(cfg.contour);
    visualizer::sync(cfg.comps[3], pal(&cfg), scale, ctl);
    visualizer::refresh();
    lockscreen::hotkey(cfg.comps[7], ctl);
    sync_hotkey(cfg.comps[2]);
    widgets::sync(cfg.comps[6]);
}

unsafe fn paint_bar(hwnd: HWND) {
    let (cfg, _, _, _, scale) = snap();
    paint(hwnd, scale, &pal(&cfg), |c, lw, _| {
        dot(c, c.acc, 18, 18, 4);
        text(c, "CLOUDDESK", 32, 7, 3, c.fg);
        let n = wall::list().len();
        let label = if n == 0 {
            "NO WALLPAPERS".to_string()
        } else {
            format!("WALL {}/{}", cfg.wall_index.min(n - 1) + 1, n)
        };
        text(c, &label, (lw - tw(&label, 3)) / 2, 7, 3, c.dim);
        let t = now();
        let stamp = format!("{} {}", t.date, t.hm);
        text(c, &stamp, lw - 18 - tw(&stamp, 3), 7, 3, c.fg);
    });
}

unsafe fn bar_click(px: i32) {
    let scale = snap().4;
    let lw = screen().0 * 100 / scale;
    let x = px * 100 / scale;
    if x < 200 {
        open(&settings_exe());
    } else if x * 10 >= lw * 3 && x * 10 <= lw * 7 {
        let mut c = Config::load();
        if wall::step(&mut c, 1).is_ok() {
            let bar = HOST.with(|h| {
                let mut h = h.borrow_mut();
                h.cfg = c;
                h.bar
            });
            if bar != 0 {
                InvalidateRect(bar as HWND, ptr::null(), 0);
            }
        }
    }
}

unsafe fn paint_dock(hwnd: HWND) {
    let (cfg, _, _, _, scale) = snap();
    dock::draw(hwnd, scale, &pal(&cfg), &cfg.dock);
}

unsafe fn dock_click(hwnd: HWND, px: i32) {
    let (cfg, _, _, _, scale) = snap();
    if let Some(i) = dock::hit(hwnd, scale, cfg.dock.len(), px) {
        if let Some((_, target)) = cfg.dock.get(i) {
            open(target);
        }
    }
}

unsafe fn dock_unpin(hwnd: HWND, px: i32) {
    let (cfg, _, _, _, scale) = snap();
    if let Some(i) = dock::hit(hwnd, scale, cfg.dock.len(), px) {
        dock::unpin(i);
    }
}

unsafe fn paint_wid(hwnd: HWND) {
    let (cfg, _, _, _, scale) = snap();
    paint(hwnd, scale, &pal(&cfg), |c, lw, lh| {
        pill(c, 0, 0, lw, lh, c.dark);
        dot(c, c.acc, 28, 28, 4);
        text(c, "CLOCK", 42, 18, 3, c.dim);
        let t = now();
        text(c, &t.hm, 24, 50, 8, c.fg);
        text(c, &t.date, 24, 112, 3, c.dim);
    });
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_bar(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_LBUTTONDOWN => {
            bar_click(lo(lp));
            0
        }
        WM_TIMER => {
            arm_timer(hwnd, false);
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_BAR => {
            fullscreen(hwnd, wp, lp);
            0
        }
        WM_DESTROY => {
            appbar_remove(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn dock_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_dock(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_LBUTTONDOWN => {
            dock_click(hwnd, lo(lp));
            0
        }
        WM_RBUTTONDOWN => {
            dock_unpin(hwnd, lo(lp));
            0
        }
        WM_MOUSEMOVE => {
            dock::on_move(hwnd, snap().4, lo(lp));
            0
        }
        0x02A3 => {
            dock::on_leave(hwnd);
            0
        }
        WM_BAR => {
            fullscreen(hwnd, wp, lp);
            0
        }
        WM_DESTROY => {
            appbar_remove(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn wid_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_wid(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_TIMER => {
            arm_timer(hwnd, false);
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RELOAD => {
            let c = Config::load();
            HOST.with(|h| h.borrow_mut().cfg = c);
            sync();
            0
        }
        island::WM_MEDIA => {
            island::refresh();
            0
        }
        visualizer::WM_VIS => {
            visualizer::refresh();
            0
        }
        WM_HOTKEY => {
            let (cfg, _, _, _, scale) = snap();
            if wp == 2 {
                lockscreen::lock(pal(&cfg), scale);
            } else {
                search::toggle(pal(&cfg), scale);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let (_, bar, dock, wid, _) = snap();
            for w in [bar, dock, wid] {
                if w != 0 {
                    DestroyWindow(w as HWND);
                }
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    if env::args().any(|a| a == "--restore") {
        let c = Config::load();
        if wall::restore(&c).is_err() {
            std::process::exit(1);
        }
        return;
    }
    unsafe {
        let scale = init_dpi();
        let host_class = wide(HOST_CLASS);
        if !windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(host_class.as_ptr(), ptr::null())
            .is_null()
        {
            return;
        }
        let ok = register(&host_class, Some(ctl_proc))
            && register(&wide(BAR_CLASS), Some(bar_proc))
            && register(&wide(DOCK_CLASS), Some(dock_proc))
            && register(&wide(WID_CLASS), Some(wid_proc))
            && search::init()
            && island::init()
            && lockscreen::init()
            && visualizer::init();
        if !ok {
            std::process::exit(1);
        }
        let cfg = Config::load();
        HOST.with(|h| {
            let mut h = h.borrow_mut();
            h.cfg = cfg;
            h.scale = scale;
        });
        let ctl = create(&host_class, 0, WS_POPUP, 0, 0, 0, 0);
        if ctl.is_null() {
            std::process::exit(1);
        }
        CTL.with(|c| c.set(ctl as usize));
        sync();
        thread::spawn(|| {
            let _ = wall::ensure();
        });
        pump();
    }
}
