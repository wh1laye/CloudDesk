use std::cell::RefCell;
use std::env;
use std::os::windows::process::CommandExt;
use std::process::{id, Child, Command};

const NO_WINDOW: u32 = 0x0800_0000;
const HOST_EXE: &str = "clouddesk-widgets.exe";

thread_local! {
    static CHILD: RefCell<Option<Child>> = const { RefCell::new(None) };
}

fn spawn() -> Option<Child> {
    let exe = env::current_exe().ok()?.with_file_name(HOST_EXE);
    if !exe.is_file() {
        return None;
    }
    Command::new(exe)
        .arg("--parent")
        .arg(id().to_string())
        .creation_flags(NO_WINDOW)
        .spawn()
        .ok()
}

pub fn sync(on: bool) {
    CHILD.with(|c| {
        let mut c = c.borrow_mut();
        let alive = match c.as_mut() {
            Some(ch) => matches!(ch.try_wait(), Ok(None)),
            None => false,
        };
        if !alive {
            *c = None;
        }
        if on {
            if c.is_none() {
                *c = spawn();
            }
        } else if let Some(mut ch) = c.take() {
            let _ = ch.kill();
            let _ = ch.wait();
        }
    });
}
