use std::ffi::c_void;
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
const P: i32 = 24;

pub fn dir() -> PathBuf {
    config_dir().join("wallpapers")
}

pub fn list() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| {
                            matches!(
                                x.to_string_lossy().to_lowercase().as_str(),
                                "bmp" | "jpg" | "jpeg" | "png"
                            )
                        })
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
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

pub fn step(c: &mut Config, d: i32) -> Result<(), String> {
    ensure()?;
    if c.wall_original.is_none() {
        c.wall_original = Some(current());
        c.save()?;
    }
    let l = list();
    if l.is_empty() {
        return Err("no wallpapers".into());
    }
    let n = l.len() as i32;
    let i = ((c.wall_index as i32 + d) % n + n) % n;
    c.wall_index = i as usize;
    apply(&l[i as usize])?;
    c.save()
}

pub fn restore(c: &Config) -> Result<(), String> {
    match &c.wall_original {
        Some(o) => apply(Path::new(o)),
        None => Ok(()),
    }
}

pub fn ensure() -> Result<(), String> {
    fs::create_dir_all(dir()).map_err(|e| format!("wallpaper dir: {e}"))?;
    if list().is_empty() {
        for (i, name) in ["dots", "waves", "rings"].iter().enumerate() {
            fs::write(dir().join(format!("{name}.bmp")), bmp(i as u8))
                .map_err(|e| format!("wallpaper write: {e}"))?;
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

fn px(kind: u8, x: i32, y: i32) -> (u8, u8, u8) {
    let cx = (x / P) * P + P / 2;
    let cy = (y / P) * P + P / 2;
    let dx = (x - cx) as f32;
    let dy = (y - cy) as f32;
    let d = (dx * dx + dy * dy).sqrt();
    let fx = cx as f32 - 960.0;
    let fy = cy as f32 - 540.0;
    let r = (fx * fx + fy * fy).sqrt();
    let (t, hot) = match kind {
        0 => ((1.0 - r / 1100.0).max(0.0), (cx / P + cy / P) % 29 == 0),
        1 => (
            0.5 + 0.5 * (cx as f32 * 0.008 + cy as f32 * 0.017).sin(),
            (cx / P) % 31 == 0 && (cy / P) % 5 == 0,
        ),
        _ => {
            let q = 0.5 + 0.5 * (r * 0.035).cos();
            (q * q, false)
        }
    };
    if d >= 1.5 + 9.0 * t {
        return (8, 8, 8);
    }
    let v = (50.0 + 150.0 * t).min(255.0) as u8;
    if hot {
        (226, 38, 45)
    } else if kind == 2 {
        (v, (v as f32 * 0.25) as u8, (v as f32 * 0.28) as u8)
    } else {
        (v, v, v)
    }
}
