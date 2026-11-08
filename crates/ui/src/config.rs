use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const KEYS: [&str; 8] = [
    "desktop",
    "island",
    "search",
    "visualizer",
    "themes",
    "music",
    "widgets",
    "lockscreen",
];
pub const THEMES: [&str; 4] = ["grey", "burgundy", "black", "light"];

#[derive(Clone)]
pub struct Config {
    pub comps: [bool; 8],
    pub theme: String,
    pub contour: bool,
    pub wall_index: usize,
    pub wall_original: Option<String>,
    pub dock: Vec<(String, String)>,
}

pub fn dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("CloudDesk")
}

fn default_dock() -> Vec<(String, String)> {
    let settings = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("clouddesk-settings.exe")))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clouddesk-settings.exe".to_string());
    vec![
        ("FILES".into(), "explorer.exe".into()),
        ("SETTINGS".into(), settings),
        ("WINDOWS".into(), "ms-settings:".into()),
    ]
}

fn clean(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\'' | '\n' | '\r')).collect()
}

impl Config {
    pub fn new() -> Config {
        Config {
            comps: [true; 8],
            theme: "black".into(),
            contour: false,
            wall_index: 0,
            wall_original: None,
            dock: default_dock(),
        }
    }

    pub fn load() -> Config {
        let mut c = Config::new();
        let text = match fs::read_to_string(dir().join("config.toml")) {
            Ok(t) => t,
            Err(_) => return c,
        };
        let mut sec = String::new();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if l.starts_with('[') && l.ends_with(']') {
                sec = l[1..l.len() - 1].to_string();
                continue;
            }
            let Some((k, v)) = l.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim().trim_matches(|ch| ch == '\'' || ch == '"');
            match (sec.as_str(), k) {
                ("components", _) => {
                    if let Some(i) = KEYS.iter().position(|x| *x == k) {
                        c.comps[i] = v == "true";
                    }
                }
                ("theme", "name") if THEMES.contains(&v) => c.theme = v.to_string(),
                ("visualizer", "mode") => c.contour = v == "contour",
                ("wallpaper", "index") => c.wall_index = v.parse().unwrap_or(0),
                ("wallpaper", "original") => c.wall_original = Some(v.to_string()),
                ("dock", "items") => {
                    let items: Vec<(String, String)> = v
                        .split(';')
                        .filter_map(|e| e.split_once('|'))
                        .map(|(a, b)| (a.to_string(), b.to_string()))
                        .collect();
                    if !items.is_empty() {
                        c.dock = items;
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) -> Result<(), String> {
        let d = dir();
        fs::create_dir_all(&d).map_err(|e| format!("config dir: {e}"))?;
        let mut s = String::from("[components]\n");
        for (k, v) in KEYS.iter().zip(self.comps.iter()) {
            s.push_str(&format!("{k} = {v}\n"));
        }
        s.push_str(&format!("\n[theme]\nname = '{}'\n", clean(&self.theme)));
        s.push_str(&format!(
            "\n[visualizer]\nmode = '{}'\n",
            if self.contour { "contour" } else { "dots" }
        ));
        s.push_str(&format!("\n[wallpaper]\nindex = {}\n", self.wall_index));
        if let Some(o) = &self.wall_original {
            s.push_str(&format!("original = '{}'\n", clean(o)));
        }
        let items: Vec<String> = self
            .dock
            .iter()
            .map(|(a, b)| {
                format!("{}|{}", clean(a).replace([';', '|'], ""), clean(b).replace(';', ""))
            })
            .collect();
        s.push_str(&format!("\n[dock]\nitems = '{}'\n", items.join(";")));
        let tmp = d.join("config.toml.tmp");
        fs::write(&tmp, s).map_err(|e| format!("config write: {e}"))?;
        fs::rename(&tmp, d.join("config.toml")).map_err(|e| format!("config rename: {e}"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::new()
    }
}

pub fn file_name(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
