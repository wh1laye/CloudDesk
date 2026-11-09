use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use clouddesk_ui::config::file_name;
use clouddesk_ui::gfx::{dot, paint, pill, text, Palette};
use clouddesk_ui::{create, open, register, wide};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetSystemMetrics, SetForegroundWindow, ShowWindow, SM_CXSCREEN,
    SM_CYSCREEN, SW_SHOW, WM_ACTIVATE, WM_CHAR, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_PAINT, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: &str = "CloudDeskSearch";
const W: i32 = 640;
const H: i32 = 340;
const TOP: i32 = 76;
const ROW: i32 = 44;
const ROWS: usize = 6;
const MAX_QUERY: usize = 30;
const MAX_ENTRIES: usize = 3000;
const VK_UP: u32 = 0x26;
const VK_DOWN: u32 = 0x28;

struct State {
    pal: Palette,
    scale: i32,
    hwnd: usize,
    all: Vec<(String, PathBuf)>,
    shown: Vec<usize>,
    query: String,
    sel: usize,
}

thread_local! {
    static ST: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn collect(dir: &Path, depth: u32, out: &mut Vec<(String, PathBuf)>) {
    if depth > 3 || out.len() >= MAX_ENTRIES {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect(&p, depth + 1, out);
        } else if p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            let name = file_name(&p);
            if !name.is_empty() {
                out.push((name, p));
            }
        }
    }
}

fn index() -> Vec<(String, PathBuf)> {
    let mut all = Vec::new();
    for var in ["APPDATA", "PROGRAMDATA"] {
        if let Some(base) = env::var_os(var) {
            let dir = PathBuf::from(base)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            collect(&dir, 0, &mut all);
        }
    }
    all.sort_by_key(|e| e.0.to_lowercase());
    all.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
    all
}

fn refilter(st: &mut State) {
    let q = st.query.to_lowercase();
    let mut hits: Vec<(bool, usize)> = Vec::new();
    if !q.is_empty() {
        for (i, (name, _)) in st.all.iter().enumerate() {
            let l = name.to_lowercase();
            if l.starts_with(&q) {
                hits.push((false, i));
            } else if l.contains(&q) {
                hits.push((true, i));
            }
        }
    }
    hits.sort_by_key(|h| h.0);
    st.shown = hits.into_iter().map(|h| h.1).take(ROWS).collect();
    st.sel = 0;
}

fn clip(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(n).collect()
}

unsafe fn launch(hwnd: HWND) {
    let target = ST.with(|s| {
        s.borrow()
            .as_ref()
            .and_then(|st| st.shown.get(st.sel).map(|&i| st.all[i].1.clone()))
    });
    if let Some(p) = target {
        open(&p.to_string_lossy());
        DestroyWindow(hwnd);
    }
}

unsafe fn on_char(hwnd: HWND, ch: u32) {
    let mut close = false;
    let mut enter = false;
    let mut pinned = false;
    ST.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            match ch {
                27 => close = true,
                13 => enter = true,
                10 => pinned = true,
                8 => {
                    st.query.pop();
                    refilter(st);
                }
                32..=126 if st.query.len() < MAX_QUERY => {
                    st.query.push(ch as u8 as char);
                    refilter(st);
                }
                _ => {}
            }
        }
    });
    if pinned {
        let t = ST.with(|s| {
            s.borrow()
                .as_ref()
                .and_then(|st| st.shown.get(st.sel).map(|&i| st.all[i].clone()))
        });
        if let Some((name, path)) = t {
            crate::dock::pin(&name, &path.to_string_lossy());
        }
        DestroyWindow(hwnd);
    } else if close {
        DestroyWindow(hwnd);
    } else if enter {
        launch(hwnd);
    } else {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

unsafe fn on_key(hwnd: HWND, vk: u32) {
    ST.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            if vk == VK_UP {
                st.sel = st.sel.saturating_sub(1);
            } else if vk == VK_DOWN && st.sel + 1 < st.shown.len() {
                st.sel += 1;
            }
        }
    });
    InvalidateRect(hwnd, std::ptr::null(), 0);
}

unsafe fn on_click(hwnd: HWND, py: i32) {
    let picked = ST.with(|s| {
        let mut b = s.borrow_mut();
        let st = b.as_mut()?;
        let y = py * 100 / st.scale;
        if y < TOP {
            return None;
        }
        let row = ((y - TOP) / ROW) as usize;
        if row < st.shown.len() {
            st.sel = row;
            Some(())
        } else {
            None
        }
    });
    if picked.is_some() {
        launch(hwnd);
    }
}

unsafe fn draw(hwnd: HWND) -> bool {
    let snap = ST.with(|s| {
        s.borrow().as_ref().map(|st| {
            let rows: Vec<String> = st.shown.iter().map(|&i| clip(&st.all[i].0, 30)).collect();
            (st.pal, st.scale, st.query.clone(), rows, st.sel)
        })
    });
    let Some((pal, scale, query, rows, sel)) = snap else {
        return false;
    };
    paint(hwnd, scale, &pal, |c, lw, _| {
        dot(c, c.acc, 32, 32, 5);
        if query.is_empty() {
            text(c, "Search apps", 52, 22, 3, c.dim);
        } else {
            text(c, &query, 52, 22, 3, c.fg);
        }
        for (i, name) in rows.iter().enumerate() {
            let y = TOP + i as i32 * ROW;
            if i == sel {
                pill(c, 12, y, lw - 24, ROW - 4, c.dark);
            }
            text(c, name, 28, y + 10, 3, if i == sel { c.fg } else { c.dim });
        }
    });
    true
}

pub unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if draw(hwnd) {
                0
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_ERASEBKGND => 1,
        WM_CHAR => {
            on_char(hwnd, wp as u32);
            0
        }
        WM_KEYDOWN => {
            on_key(hwnd, wp as u32);
            0
        }
        WM_LBUTTONDOWN => {
            on_click(hwnd, ((lp >> 16) & 0xffff) as u16 as i16 as i32);
            0
        }
        WM_ACTIVATE => {
            if wp & 0xffff == 0 {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            ST.with(|s| *s.borrow_mut() = None);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn init() -> bool {
    register(&wide(CLASS), Some(proc))
}

pub unsafe fn toggle(pal: Palette, scale: i32) {
    let existing = ST.with(|s| s.borrow().as_ref().map(|st| st.hwnd));
    if let Some(h) = existing {
        DestroyWindow(h as HWND);
        return;
    }
    let all = index();
    let w = W * scale / 100;
    let h = H * scale / 100;
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let hwnd = create(
        &wide(CLASS),
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        WS_POPUP,
        (sw - w) / 2,
        sh * 22 / 100,
        w,
        h,
    );
    if hwnd.is_null() {
        return;
    }
    ST.with(|s| {
        *s.borrow_mut() = Some(State {
            pal,
            scale,
            hwnd: hwnd as usize,
            all,
            shown: Vec::new(),
            query: String::new(),
            sel: 0,
        });
    });
    let r = 28 * scale / 100;
    SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r), 0);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
}
