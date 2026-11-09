use std::cell::RefCell;
use std::ptr;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use clouddesk_ui::gfx::{dot, paint, pill, text, Ctx, Palette};
use clouddesk_ui::{create, register, wide};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, InvalidateRect, Polygon, SelectObject, SetWindowRgn,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetSystemMetrics, PostMessageW, SetWindowPos, HWND_TOPMOST,
    SM_CXSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_APP, WM_ERASEBKGND, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_PAINT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub const WM_MEDIA: u32 = WM_APP + 3;

const CLASS: &str = "CloudDeskIsland";
const W: i32 = 420;
const H: i32 = 64;
const TOP: i32 = 44;
const MA_NOACTIVATE: LRESULT = 3;

#[derive(Clone, PartialEq)]
struct Media {
    title: String,
    artist: String,
    playing: bool,
    present: bool,
}

#[derive(Clone, Copy)]
enum Cmd {
    Prev,
    Toggle,
    Next,
}

struct Isl {
    pal: Option<Palette>,
    scale: i32,
    hwnd: usize,
}

static MEDIA: Mutex<Media> = Mutex::new(Media {
    title: String::new(),
    artist: String::new(),
    playing: false,
    present: false,
});
static TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);

thread_local! {
    static ISL: RefCell<Isl> = const {
        RefCell::new(Isl {
            pal: None,
            scale: 100,
            hwnd: 0,
        })
    };
}

fn read(s: &Session) -> Option<Media> {
    let p = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let status = s.GetPlaybackInfo().ok()?.PlaybackStatus().ok()?;
    Some(Media {
        title: p.Title().ok()?.to_string(),
        artist: p.Artist().ok()?.to_string(),
        playing: status == Status::Playing,
        present: true,
    })
}

fn worker(ctl: usize, rx: Receiver<Cmd>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let Ok(mgr) = Manager::RequestAsync().and_then(|a| a.get()) else {
        return;
    };
    loop {
        let next = mgr
            .GetCurrentSession()
            .ok()
            .and_then(|s| read(&s))
            .unwrap_or(Media {
                title: String::new(),
                artist: String::new(),
                playing: false,
                present: false,
            });
        let changed = match MEDIA.lock() {
            Ok(mut m) => {
                let c = *m != next;
                *m = next;
                c
            }
            Err(_) => false,
        };
        if changed {
            unsafe {
                PostMessageW(ctl as HWND, WM_MEDIA, 0, 0);
            }
        }
        match rx.recv_timeout(Duration::from_millis(1500)) {
            Ok(cmd) => {
                if let Ok(s) = mgr.GetCurrentSession() {
                    let _ = match cmd {
                        Cmd::Prev => s.TrySkipPreviousAsync(),
                        Cmd::Toggle => s.TryTogglePlayPauseAsync(),
                        Cmd::Next => s.TrySkipNextAsync(),
                    };
                }
                thread::sleep(Duration::from_millis(300));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn send(cmd: Cmd) {
    if let Ok(g) = TX.lock() {
        if let Some(tx) = g.as_ref() {
            let _ = tx.send(cmd);
        }
    }
}

fn clip(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(n).collect()
}

unsafe fn tri(c: &Ctx, x: i32, y: i32, d: i32) {
    let p = |px: i32, py: i32| POINT {
        x: c.s(px),
        y: c.s(py),
    };
    let pts = [p(x - 6 * d, y - 9), p(x - 6 * d, y + 9), p(x + 8 * d, y)];
    SelectObject(c.dc, c.fg);
    Polygon(c.dc, pts.as_ptr(), 3);
}

unsafe fn draw(hwnd: HWND) -> bool {
    let (pal, scale) = ISL.with(|i| {
        let i = i.borrow();
        (i.pal, i.scale)
    });
    let Some(pal) = pal else {
        return false;
    };
    let m = MEDIA.lock().map(|m| m.clone()).unwrap_or(Media {
        title: String::new(),
        artist: String::new(),
        playing: false,
        present: false,
    });
    paint(hwnd, scale, &pal, |c, _, _| {
        dot(c, if m.playing { c.acc } else { c.dim }, 32, 32, 7);
        text(c, &clip(&m.title, 20), 52, 10, 3, c.fg);
        text(c, &clip(&m.artist, 20), 52, 35, 2, c.dim);
        tri(c, 315, 32, -1);
        tri(c, 351, 32, 1);
        if m.playing {
            pill(c, 343, 23, 6, 18, c.fg);
            pill(c, 353, 23, 6, 18, c.fg);
        }
        tri(c, 384, 32, 1);
        pill(c, 394, 23, 3, 18, c.fg);
        pill(c, 306, 23, 3, 18, c.fg);
    });
    true
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if draw(hwnd) {
                0
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_LBUTTONDOWN => {
            let scale = ISL.with(|i| i.borrow().scale);
            let x = (lp & 0xffff) as u16 as i16 as i32 * 100 / scale;
            if (294..330).contains(&x) {
                send(Cmd::Prev);
            } else if (330..366).contains(&x) {
                send(Cmd::Toggle);
            } else if (366..402).contains(&x) {
                send(Cmd::Next);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn init() -> bool {
    register(&wide(CLASS), Some(proc))
}

pub unsafe fn sync(on: bool, pal: Palette, scale: i32, ctl: HWND) {
    ISL.with(|i| {
        let mut i = i.borrow_mut();
        i.pal = Some(pal);
        i.scale = scale;
    });
    if on {
        if let Ok(mut g) = TX.lock() {
            if g.is_none() {
                let (tx, rx) = channel();
                *g = Some(tx);
                let target = ctl as usize;
                thread::spawn(move || worker(target, rx));
            }
        }
        let h = ISL.with(|i| i.borrow().hwnd);
        if h != 0 {
            InvalidateRect(h as HWND, ptr::null(), 0);
        }
    } else {
        if let Ok(mut g) = TX.lock() {
            *g = None;
        }
        if let Ok(mut m) = MEDIA.lock() {
            m.present = false;
        }
        refresh();
    }
}

pub unsafe fn refresh() {
    let (present, ready) = match MEDIA.lock() {
        Ok(m) => (m.present && !m.title.is_empty(), true),
        Err(_) => (false, false),
    };
    if !ready {
        return;
    }
    let (hwnd, scale) = ISL.with(|i| {
        let i = i.borrow();
        (i.hwnd, i.scale)
    });
    if present && hwnd == 0 {
        let w = W * scale / 100;
        let h = H * scale / 100;
        let win = create(
            &wide(CLASS),
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            WS_POPUP,
            0,
            0,
            w,
            h,
        );
        if win.is_null() {
            return;
        }
        let r = H * scale / 100;
        SetWindowRgn(win, CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r), 0);
        SetWindowPos(
            win,
            HWND_TOPMOST,
            (GetSystemMetrics(SM_CXSCREEN) - w) / 2,
            TOP * scale / 100,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        ISL.with(|i| i.borrow_mut().hwnd = win as usize);
    } else if present {
        InvalidateRect(hwnd as HWND, ptr::null(), 0);
    } else if hwnd != 0 {
        DestroyWindow(hwnd as HWND);
        ISL.with(|i| i.borrow_mut().hwnd = 0);
    }
}
