use std::fs;
use std::path::{Component, Path, PathBuf};

static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

const MAGIC: &[u8; 4] = b"CDP1";
const MAX_FILES: u32 = 256;
const MAX_FILE: u64 = 256 * 1024 * 1024;

pub fn embedded() -> bool {
    DATA.len() > MAGIC.len()
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("payload is corrupt")?;
        let s = self.buf.get(self.pos..end).ok_or("payload is truncated")?;
        self.pos = end;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }
}

fn safe(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains(':') || name.contains('\\') {
        return Err(format!("unsafe payload entry: {name}"));
    }
    let p = Path::new(name);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("unsafe payload entry: {name}"));
    }
    Ok(p.to_path_buf())
}

pub fn extract(dst: &Path) -> Result<(), String> {
    if !embedded() {
        return Err("installer has no embedded payload".into());
    }
    let mut r = Reader { buf: DATA, pos: 0 };
    if r.take(4)? != MAGIC {
        return Err("payload header is invalid".into());
    }
    let count = r.u32()?;
    if count == 0 || count > MAX_FILES {
        return Err("payload file count is invalid".into());
    }
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| format!("clear staging dir: {e}"))?;
    }
    fs::create_dir_all(dst).map_err(|e| format!("create staging dir: {e}"))?;
    for _ in 0..count {
        let n = r.u16()? as usize;
        let name = String::from_utf8(r.take(n)?.to_vec()).map_err(|_| "payload name is invalid")?;
        let len = r.u64()?;
        if len > MAX_FILE {
            return Err("payload entry is too large".into());
        }
        let body = r.take(len as usize)?;
        let target = dst.join(safe(&name)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
        }
        fs::write(&target, body).map_err(|e| format!("write {name}: {e}"))?;
    }
    Ok(())
}
