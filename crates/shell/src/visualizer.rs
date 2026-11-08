use std::cell::RefCell;
use std::f32::consts::PI;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clouddesk_ui::gfx::{dot, paint, Palette};
use clouddesk_ui::{create, register, wide};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetSystemMetrics, PostMessageW, SetWindowPos, HWND_BOTTOM,
    SM_CYSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_APP, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_LBUTTONDOWN, WM_PAINT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

pub const WM_VIS: u32 = WM_APP + 4;

const CLASS: &str = "CloudDeskVis";
const BANDS: usize = 24;
const ROWS: usize = 10;
const RING: usize = BANDS * 2;
const FFT_N: usize = 512;
const W: i32 = 360;
const H: i32 = 150;
const MA_NOACTIVATE: LRESULT = 3;

struct Vis {
    pal: Option<Palette>,
    scale: i32,
    hwnd: usize,
}

static NOW: Mutex<[u8; BANDS]> = Mutex::new([0; BANDS]);
static CONTOUR: AtomicBool = AtomicBool::new(false);
static STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

thread_local! {
    static VIS: RefCell<Vis> = const {
        RefCell::new(Vis {
            pal: None,
            scale: 100,
            hwnd: 0,
        })
    };
}

fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (i + k, i + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

fn edge(k: usize) -> usize {
    ((FFT_N as f32 / 2.0 - 1.0).powf(k as f32 / BANDS as f32) as usize).max(1)
}

fn spectrum(s: &[f32]) -> [f32; BANDS] {
    let n = s.len();
    let mut re: Vec<f32> = s
        .iter()
        .enumerate()
        .map(|(i, v)| v * (0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos()))
        .collect();
    let mut im = vec![0f32; n];
    fft(&mut re, &mut im);
    let mut out = [0f32; BANDS];
    for (k, o) in out.iter_mut().enumerate() {
        let lo = edge(k);
        let hi = edge(k + 1).max(lo + 1);
        let m = re[lo..hi]
            .iter()
            .zip(&im[lo..hi])
            .map(|(a, b)| (a * a + b * b).sqrt())
            .fold(0f32, f32::max);
        let db = 20.0 * (m / (n as f32 / 4.0) + 1e-6).log10();
        *o = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    }
    out
}

unsafe fn push(
    out: &mut Vec<f32>,
    data: *const u8,
    frames: usize,
    ch: usize,
    bits: u16,
    silent: bool,
) {
    for i in 0..frames {
        let mut v = 0.0f32;
        if !silent && !data.is_null() {
            for c in 0..ch {
                v += match bits {
                    32 => (data as *const f32).add(i * ch + c).read_unaligned(),
                    16 => (data as *const i16).add(i * ch + c).read_unaligned() as f32 / 32768.0,
                    _ => 0.0,
                };
            }
            v /= ch as f32;
        }
        out.push(v);
    }
    if out.len() > 2048 {
        let cut = out.len() - 2048;
        out.drain(..cut);
    }
}

fn capture(stop: &AtomicBool, ctl: usize) -> windows::core::Result<()> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        let fmt = client.GetMixFormat()?;
        let channels = ((*fmt).nChannels as usize).max(1);
        let bits = (*fmt).wBitsPerSample;
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0x0002_0000, 10_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as *const _));
        init?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        let mut mono: Vec<f32> = Vec::with_capacity(4096);
        let mut levels = [0f32; BANDS];
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(33));
            let mut got = false;
            loop {
                let n = cap.GetNextPacketSize()?;
                if n == 0 {
                    break;
                }
                let mut data: *mut u8 = ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                push(&mut mono, data, frames as usize, channels, bits, flags & 2 != 0);
                cap.ReleaseBuffer(frames)?;
                got = true;
            }
            let target = if got && mono.len() >= FFT_N {
                spectrum(&mono[mono.len() - FFT_N..])
            } else {
                mono.clear();
                [0f32; BANDS]
            };
            for (l, t) in levels.iter_mut().zip(target.iter()) {
                *l = if t > l { *t } else { *l * 0.82 };
            }
            let mut q = [0u8; BANDS];
            for (o, l) in q.iter_mut().zip(levels.iter()) {
                *o = (*l * 100.0) as u8;
            }
            let changed = match NOW.lock() {
                Ok(mut g) => {
                    let c = *g != q;
                    *g = q;
                    c
                }
                Err(_) => false,
            };
            if changed {
                PostMessageW(ctl as HWND, WM_VIS, 0, 0);
            }
        }
        let _ = client.Stop();
    }
    Ok(())
}

fn worker(stop: Arc<AtomicBool>, ctl: usize) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let _ = capture(&stop, ctl);
}

unsafe fn draw(hwnd: HWND) -> bool {
    let (pal, scale) = VIS.with(|v| {
        let v = v.borrow();
        (v.pal, v.scale)
    });
    let Some(pal) = pal else {
        return false;
    };
    let bands = NOW.lock().map(|g| *g).unwrap_or([0; BANDS]);
    let contour = CONTOUR.load(Ordering::Relaxed);
    paint(hwnd, scale, &pal, |c, _, _| {
        if contour {
            for k in 0..RING {
                let lv = bands[if k < BANDS { k } else { 2 * BANDS - 1 - k }] as f32;
                let a = k as f32 * 2.0 * PI / RING as f32;
                for (inner, base, amp) in [(false, 38.0, 0.3), (true, 22.0, 0.15)] {
                    let r = base + lv * amp;
                    let x = (180.0 + r * a.cos()) as i32;
                    let y = (75.0 + r * a.sin()) as i32;
                    let b = if inner {
                        c.dark
                    } else if lv > 0.0 {
                        c.acc
                    } else {
                        c.dim
                    };
                    dot(c, b, x, y, 3);
                }
            }
            return;
        }
        for (col, &lv) in bands.iter().enumerate() {
            let lit = (lv as i32 * ROWS as i32 + 99) / 100;
            for row in 0..ROWS as i32 {
                let b = if row < lit { c.acc } else { c.dark };
                dot(c, b, 20 + col as i32 * 14, 138 - row * 13, 4);
            }
        }
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
            CONTOUR.fetch_xor(true, Ordering::Relaxed);
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn init() -> bool {
    register(&wide(CLASS), Some(proc))
}

pub unsafe fn refresh() {
    let h = VIS.with(|v| v.borrow().hwnd);
    if h != 0 {
        InvalidateRect(h as HWND, ptr::null(), 0);
    }
}

pub unsafe fn sync(on: bool, pal: Palette, scale: i32, ctl: HWND) {
    VIS.with(|v| {
        let mut v = v.borrow_mut();
        v.pal = Some(pal);
        v.scale = scale;
    });
    let hwnd = VIS.with(|v| v.borrow().hwnd);
    if on {
        if let Ok(mut g) = STOP.lock() {
            if g.is_none() {
                let flag = Arc::new(AtomicBool::new(false));
                *g = Some(flag.clone());
                let target = ctl as usize;
                thread::spawn(move || worker(flag, target));
            }
        }
        if hwnd == 0 {
            let w = W * scale / 100;
            let h = H * scale / 100;
            let win = create(
                &wide(CLASS),
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                WS_POPUP,
                0,
                0,
                w,
                h,
            );
            if win.is_null() {
                return;
            }
            let r = 28 * scale / 100;
            SetWindowRgn(win, CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r), 0);
            let y = GetSystemMetrics(SM_CYSCREEN) - (crate::dock::STRIP_H + 24 + H) * scale / 100;
            SetWindowPos(
                win,
                HWND_BOTTOM,
                24 * scale / 100,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            VIS.with(|v| v.borrow_mut().hwnd = win as usize);
        } else {
            InvalidateRect(hwnd as HWND, ptr::null(), 0);
        }
    } else {
        if let Ok(mut g) = STOP.lock() {
            if let Some(f) = g.take() {
                f.store(true, Ordering::Relaxed);
            }
        }
        if hwnd != 0 {
            DestroyWindow(hwnd as HWND);
            VIS.with(|v| v.borrow_mut().hwnd = 0);
        }
    }
}
