#![windows_subsystem = "windows"]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::Path;
use std::ptr;
use std::time::SystemTime;

use clouddesk_ui::config::{dir, Config};
use clouddesk_ui::gfx::{dot, paint, palette, pill, text, Ctx, Palette};
use clouddesk_ui::{create, init_dpi, now, pump, register, wide};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetSystemTimes, OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowLongPtrW, PostQuitMessage, SetTimer, SetWindowLongPtrW, SetWindowPos,
    GWLP_USERDATA, HWND_BOTTOM, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_PAINT, WM_TIMER, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

const CLASS: &str = "CloudDeskUserWidget";
const MAX_WIDGETS: usize = 16;
const MAX_MANIFEST: u64 = 4096;
const MAX_TEXT: usize = 48;
const TIMER_DATA: usize = 1;
const TIMER_WATCH: usize = 2;
const WATCH_MS: u32 = 3000;
const MA_NOACTIVATE: LRESULT = 3;

const SEED_MEMORY: &str = "[widget]\nname = 'MEMORY'\nkind = 'memory'\npermissions = 'system'\nx = 24\ny = 60\n";
const SEED_CPU: &str = "[widget]\nname = 'CPU'\nkind = 'cpu'\npermissions = 'system'\nx = 24\ny = 210\n";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Clock,
    Text,
    Battery,
    Memory,
    Cpu,
}

struct Widget {
    kind: Kind,
    name: String,
    text: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    value: u32,
    aux: u32,
    last: (u64, u64),
}

thread_local! {
    static WIDGETS: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
    static PAL: RefCell<Option<Palette>> = const { RefCell::new(None) };
    static SCALE: Cell<i32> = const { Cell::new(100) };
    static PARENT: Cell<usize> = const { Cell::new(0) };
    static WINS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static CFG_TIME: Cell<Option<SystemTime>> = const { Cell::new(None) };
}

fn clean_text(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(max)
        .collect::<String>()
        .to_uppercase()
}

fn read_fields(path: &Path) -> Option<HashMap<String, String>> {
    let md = fs::symlink_metadata(path).ok()?;
    if !md.is_file() || md.len() > MAX_MANIFEST {
        return None;
    }
    let src = fs::read_to_string(path).ok()?;
    let mut inside = false;
    let mut map = HashMap::new();
    for line in src.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') && l.ends_with(']') {
            inside = &l[1..l.len() - 1] == "widget";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let v = v.trim().trim_matches(|ch| ch == '\'' || ch == '"');
            map.insert(k.trim().to_string(), v.to_string());
        }
    }
    Some(map)
}

fn num(m: &HashMap<String, String>, k: &str, def: i32, lo: i32, hi: i32) -> i32 {
    m.get(k)
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(def)
        .clamp(lo, hi)
}

fn parse(path: &Path) -> Option<Widget> {
    let m = read_fields(path)?;
    let kind = match m.get("kind")?.as_str() {
        "clock" => Kind::Clock,
        "text" => Kind::Text,
        "battery" => Kind::Battery,
        "memory" => Kind::Memory,
        "cpu" => Kind::Cpu,
        _ => return None,
    };
    let system = m
        .get("permissions")
        .map(|p| p.split(',').any(|x| x.trim() == "system"))
        .unwrap_or(false);
    if matches!(kind, Kind::Battery | Kind::Memory | Kind::Cpu) && !system {
        return None;
    }
    let name = clean_text(m.get("name").map(String::as_str).unwrap_or(""), 24);
    let label = clean_text(m.get("text").map(String::as_str).unwrap_or(""), MAX_TEXT);
    Some(Widget {
        kind,
        name,
        text: label,
        x: num(&m, "x", 24, 0, 8000),
        y: num(&m, "y", 60, 0, 8000),
        w: num(&m, "width", 220, 120, 600),
        h: num(&m, "height", 130, 100, 400),
        value: 0,
        aux: 0,
        last: (0, 0),
    })
}

fn seed(root: &Path) {
    if root.exists() {
        return;
    }
    for (name, body) in [("memory", SEED_MEMORY), ("cpu", SEED_CPU)] {
        let d = root.join(name);
        if fs::create_dir_all(&d).is_ok() {
            let _ = fs::write(d.join("widget.toml"), body);
        }
    }
}

fn discover() -> Vec<Widget> {
    let root = dir().join("widgets");
    seed(&root);
    let Ok(rd) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if out.len() >= MAX_WIDGETS {
            break;
        }
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        if let Some(w) = parse(&e.path().join("widget.toml")) {
            out.push(w);
        }
    }
    out
}

fn filetime(f: &FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn cpu_times() -> Option<(u64, u64)> {
    unsafe {
        let mut idle: FILETIME = zeroed();
        let mut kernel: FILETIME = zeroed();
        let mut user: FILETIME = zeroed();
        if GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 {
            return None;
        }
        Some((filetime(&idle), filetime(&kernel) + filetime(&user)))
    }
}

fn mem_percent() -> u32 {
    unsafe {
        let mut s: MEMORYSTATUSEX = zeroed();
        s.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut s) == 0 {
            return 0;
        }
        s.dwMemoryLoad.min(100)
    }
}

fn battery() -> (u32, u32) {
    unsafe {
        let mut s: SYSTEM_POWER_STATUS = zeroed();
        if GetSystemPowerStatus(&mut s) == 0 || s.BatteryFlag == 128 {
            return (255, 0);
        }
        (s.BatteryLifePercent as u32, s.ACLineStatus as u32)
    }
}

fn sample(w: &mut Widget) {
    match w.kind {
        Kind::Memory => w.value = mem_percent(),
        Kind::Battery => {
            let (p, ac) = battery();
            w.value = p;
            w.aux = ac;
        }
        Kind::Cpu => {
            if let Some((idle, total)) = cpu_times() {
                let (pi, pt) = w.last;
                if total > pt {
                    let dt = total - pt;
                    let di = idle.saturating_sub(pi).min(dt);
                    w.value = (100 * (dt - di) / dt) as u32;
                }
                w.last = (idle, total);
            }
        }
        Kind::Clock | Kind::Text => {}
    }
}

fn interval(kind: Kind, first: bool) -> u32 {
    match kind {
        Kind::Clock if first => (60 - now().sec) * 1000 + 100,
        Kind::Clock => 60000,
        Kind::Battery => 10000,
        Kind::Memory | Kind::Cpu => 2000,
        Kind::Text => 0,
    }
}

fn fit(main: &str, lw: i32, lh: i32) -> i32 {
    let by_h = ((lh - 94) / 7).clamp(3, 8);
    let by_w = (lw - 40) / (6 * main.len().max(1) as i32);
    by_h.min(by_w).max(2)
}

unsafe fn draw(c: &Ctx, w: &Widget, lw: i32, lh: i32) {
    pill(c, 0, 0, lw, lh, c.dark);
    dot(c, c.acc, 24, 24, 4);
    let max = ((lw - 50) / 18).max(1) as usize;
    let title: String = w.name.chars().take(max).collect();
    text(c, &title, 38, 14, 3, c.dim);
    let (main, sub, bar): (String, String, Option<u32>) = match w.kind {
        Kind::Clock => {
            let t = now();
            (t.hm, t.date, None)
        }
        Kind::Text => (w.text.clone(), String::new(), None),
        Kind::Memory => (format!("{}%", w.value), "USED".into(), Some(w.value)),
        Kind::Cpu => (format!("{}%", w.value), "LOAD".into(), Some(w.value)),
        Kind::Battery if w.value > 100 => ("--".into(), "NO BATTERY".into(), None),
        Kind::Battery => (
            format!("{}%", w.value),
            if w.aux == 1 { "AC".into() } else { "BATTERY".into() },
            Some(w.value),
        ),
    };
    text(c, &main, 20, 46, fit(&main, lw, lh), c.fg);
    if !sub.is_empty() {
        text(c, &sub, 22, lh - 40, 3, c.dim);
    }
    if let Some(p) = bar {
        let track = lw - 44;
        pill(c, 22, lh - 16, track, 6, c.bg);
        if p > 0 {
            pill(c, 22, lh - 16, (track * p.min(100) as i32 / 100).max(6), 6, c.acc);
        }
    }
}

fn index(hwnd: HWND) -> Option<usize> {
    let v = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as usize;
    v.checked_sub(1)
}

unsafe fn paint_one(hwnd: HWND, i: usize) {
    let scale = SCALE.with(|s| s.get());
    PAL.with(|p| {
        let p = p.borrow();
        let Some(pal) = p.as_ref() else {
            return;
        };
        paint(hwnd, scale, pal, |c, lw, lh| {
            WIDGETS.with(|ws| {
                if let Some(w) = ws.borrow().get(i) {
                    draw(c, w, lw, lh);
                }
            });
        });
    });
}

fn load_theme() {
    let cfg = Config::load();
    PAL.with(|p| *p.borrow_mut() = Some(palette(&cfg.theme)));
}

fn config_time() -> Option<SystemTime> {
    fs::metadata(dir().join("config.toml"))
        .and_then(|m| m.modified())
        .ok()
}

unsafe fn watch() {
    let parent = PARENT.with(|p| p.get());
    if parent == 0 || WaitForSingleObject(parent as _, 0) == 0 {
        PostQuitMessage(0);
        return;
    }
    let t = config_time();
    if t != CFG_TIME.with(|c| c.get()) {
        CFG_TIME.with(|c| c.set(t));
        load_theme();
        WINS.with(|w| {
            for h in w.borrow().iter() {
                InvalidateRect(*h as HWND, ptr::null(), 0);
            }
        });
    }
}

unsafe extern "system" fn wid_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(i) = index(hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    match msg {
        WM_PAINT => {
            paint_one(hwnd, i);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_TIMER if wp == TIMER_WATCH => {
            watch();
            0
        }
        WM_TIMER => {
            let kind = WIDGETS.with(|ws| {
                let mut ws = ws.borrow_mut();
                ws.get_mut(i).map(|w| {
                    sample(w);
                    w.kind
                })
            });
            if let Some(k) = kind {
                SetTimer(hwnd, TIMER_DATA, interval(k, false).max(1000), None);
            }
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn parent_pid() -> Option<u32> {
    let mut it = env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--parent" {
            return it.next()?.parse().ok();
        }
    }
    None
}

unsafe fn spawn_all(scale: i32) -> bool {
    let count = WIDGETS.with(|ws| ws.borrow().len());
    let mut shown = 0;
    for i in 0..count {
        let Some((x, y, w, h, kind)) = WIDGETS.with(|ws| {
            ws.borrow_mut().get_mut(i).map(|v| {
                sample(v);
                (v.x, v.y, v.w, v.h, v.kind)
            })
        }) else {
            continue;
        };
        let (pw, ph) = (w * scale / 100, h * scale / 100);
        let win = create(
            &wide(CLASS),
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            WS_POPUP,
            0,
            0,
            pw,
            ph,
        );
        if win.is_null() {
            continue;
        }
        WINS.with(|w| w.borrow_mut().push(win as usize));
        SetWindowLongPtrW(win, GWLP_USERDATA, (i + 1) as isize);
        let r = 28 * scale / 100;
        SetWindowRgn(win, CreateRoundRectRgn(0, 0, pw + 1, ph + 1, r, r), 0);
        SetWindowPos(
            win,
            HWND_BOTTOM,
            x * scale / 100,
            y * scale / 100,
            pw,
            ph,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let ms = interval(kind, true);
        if ms > 0 {
            SetTimer(win, TIMER_DATA, ms, None);
        }
        if shown == 0 {
            SetTimer(win, TIMER_WATCH, WATCH_MS, None);
        }
        shown += 1;
    }
    shown > 0
}

fn main() {
    let Some(pid) = parent_pid() else {
        return;
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        PARENT.with(|p| p.set(handle as usize));
        let scale = init_dpi();
        SCALE.with(|s| s.set(scale));
        let list = discover();
        if list.is_empty() || !register(&wide(CLASS), Some(wid_proc)) {
            CloseHandle(handle);
            return;
        }
        WIDGETS.with(|ws| *ws.borrow_mut() = list);
        load_theme();
        CFG_TIME.with(|c| c.set(config_time()));
        if spawn_all(scale) {
            pump();
        }
        CloseHandle(handle);
    }
}
