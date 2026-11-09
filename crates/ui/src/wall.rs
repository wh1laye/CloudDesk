use std::ffi::c_void;
use std::env;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETDESKWALLPAPER,
    SPI_SETDESKWALLPAPER,
};

use crate::config::{dir as config_dir, Config};

const W: i32 = 1920;
const H: i32 = 1080;
const PRESETS: [&str; 5] = ["azure", "petal", "dusk", "night", "aurora"];
const SYSTEM_LIMIT: usize = 24;

pub fn dir() -> PathBuf {
    config_dir().join("wallpapers")
}

fn is_image(p: &Path) -> bool {
    p.extension()
        .map(|x| {
            matches!(
                x.to_string_lossy().to_lowercase().as_str(),
                "bmp" | "jpg" | "jpeg" | "png"
            )
        })
        .unwrap_or(false)
}

pub fn own() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| is_image(p))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn system_root() -> PathBuf {
    env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
        .join("Web")
        .join("Wallpaper")
}

pub fn system() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    let Ok(top) = fs::read_dir(system_root()) else {
        return v;
    };
    for e in top.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            if let Ok(inner) = fs::read_dir(&p) {
                v.extend(inner.filter_map(|e| e.ok()).map(|e| e.path()).filter(|q| is_image(q)));
            }
        } else if is_image(&p) {
            v.push(p);
        }
    }
    v.sort();
    v.truncate(SYSTEM_LIMIT);
    v
}

pub fn is_system(p: &Path) -> bool {
    p.starts_with(system_root())
}

pub fn list() -> Vec<PathBuf> {
    let mut v = own();
    v.extend(system());
    v
}

pub fn apply(p: &Path) -> Result<(), String> {
    let w: Vec<u16> = p
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            w.as_ptr() as *mut c_void,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err("cannot set wallpaper".into())
    }
}

pub fn current() -> String {
    let mut b = [0u16; 520];
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            b.len() as u32,
            b.as_mut_ptr() as *mut c_void,
            0,
        )
    };
    if ok == 0 {
        return String::new();
    }
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

pub fn pick(c: &mut Config, i: usize) -> Result<(), String> {
    ensure()?;
    if c.wall_original.is_none() {
        c.wall_original = Some(current());
        c.save()?;
    }
    let l = list();
    if l.is_empty() {
        return Err("no wallpapers".into());
    }
    let i = i.min(l.len() - 1);
    apply(&l[i])?;
    c.wall_index = i;
    c.save()
}

pub fn step(c: &mut Config, d: i32) -> Result<(), String> {
    ensure()?;
    let n = list().len() as i32;
    if n == 0 {
        return Err("no wallpapers".into());
    }
    let i = ((c.wall_index as i32 + d) % n + n) % n;
    pick(c, i as usize)
}

pub fn restore(c: &Config) -> Result<(), String> {
    match &c.wall_original {
        Some(o) => apply(Path::new(o)),
        None => Ok(()),
    }
}

pub fn ensure() -> Result<(), String> {
    fs::create_dir_all(dir()).map_err(|e| format!("wallpaper dir: {e}"))?;
    for (i, name) in PRESETS.iter().enumerate() {
        let p = dir().join(format!("{name}.bmp"));
        if !p.exists() {
            fs::write(&p, bmp(i as u8)).map_err(|e| format!("wallpaper write: {e}"))?;
        }
    }
    Ok(())
}

fn bmp(kind: u8) -> Vec<u8> {
    let size = (W * H * 3) as usize;
    let mut v = Vec::with_capacity(54 + size);
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&((54 + size) as u32).to_le_bytes());
    v.extend_from_slice(&[0; 4]);
    v.extend_from_slice(&54u32.to_le_bytes());
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&W.to_le_bytes());
    v.extend_from_slice(&H.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(&[0; 24]);
    for y in (0..H).rev() {
        for x in 0..W {
            let (r, g, b) = px(kind, x, y);
            v.push(b);
            v.push(g);
            v.push(r);
        }
    }
    v
}

type Rgb = (f32, f32, f32);

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn noise(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 255) as f32 / 255.0 - 0.5
}

struct Look {
    top: Rgb,
    bottom: Rgb,
    glow: Rgb,
    glow_at: (f32, f32),
    glow_size: f32,
    glow_power: f32,
    ribbon: Rgb,
    ribbon_power: f32,
}

fn look(kind: u8) -> Look {
    match kind {
        0 => Look {
            top: (150.0, 205.0, 255.0),
            bottom: (8.0, 70.0, 190.0),
            glow: (235.0, 248.0, 255.0),
            glow_at: (0.78, 0.18),
            glow_size: 0.34,
            glow_power: 0.9,
            ribbon: (255.0, 255.0, 255.0),
            ribbon_power: 0.22,
        },
        1 => Look {
            top: (226.0, 238.0, 252.0),
            bottom: (170.0, 205.0, 245.0),
            glow: (255.0, 255.0, 255.0),
            glow_at: (0.5, 0.5),
            glow_size: 0.4,
            glow_power: 0.8,
            ribbon: (20.0, 96.0, 220.0),
            ribbon_power: 0.0,
        },
        2 => Look {
            top: (20.0, 24.0, 70.0),
            bottom: (96.0, 58.0, 170.0),
            glow: (255.0, 140.0, 170.0),
            glow_at: (0.3, 1.05),
            glow_size: 0.5,
            glow_power: 0.75,
            ribbon: (170.0, 150.0, 255.0),
            ribbon_power: 0.16,
        },
        3 => Look {
            top: (4.0, 8.0, 20.0),
            bottom: (10.0, 28.0, 64.0),
            glow: (40.0, 110.0, 220.0),
            glow_at: (0.7, 0.9),
            glow_size: 0.42,
            glow_power: 0.55,
            ribbon: (80.0, 150.0, 255.0),
            ribbon_power: 0.08,
        },
        _ => Look {
            top: (4.0, 22.0, 40.0),
            bottom: (12.0, 70.0, 96.0),
            glow: (60.0, 220.0, 190.0),
            glow_at: (0.2, 0.3),
            glow_size: 0.4,
            glow_power: 0.7,
            ribbon: (130.0, 120.0, 255.0),
            ribbon_power: 0.28,
        },
    }
}

fn px(kind: u8, x: i32, y: i32) -> (u8, u8, u8) {
    let l = look(kind);
    let u = x as f32 / W as f32;
    let v = y as f32 / H as f32;
    let aspect = W as f32 / H as f32;
    let mut c = lerp(l.top, l.bottom, v * 0.75 + u * 0.25);
    let gx = (u - l.glow_at.0) * aspect;
    let gy = v - l.glow_at.1;
    let g = (-(gx * gx + gy * gy) / (2.0 * l.glow_size * l.glow_size)).exp();
    c = lerp(c, l.glow, g * l.glow_power);
    if kind == 1 {
        let dx = (u - 0.5) * aspect;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        let a = dy.atan2(dx);
        let petal = 0.5 + 0.5 * (5.0 * a + r * 7.0).cos();
        let fall = (-r * 2.4).exp();
        let tone = lerp((120.0, 170.0, 245.0), (30.0, 100.0, 225.0), petal);
        c = lerp(c, tone, fall * (0.35 + 0.5 * petal));
        let core = (-(r * r) / 0.012).exp();
        c = lerp(c, (255.0, 255.0, 255.0), core * 0.9);
    } else {
        for i in 0..3 {
            let k = i as f32;
            let w = (u * (2.2 + k * 0.9) + v * (3.4 - k * 0.8) + k * 1.7).sin();
            let band = (0.5 + 0.5 * w).powi(8);
            let fade = 0.35 + 0.65 * (1.0 - (v - 0.5).abs() * 1.4).max(0.0);
            c = lerp(c, l.ribbon, band * l.ribbon_power * fade);
        }
    }
    let n = noise(x, y) * 2.2;
    let f = |t: f32| (t + n).clamp(0.0, 255.0) as u8;
    (f(c.0), f(c.1), f(c.2))
}
