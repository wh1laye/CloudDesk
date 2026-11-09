#![windows_subsystem = "windows"]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::env;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::thread;

use clouddesk_ui::config::{dir as config_dir, file_name, Config, THEMES};
use clouddesk_ui::gfx::{
    disc, measure, mix, paint, palette, picture, rgrad, routline, rrect, txt, Ctx, R,
};
use clouddesk_ui::gp::Image;
use clouddesk_ui::{create, init_dpi, open, pump, register, wall, wide, HOST_CLASS};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DefWindowProcW, FindWindowW, GetSystemMetrics, LoadCursorW, PostMessageW,
    PostQuitMessage, SetCursor, SetForegroundWindow, SetWindowTextW, ShowWindow, IDC_HAND,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_PAINT, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

const CLASS: &str = "CloudDeskSettings";
const WM_SETCURSOR_ID: u32 = 0x0020;
const WM_MOUSELEAVE_ID: u32 = 0x02A3;
const WIN_W: i32 = 960;
const WIN_H: i32 = 640;
const CX: i32 = 240;
const CW: i32 = 688;
const PER_PAGE: usize = 12;
const MAX_ZONE: usize = 6;
const THUMBS_PER_PAINT: usize = 3;

const COMPONENTS: [(&str, &str); 8] = [
    ("Desktop", "Top bar, dock and the clock widget"),
    ("Island", "Now playing island under the top bar"),
    ("Search", "Launcher on Alt+Space or Win+D"),
    ("Visualizer", "Live audio equalizer on the desktop"),
    ("Colors", "Themes and accent colors"),
    ("Music", "Media keys and player control"),
    ("Widgets", "Desktop widgets, including your own"),
    ("Lock screen", "Ctrl+Alt+L cover inside your session"),
];
const IMPL: [bool; 8] = [true, true, true, true, true, false, true, true];
const TABS: [&str; 6] = [
    "Components",
    "Appearance",
    "Top bar",
    "Desktop",
    "Wallpaper",
    "About",
];
const ZONES: [&str; 3] = ["Left side", "Center", "Right side"];
const BAR_ITEMS: [(&str, &str); 8] = [
    ("settings", "Settings"),
    ("search", "Search"),
    ("spaces", "Desktops"),
    ("clock", "Date and time"),
    ("time", "Time"),
    ("battery", "Battery"),
    ("wall", "Wallpaper"),
    ("name", "Name"),
];
const DEFAULT_BAR: [&str; 3] = ["settings,search,spaces", "clock", "wall,battery"];

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Tab(u8),
    Toggle(usize),
    Theme(usize),
    Contour,
    Spaces,
    Taskbar,
    Wall(usize),
    Page(i32),
    Restore,
    Folder,
    Shell,
    BarItem(u8, usize),
    BarReset,
    ConfigFolder,
}

#[derive(Clone)]
struct St {
    cfg: Config,
    tab: u8,
    scale: i32,
    msg: String,
    hover: Option<Act>,
    page: usize,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St {
        cfg: Config::new(),
        tab: 0,
        scale: 100,
        msg: String::new(),
        hover: None,
        page: 0,
    });
    static HITS: RefCell<Vec<(R, Act)>> = const { RefCell::new(Vec::new()) };
    static THUMBS: RefCell<HashMap<PathBuf, Option<Image>>> = RefCell::new(HashMap::new());
    static MORE: Cell<bool> = const { Cell::new(false) };
    static TRACK: Cell<bool> = const { Cell::new(false) };
}

fn inside(r: R, x: i32, y: i32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn at(x: i32, y: i32) -> Option<Act> {
    HITS.with(|h| {
        h.borrow()
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, a)| *a)
    })
}

fn on_accent(acc: (u8, u8, u8)) -> (u8, u8, u8) {
    let l = (acc.0 as u32 * 299 + acc.1 as u32 * 587 + acc.2 as u32 * 114) / 1000;
    if l > 150 {
        (10, 10, 10)
    } else {
        (255, 255, 255)
    }
}

fn cap(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

fn items_of(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

fn zone_ref(cfg: &Config, z: u8) -> &String {
    match z {
        0 => &cfg.bar_left,
        1 => &cfg.bar_center,
        _ => &cfg.bar_right,
    }
}

fn zone_mut(cfg: &mut Config, z: u8) -> &mut String {
    match z {
        0 => &mut cfg.bar_left,
        1 => &mut cfg.bar_center,
        _ => &mut cfg.bar_right,
    }
}

fn toggle_item(cfg: &mut Config, z: u8, item: &str) {
    let present = items_of(zone_ref(cfg, z)).iter().any(|x| x == item);
    for k in 0..3u8 {
        let kept: Vec<String> = items_of(zone_ref(cfg, k))
            .into_iter()
            .filter(|x| x != item)
            .collect();
        *zone_mut(cfg, k) = kept.join(",");
    }
    if !present {
        let mut v = items_of(zone_ref(cfg, z));
        if v.len() < MAX_ZONE {
            v.push(item.to_string());
        }
        *zone_mut(cfg, z) = v.join(",");
    }
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

fn edit(f: impl FnOnce(&mut Config)) -> Config {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        f(&mut s.cfg);
        s.cfg.clone()
    })
}

unsafe fn persist(cfg: &Config) {
    match cfg.save() {
        Ok(()) => {
            set_msg("");
            notify();
        }
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

unsafe fn run(act: Act) {
    match act {
        Act::Tab(t) => ST.with(|s| {
            let mut s = s.borrow_mut();
            s.tab = t;
            s.msg.clear();
        }),
        Act::Toggle(i) if IMPL[i] => persist(&edit(|c| c.comps[i] = !c.comps[i])),
        Act::Toggle(_) => set_msg("This component is not built yet"),
        Act::Taskbar => persist(&edit(|c| c.hide_taskbar = !c.hide_taskbar)),
        Act::Spaces => persist(&edit(|c| c.spaces = !c.spaces)),
        Act::Contour => persist(&edit(|c| c.contour = !c.contour)),
        Act::Theme(i) => persist(&edit(|c| c.theme = THEMES[i].to_string())),
        Act::BarItem(z, i) => persist(&edit(|c| toggle_item(c, z, BAR_ITEMS[i].0))),
        Act::BarReset => persist(&edit(|c| {
            c.bar_left = DEFAULT_BAR[0].into();
            c.bar_center = DEFAULT_BAR[1].into();
            c.bar_right = DEFAULT_BAR[2].into();
        })),
        Act::Wall(i) => {
            let mut cfg = ST.with(|s| s.borrow().cfg.clone());
            match wall::pick(&mut cfg, i) {
                Ok(()) => {
                    ST.with(|s| s.borrow_mut().cfg = cfg);
                    set_msg("");
                    notify();
                }
                Err(e) => set_msg(&e),
            }
        }
        Act::Page(d) => ST.with(|s| {
            let mut s = s.borrow_mut();
            let pages = wall::list().len().div_ceil(PER_PAGE).max(1) as i32;
            s.page = (s.page as i32 + d).rem_euclid(pages) as usize;
        }),
        Act::Restore => {
            let cfg = ST.with(|s| s.borrow().cfg.clone());
            match wall::restore(&cfg) {
                Ok(()) => set_msg("Original wallpaper restored"),
                Err(e) => set_msg(&e),
            }
        }
        Act::Folder => {
            if wall::ensure().is_ok() {
                open(&wall::dir().to_string_lossy());
            }
        }
        Act::ConfigFolder => open(&config_dir().to_string_lossy()),
        Act::Shell => {
            let h = host_window();
            if h.is_null() {
                open(&host_exe());
            } else {
                PostMessageW(h, WM_CLOSE, 0, 0);
            }
        }
    }
}

struct Ui<'a> {
    c: &'a Ctx,
    hits: Vec<(R, Act)>,
    hover: Option<Act>,
}

impl Ui<'_> {
    fn hot(&mut self, r: R, a: Act) -> bool {
        self.hits.push((r, a));
        self.hover == Some(a)
    }

    unsafe fn button(&mut self, r: R, label: &str, primary: bool, act: Act) {
        let c = self.c;
        let p = c.pal;
        let hot = self.hot(r, act);
        let w = measure(label, 13, true);
        if primary {
            rrect(
                c,
                r,
                r.3,
                if hot { mix(p.acc, p.fg, 0.18) } else { p.acc },
                255,
            );
            txt(
                c,
                label,
                r.0 + (r.2 - w) / 2,
                r.1 + r.3 / 2,
                13,
                on_accent(p.acc),
                true,
            );
        } else {
            rrect(c, r, r.3, p.fg, if hot { 34 } else { 18 });
            txt(c, label, r.0 + (r.2 - w) / 2, r.1 + r.3 / 2, 13, p.fg, true);
        }
    }

    unsafe fn switch(&mut self, x: i32, y: i32, on: bool) {
        let c = self.c;
        let p = c.pal;
        if on {
            rrect(c, (x, y, 44, 26), 26, p.acc, 255);
        } else {
            rrect(c, (x, y, 44, 26), 26, p.fg, 40);
        }
        let kx = if on { x + 31 } else { x + 13 };
        disc(c, kx as f32, y as f32 + 14.0, 10.0, (0, 0, 0), 40);
        disc(
            c,
            kx as f32,
            y as f32 + 13.0,
            9.5,
            (255, 255, 255),
            if on { 255 } else { 215 },
        );
    }

    unsafe fn row(&mut self, r: R, name: &str, desc: &str, on: bool, live: bool, act: Act) {
        let c = self.c;
        let p = c.pal;
        let hot = self.hot(r, act);
        rrect(c, r, 18, p.fg, if hot { 26 } else { 14 });
        let cy = r.1 + r.3 / 2;
        txt(
            c,
            name,
            r.0 + 20,
            cy - 9,
            15,
            if live { p.fg } else { p.dim },
            true,
        );
        txt(c, desc, r.0 + 20, cy + 11, 12, p.dim, false);
        self.switch(r.0 + r.2 - 64, r.1 + (r.3 - 26) / 2, on && live);
    }

    unsafe fn head(&self, title: &str, sub: &str) {
        let c = self.c;
        txt(c, title, CX, 50, 26, c.pal.fg, true);
        txt(c, sub, CX, 82, 13, c.pal.dim, false);
    }

    unsafe fn chip(&mut self, r: R, label: &str, on: bool, act: Act) {
        let c = self.c;
        let p = c.pal;
        let hot = self.hot(r, act);
        if on {
            rrect(
                c,
                r,
                r.3,
                if hot { mix(p.acc, p.fg, 0.18) } else { p.acc },
                255,
            );
        } else {
            rrect(c, r, r.3, p.fg, if hot { 30 } else { 16 });
        }
        let w = measure(label, 13, on);
        let ink = if on { on_accent(p.acc) } else { p.fg };
        txt(c, label, r.0 + (r.2 - w) / 2, r.1 + r.3 / 2, 13, ink, on);
    }
}

unsafe fn sidebar(ui: &mut Ui, s: &St) {
    let c = ui.c;
    let p = c.pal;
    rrect(c, (12, 12, 196, WIN_H - 24), 22, p.fg, 10);
    disc(c, 40.0, 46.0, 8.0, p.acc, 255);
    disc(c, 40.0, 46.0, 3.5, p.bg, 255);
    txt(c, "CloudDesk", 58, 46, 18, p.fg, true);
    for (i, n) in TABS.iter().enumerate() {
        let r: R = (24, 90 + i as i32 * 46, 172, 40);
        let act = Act::Tab(i as u8);
        let hot = ui.hot(r, act);
        let sel = s.tab as usize == i;
        if sel {
            rrect(c, r, 16, p.fg, 24);
            rrect(c, (r.0 + 8, r.1 + 11, 4, 18), 4, p.acc, 255);
        } else if hot {
            rrect(c, r, 16, p.fg, 12);
        }
        txt(
            c,
            n,
            r.0 + 26,
            r.1 + r.3 / 2,
            14,
            if sel { p.fg } else { p.dim },
            sel,
        );
    }
    let running = !host_window().is_null();
    let dot = if running { (60, 210, 120) } else { p.dim };
    disc(c, 40.0, WIN_H as f32 - 82.0, 4.5, dot, 255);
    let label = if running {
        "Shell is running"
    } else {
        "Shell is stopped"
    };
    txt(c, label, 54, WIN_H - 82, 12, p.dim, false);
    let text = if running { "Stop shell" } else { "Start shell" };
    ui.button((24, WIN_H - 64, 172, 38), text, !running, Act::Shell);
}

unsafe fn page_components(ui: &mut Ui, s: &St) {
    ui.head(
        "Components",
        "Turn parts of CloudDesk on or off. Changes apply instantly.",
    );
    for (i, (name, desc)) in COMPONENTS.iter().enumerate() {
        let r: R = (CX, 112 + i as i32 * 60, CW, 54);
        ui.row(r, name, desc, s.cfg.comps[i], IMPL[i], Act::Toggle(i));
    }
}

unsafe fn page_look(ui: &mut Ui, s: &St) {
    ui.head(
        "Appearance",
        "Pick a color theme. The shell and this window follow it.",
    );
    let c = ui.c;
    let p = c.pal;
    for (i, n) in THEMES.iter().enumerate() {
        let r: R = (
            CX + (i % 5) as i32 * 140,
            112 + (i / 5) as i32 * 190,
            128,
            170,
        );
        let hot = ui.hot(r, Act::Theme(i));
        let tp = palette(n);
        rrect(c, r, 22, p.fg, if hot { 26 } else { 14 });
        if s.cfg.theme == *n {
            routline(c, r, 22, 2.5, p.acc, 255);
        }
        let pr: R = (r.0 + 10, r.1 + 10, 108, 100);
        match tp.glass {
            1 => rgrad(c, pr, 16, (52, 84, 140), (14, 22, 46)),
            2 => rgrad(c, pr, 16, (222, 234, 250), (150, 180, 226)),
            _ => rrect(c, pr, 16, tp.bg, 255),
        }
        rrect(c, (pr.0 + 8, pr.1 + 8, pr.2 - 16, 12), 12, tp.fg, 60);
        rrect(c, (pr.0 + 10, pr.1 + 36, 56, 8), 8, tp.fg, 230);
        rrect(c, (pr.0 + 10, pr.1 + 52, 36, 6), 6, tp.dim, 230);
        disc(
            c,
            (pr.0 + pr.2 - 24) as f32,
            (pr.1 + pr.3 - 24) as f32,
            10.0,
            tp.acc,
            255,
        );
        txt(c, &cap(n), r.0 + 14, r.1 + 132, 14, p.fg, true);
        let kind = match tp.glass {
            1 => "Acrylic, dark",
            2 => "Acrylic, light",
            _ => "Solid",
        };
        txt(c, kind, r.0 + 14, r.1 + 151, 11, p.dim, false);
    }
}

unsafe fn page_bar(ui: &mut Ui, s: &St) {
    ui.head(
        "Top bar",
        "Choose what the bar shows and where. An item lives in one place.",
    );
    let c = ui.c;
    let p = c.pal;
    for z in 0..3u8 {
        let y = 112 + z as i32 * 146;
        txt(c, ZONES[z as usize], CX, y + 8, 15, p.fg, true);
        let on = items_of(zone_ref(&s.cfg, z));
        let (mut x, mut row) = (CX, 0);
        for (i, (key, label)) in BAR_ITEMS.iter().enumerate() {
            let w = measure(label, 13, true) + 30;
            if x + w > CX + CW {
                x = CX;
                row += 1;
            }
            let r: R = (x, y + 28 + row * 42, w, 34);
            ui.chip(r, label, on.iter().any(|k| k == key), Act::BarItem(z, i));
            x += w + 8;
        }
    }
    ui.button(
        (CX, 112 + 3 * 146 + 2, 150, 36),
        "Reset bar",
        false,
        Act::BarReset,
    );
    txt(
        c,
        "Up to 6 items per side, shown in the order you switch them on.",
        CX + 166,
        112 + 3 * 146 + 20,
        12,
        p.dim,
        false,
    );
}

unsafe fn page_desktop(ui: &mut Ui, s: &St) {
    ui.head("Desktop", "How CloudDesk sits on top of Windows.");
    ui.row(
        (CX, 112, CW, 54),
        "Hide Windows taskbar",
        "Restored automatically when the shell stops",
        s.cfg.hide_taskbar,
        true,
        Act::Taskbar,
    );
    ui.row(
        (CX, 172, CW, 54),
        "Virtual desktops",
        "Win+1 to Win+9 switch desktops",
        s.cfg.spaces,
        true,
        Act::Spaces,
    );
    let c = ui.c;
    let p = c.pal;
    let r: R = (CX, 232, CW, 54);
    let hot = ui.hot(r, Act::Contour);
    rrect(c, r, 18, p.fg, if hot { 26 } else { 14 });
    txt(c, "Visualizer style", r.0 + 20, r.1 + 18, 15, p.fg, true);
    txt(
        c,
        "Dots or smooth contour",
        r.0 + 20,
        r.1 + 38,
        12,
        p.dim,
        false,
    );
    let label = if s.cfg.contour { "Contour" } else { "Dots" };
    rrect(c, (r.0 + r.2 - 130, r.1 + 11, 110, 32), 32, p.acc, 255);
    let w = measure(label, 13, true);
    txt(
        c,
        label,
        r.0 + r.2 - 130 + (110 - w) / 2,
        r.1 + 27,
        13,
        on_accent(p.acc),
        true,
    );
    txt(c, "Dock", CX, 326, 15, p.fg, true);
    let mut x = CX;
    for (name, _) in s.cfg.dock.iter() {
        let label = clip(&cap(&name.to_lowercase()), 16);
        let w = measure(&label, 13, false) + 30;
        if x + w > CX + CW {
            break;
        }
        rrect(c, (x, 346, w, 34), 34, p.fg, 16);
        txt(c, &label, x + 15, 363, 13, p.fg, false);
        x += w + 8;
    }
    txt(
        c,
        "Pin apps from search with Ctrl+Enter, remove them with a right click on the dock.",
        CX,
        404,
        12,
        p.dim,
        false,
    );
}

fn wide_path(p: &Path) -> Vec<u16> {
    p.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

unsafe fn page_wall(ui: &mut Ui, s: &St) {
    ui.head(
        "Wallpaper",
        "CloudDesk presets and the wallpapers that ship with Windows.",
    );
    let c = ui.c;
    let p = c.pal;
    let list = wall::list();
    let pages = list.len().div_ceil(PER_PAGE).max(1);
    let page = s.page.min(pages - 1);
    let start = page * PER_PAGE;
    let mut budget = THUMBS_PER_PAINT;
    let mut more = false;
    for (k, path) in list.iter().skip(start).take(PER_PAGE).enumerate() {
        let idx = start + k;
        let r: R = (
            CX + (k % 4) as i32 * 176,
            112 + (k / 4) as i32 * 128,
            160,
            90,
        );
        let hot = ui.hot(r, Act::Wall(idx));
        rrect(c, r, 16, p.fg, 14);
        THUMBS.with(|t| {
            let mut t = t.borrow_mut();
            if !t.contains_key(path) {
                if budget > 0 {
                    budget -= 1;
                    let img = Image::load(&wide_path(path)).and_then(|i| i.thumb(320, 180));
                    t.insert(path.clone(), img);
                } else {
                    more = true;
                }
            }
            if let Some(Some(img)) = t.get(path) {
                picture(c, img, r, 16);
            }
        });
        if hot {
            routline(c, r, 16, 2.0, p.fg, 90);
        }
        if idx == s.cfg.wall_index {
            routline(c, r, 16, 3.0, p.acc, 255);
        }
        let tag = if wall::is_system(path) {
            "  Windows"
        } else {
            ""
        };
        let label = format!("{}{}", clip(&file_name(path), 16), tag);
        txt(c, &label, r.0 + 4, r.1 + r.3 + 14, 12, p.dim, false);
    }
    if more {
        MORE.with(|m| m.set(true));
    }
    if list.is_empty() {
        txt(c, "No wallpapers found yet.", CX, 140, 14, p.dim, false);
    }
    let y = 500;
    ui.button((CX, y, 100, 36), "Previous", false, Act::Page(-1));
    ui.button((CX + 110, y, 100, 36), "Next", false, Act::Page(1));
    txt(
        c,
        &format!("{} / {}", page + 1, pages),
        CX + 224,
        y + 18,
        13,
        p.dim,
        false,
    );
    ui.button(
        (CX + 340, y, 190, 36),
        "Restore original",
        false,
        Act::Restore,
    );
    ui.button((CX + 540, y, 148, 36), "Open folder", false, Act::Folder);
}

unsafe fn page_about(ui: &mut Ui) {
    ui.head("About", "Open desktop shell and theme for Windows 11.");
    let c = ui.c;
    let p = c.pal;
    rrect(c, (CX, 112, CW, 214), 22, p.fg, 14);
    disc(c, (CX + 46) as f32, 160.0, 18.0, p.acc, 255);
    disc(c, (CX + 46) as f32, 160.0, 7.0, p.bg, 255);
    txt(c, "CloudDesk", CX + 82, 150, 20, p.fg, true);
    txt(
        c,
        &format!("Version {}  ·  MIT license", env!("CARGO_PKG_VERSION")),
        CX + 82,
        176,
        12,
        p.dim,
        false,
    );
    txt(
        c,
        "Settings and wallpapers live in your profile, nothing is written to HKLM.",
        CX + 24,
        226,
        13,
        p.fg,
        false,
    );
    txt(
        c,
        "If the shell ever misbehaves, run clouddesk.exe --restore to bring the",
        CX + 24,
        262,
        13,
        p.dim,
        false,
    );
    txt(
        c,
        "Windows taskbar and your original wallpaper back.",
        CX + 24,
        284,
        13,
        p.dim,
        false,
    );
    ui.button(
        (CX, 346, 190, 38),
        "Open settings folder",
        false,
        Act::ConfigFolder,
    );
}

unsafe fn draw(c: &Ctx, s: &St) {
    let mut ui = Ui {
        c,
        hits: Vec::new(),
        hover: s.hover,
    };
    sidebar(&mut ui, s);
    match s.tab {
        0 => page_components(&mut ui, s),
        1 => page_look(&mut ui, s),
        2 => page_bar(&mut ui, s),
        3 => page_desktop(&mut ui, s),
        4 => page_wall(&mut ui, s),
        _ => page_about(&mut ui),
    }
    if !s.msg.is_empty() {
        let w = measure(&s.msg, 13, true) + 32;
        let r: R = (CX + CW - w, WIN_H - 52, w, 34);
        rrect(c, r, 34, c.pal.acc, 255);
        txt(
            c,
            &s.msg,
            r.0 + 16,
            r.1 + 17,
            13,
            on_accent(c.pal.acc),
            true,
        );
    }
    let hits = ui.hits;
    HITS.with(|h| *h.borrow_mut() = hits);
}

unsafe fn paint_win(hwnd: HWND) {
    let s = ST.with(|s| s.borrow().clone());
    let pal = palette(&s.cfg.theme);
    clouddesk_ui::glass::chrome(hwnd, pal.bg, pal.fg);
    paint(hwnd, s.scale, &pal, |c, _, _| draw(c, &s));
    if MORE.with(|m| m.replace(false)) {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

fn logical(lp: LPARAM) -> (i32, i32) {
    let scale = ST.with(|s| s.borrow().scale);
    let x = (lp & 0xffff) as u16 as i16 as i32;
    let y = ((lp >> 16) & 0xffff) as u16 as i16 as i32;
    (x * 100 / scale, y * 100 / scale)
}

unsafe fn on_click(hwnd: HWND, lp: LPARAM) {
    let (x, y) = logical(lp);
    if let Some(a) = at(x, y) {
        run(a);
    }
    InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe fn on_move(hwnd: HWND, lp: LPARAM) {
    if !TRACK.with(|t| t.replace(true)) {
        let mut e = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut e);
    }
    let (x, y) = logical(lp);
    let now = at(x, y);
    let changed = ST.with(|s| {
        let mut s = s.borrow_mut();
        let was = s.hover;
        s.hover = now;
        was != now
    });
    if changed {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

unsafe fn on_leave(hwnd: HWND) {
    TRACK.with(|t| t.set(false));
    let changed = ST.with(|s| s.borrow_mut().hover.take().is_some());
    if changed {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_win(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            on_click(hwnd, lp);
            0
        }
        WM_MOUSEMOVE => {
            on_move(hwnd, lp);
            0
        }
        WM_MOUSELEAVE_ID => {
            on_leave(hwnd);
            0
        }
        WM_SETCURSOR_ID if (lp & 0xffff) == 1 && ST.with(|s| s.borrow().hover.is_some()) => {
            SetCursor(LoadCursorW(ptr::null_mut(), IDC_HAND));
            1
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
            right: WIN_W * scale / 100,
            bottom: WIN_H * scale / 100,
        };
        AdjustWindowRectEx(&mut rc, style, 0, 0);
        let ww = rc.right - rc.left;
        let wh = rc.bottom - rc.top;
        let x = ((GetSystemMetrics(SM_CXSCREEN) - ww) / 2).max(0);
        let y = ((GetSystemMetrics(SM_CYSCREEN) - wh) / 2).max(0);
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
