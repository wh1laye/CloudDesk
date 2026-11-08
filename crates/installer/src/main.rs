use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const APP: &str = "CloudDesk";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudDesk";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const MARKER: &str = ".clouddesk-install";
const MAIN_EXE: &str = "clouddesk.exe";
const TEMP_PREFIX: &str = "clouddesk-uninstall-";
const MIN_BUILD: u32 = 22631;
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Install,
    Update,
    Uninstall,
}

struct Opts {
    mode: Mode,
    silent: bool,
    dir: Option<PathBuf>,
    payload: Option<PathBuf>,
    autostart: bool,
    purge: bool,
    skip_checks: bool,
    log: Option<PathBuf>,
}

#[derive(Default)]
struct State {
    had_install: bool,
    bak: Option<PathBuf>,
    dir_created: bool,
    reg_written: bool,
    run_written: bool,
    reg_backup: Option<PathBuf>,
}

struct Log {
    file: Option<RefCell<fs::File>>,
}

impl Log {
    fn new(path: Option<&Path>) -> Log {
        let target = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| env::temp_dir().join("clouddesk-installer.log"));
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(target)
            .ok()
            .map(RefCell::new);
        Log { file }
    }

    fn say(&self, msg: &str) {
        println!("{msg}");
        if let Some(f) = &self.file {
            let _ = writeln!(f.borrow_mut(), "{msg}");
        }
    }
}

fn ctx<T>(r: io::Result<T>, what: &str) -> Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

fn parse(args: Vec<String>) -> Result<Opts, String> {
    let mut o = Opts {
        mode: Mode::Install,
        silent: false,
        dir: None,
        payload: None,
        autostart: false,
        purge: false,
        skip_checks: false,
        log: None,
    };
    let mut mode_set = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "install" | "update" | "uninstall" if !mode_set => {
                o.mode = match a.as_str() {
                    "install" => Mode::Install,
                    "update" => Mode::Update,
                    _ => Mode::Uninstall,
                };
                mode_set = true;
            }
            "--silent" => o.silent = true,
            "--autostart" => o.autostart = true,
            "--purge" => o.purge = true,
            "--skip-checks" => o.skip_checks = true,
            "--dir" => o.dir = Some(PathBuf::from(it.next().ok_or("--dir needs a value")?)),
            "--payload" => {
                o.payload = Some(PathBuf::from(it.next().ok_or("--payload needs a value")?))
            }
            "--log" => o.log = Some(PathBuf::from(it.next().ok_or("--log needs a value")?)),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(o)
}

fn reg(args: &[&str]) -> Result<String, String> {
    let out = Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| format!("reg: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "reg {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn try_reg(log: &Log, args: &[&str]) {
    if let Err(e) = reg(args) {
        log.say(&format!("warning: {e}"));
    }
}

fn windows_build() -> Result<u32, String> {
    let out = reg(&[
        "query",
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "/v",
        "CurrentBuildNumber",
    ])?;
    out.split_whitespace()
        .last()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| "cannot read windows build".to_string())
}

fn free_bytes(dir: &Path) -> Result<u64, String> {
    let letter = dir
        .to_string_lossy()
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or("cannot determine drive letter")?;
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("(Get-PSDrive -Name {letter}).Free"),
        ])
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|_| "cannot read free disk space".to_string())
}

fn preflight(dir: &Path, size: u64, log: &Log) -> Result<(), String> {
    let build = windows_build()?;
    log.say(&format!("windows build {build}"));
    if build < MIN_BUILD {
        return Err(format!(
            "windows 11 23h2 (build {MIN_BUILD}) or newer required, found {build}"
        ));
    }
    let need = size.saturating_mul(2).saturating_add(64 * 1024 * 1024);
    let free = free_bytes(dir)?;
    if free < need {
        return Err(format!("not enough disk space: need {need}, free {free}"));
    }
    Ok(())
}

fn local_appdata() -> Result<PathBuf, String> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is not set".to_string())
}

fn resolve_dir(o: &Opts) -> Result<PathBuf, String> {
    match &o.dir {
        Some(d) => Ok(d.clone()),
        None => Ok(local_appdata()?.join("Programs").join(APP)),
    }
}

fn lower(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_lowercase())
}

fn check_dir(d: &Path) -> Result<(), String> {
    if !d.is_absolute() {
        return Err("install path must be absolute".into());
    }
    if d.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("install path must not contain '..'".into());
    }
    if d.parent().is_none() || d.components().count() < 3 {
        return Err("install path is too close to the drive root".into());
    }
    let target = lower(d);
    for var in [
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "TEMP",
    ] {
        if let Some(v) = env::var_os(var) {
            if lower(Path::new(&v)).starts_with(&target) {
                return Err(format!("install path may not be or contain %{var}%"));
            }
        }
    }
    Ok(())
}

fn is_empty(d: &Path) -> Result<bool, String> {
    let mut rd = ctx(fs::read_dir(d), "read dir")?;
    Ok(rd.next().is_none())
}

fn dir_size(p: &Path) -> Result<u64, String> {
    let mut total = 0;
    for e in ctx(fs::read_dir(p), "read payload")? {
        let e = ctx(e, "read payload entry")?;
        let ft = ctx(e.file_type(), "file type")?;
        if ft.is_dir() {
            total += dir_size(&e.path())?;
        } else if ft.is_file() {
            total += ctx(e.metadata(), "metadata")?.len();
        }
    }
    Ok(total)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    ctx(fs::create_dir_all(dst), "create dir")?;
    for e in ctx(fs::read_dir(src), "read payload")? {
        let e = ctx(e, "read payload entry")?;
        let ft = ctx(e.file_type(), "file type")?;
        let target = dst.join(e.file_name());
        if ft.is_dir() {
            copy_tree(&e.path(), &target)?;
        } else if ft.is_file() {
            ctx(fs::copy(e.path(), &target), "copy file")?;
        }
    }
    Ok(())
}

fn remove_tree(p: &Path) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..20 {
        match fs::remove_dir_all(p) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last = e.to_string();
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(format!("cannot remove {}: {last}", p.display()))
}

fn confirm(q: &str) -> bool {
    print!("{q} [y/N] ");
    let _ = io::stdout().flush();
    let mut s = String::new();
    io::stdin().read_line(&mut s).is_ok()
        && matches!(s.trim().to_lowercase().as_str(), "y" | "yes")
}

fn bak_path(dir: &Path) -> Result<PathBuf, String> {
    let name = dir
        .file_name()
        .ok_or("install path has no name")?
        .to_string_lossy()
        .into_owned();
    Ok(dir.with_file_name(format!("{name}.bak")))
}

fn apply(
    o: &Opts,
    log: &Log,
    dir: &Path,
    exe: &Path,
    payload: &Path,
    size: u64,
    st: &mut State,
) -> Result<(), String> {
    if st.had_install {
        let backup = temp_file("clouddesk-uninstall-backup.reg");
        let bs = backup.to_string_lossy().into_owned();
        if reg(&["export", UNINSTALL_KEY, bs.as_str(), "/y"]).is_ok() {
            st.reg_backup = Some(backup);
        }
        let bak = bak_path(dir)?;
        if bak.exists() {
            remove_tree(&bak)?;
        }
        ctx(fs::rename(dir, &bak), "move previous installation")?;
        st.bak = Some(bak);
        log.say("previous installation moved aside");
    }
    ctx(fs::create_dir_all(dir), "create install dir")?;
    st.dir_created = true;
    copy_tree(payload, dir)?;
    log.say("payload copied");
    ctx(fs::copy(exe, dir.join("uninstall.exe")), "copy uninstaller")?;
    ctx(fs::write(dir.join(MARKER), VERSION), "write marker")?;

    let d = dir.to_string_lossy().into_owned();
    let un = format!("\"{d}\\uninstall.exe\" uninstall");
    let kb = (size / 1024).to_string();
    st.reg_written = true;
    for (name, kind, data) in [
        ("DisplayName", "REG_SZ", APP),
        ("DisplayVersion", "REG_SZ", VERSION),
        ("InstallLocation", "REG_SZ", d.as_str()),
        ("UninstallString", "REG_SZ", un.as_str()),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
        ("EstimatedSize", "REG_DWORD", kb.as_str()),
    ] {
        reg(&[
            "add",
            UNINSTALL_KEY,
            "/v",
            name,
            "/t",
            kind,
            "/d",
            data,
            "/f",
        ])?;
    }
    log.say("uninstall entry written");

    if o.autostart {
        if dir.join(MAIN_EXE).is_file() {
            let cmd = format!("\"{d}\\{MAIN_EXE}\"");
            st.run_written = true;
            reg(&[
                "add",
                RUN_KEY,
                "/v",
                APP,
                "/t",
                "REG_SZ",
                "/d",
                cmd.as_str(),
                "/f",
            ])?;
            log.say("autostart enabled");
        } else {
            log.say("autostart skipped: main executable not in payload");
        }
    }
    Ok(())
}

fn rollback(dir: &Path, st: &State, log: &Log) {
    if st.run_written {
        try_reg(log, &["delete", RUN_KEY, "/v", APP, "/f"]);
    }
    if st.reg_written {
        match &st.reg_backup {
            Some(b) => {
                let bs = b.to_string_lossy().into_owned();
                try_reg(log, &["import", bs.as_str()]);
            }
            None => try_reg(log, &["delete", UNINSTALL_KEY, "/f"]),
        }
    }
    if st.dir_created {
        if let Err(e) = remove_tree(dir) {
            log.say(&format!("warning: {e}"));
        }
    }
    if let Some(bak) = &st.bak {
        if let Err(e) = fs::rename(bak, dir) {
            log.say(&format!("warning: cannot restore previous installation: {e}"));
        }
    }
}

fn temp_file(name: &str) -> PathBuf {
    env::temp_dir().join(name)
}

fn install(o: &Opts, log: &Log, update: bool) -> Result<(), String> {
    let exe = ctx(env::current_exe(), "current exe")?;
    let payload = match &o.payload {
        Some(p) => p.clone(),
        None => exe
            .parent()
            .map(|p| p.join("payload"))
            .ok_or("cannot locate installer directory")?,
    };
    if !payload.is_dir() {
        return Err(format!("payload not found: {}", payload.display()));
    }
    let dir = resolve_dir(o)?;
    check_dir(&dir)?;
    let had = dir.join(MARKER).is_file();
    if update && !had {
        return Err("no existing installation to update".into());
    }
    if dir.exists() && !had && !is_empty(&dir)? {
        return Err(format!(
            "{} exists and is not a clouddesk installation",
            dir.display()
        ));
    }
    let size = dir_size(&payload)?;
    if !o.skip_checks {
        preflight(&dir, size, log)?;
    }
    let verb = if had { "update" } else { "install" };
    if !o.silent && !confirm(&format!("{verb} {APP} to {}?", dir.display())) {
        return Err("cancelled".into());
    }
    log.say(&format!("{verb}: {}", dir.display()));
    let mut st = State {
        had_install: had,
        ..State::default()
    };
    match apply(o, log, &dir, &exe, &payload, size, &mut st) {
        Ok(()) => {
            if let Some(b) = &st.bak {
                if let Err(e) = remove_tree(b) {
                    log.say(&format!("warning: {e}"));
                }
            }
            log.say("done");
            Ok(())
        }
        Err(e) => {
            log.say(&format!("failed: {e}; rolling back"));
            rollback(&dir, &st, log);
            Err(e)
        }
    }
}

fn registered_location() -> Option<PathBuf> {
    let out = reg(&["query", UNINSTALL_KEY, "/v", "InstallLocation"]).ok()?;
    let line = out.lines().find(|l| l.contains("InstallLocation"))?;
    let value = line.split("REG_SZ").nth(1)?.trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

fn uninstall(o: &Opts, log: &Log) -> Result<(), String> {
    let exe = ctx(env::current_exe(), "current exe")?;
    let exe_dir = exe.parent().map(Path::to_path_buf);
    let dir = match &o.dir {
        Some(d) => d.clone(),
        None => match exe_dir {
            Some(p) if p.join(MARKER).is_file() => p,
            _ => registered_location().ok_or("installation not found")?,
        },
    };
    check_dir(&dir)?;
    if !dir.join(MARKER).is_file() {
        return Err(format!("{} is not a clouddesk installation", dir.display()));
    }
    let mut purge = o.purge;
    if !o.silent {
        if !confirm(&format!("remove {APP} from {}?", dir.display())) {
            return Err("cancelled".into());
        }
        if !purge {
            purge = confirm("also delete user settings?");
        }
    }

    if exe.starts_with(&dir) {
        let tmp = temp_file(&format!("{TEMP_PREFIX}{}.exe", std::process::id()));
        ctx(fs::copy(&exe, &tmp), "copy uninstaller to temp")?;
        let mut cmd = Command::new(&tmp);
        cmd.arg("uninstall").arg("--silent").arg("--dir").arg(&dir);
        if purge {
            cmd.arg("--purge");
        }
        ctx(cmd.spawn(), "relaunch uninstaller")?;
        log.say("uninstall continues in a temporary process");
        return Ok(());
    }

    log.say(&format!("uninstall: {}", dir.display()));
    let _ = Command::new("taskkill")
        .args(["/im", MAIN_EXE, "/f", "/t"])
        .output();
    try_reg(log, &["delete", RUN_KEY, "/v", APP, "/f"]);
    try_reg(log, &["delete", UNINSTALL_KEY, "/f"]);
    remove_tree(&dir)?;
    log.say("files removed");
    if purge {
        if let Some(a) = env::var_os("APPDATA") {
            remove_tree(&PathBuf::from(a).join(APP))?;
            log.say("user settings removed");
        }
    }
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.starts_with(TEMP_PREFIX) && exe.starts_with(env::temp_dir()) {
        let line = format!(
            "/c \"ping -n 3 127.0.0.1 >nul & del /f /q \"{}\"\"",
            exe.display()
        );
        let _ = Command::new("cmd")
            .raw_arg(line)
            .creation_flags(NO_WINDOW)
            .spawn();
    }
    log.say("done");
    Ok(())
}

fn run() -> Result<(), String> {
    let o = parse(env::args().skip(1).collect())?;
    let log = Log::new(o.log.as_deref());
    let r = match o.mode {
        Mode::Install => install(&o, &log, false),
        Mode::Update => install(&o, &log, true),
        Mode::Uninstall => uninstall(&o, &log),
    };
    if let Err(e) = &r {
        log.say(&format!("error: {e}"));
    }
    r
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(_) => 1,
    };
    std::process::exit(code);
}
