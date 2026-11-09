use std::mem::zeroed;

use clouddesk_ui::config::Config;
use clouddesk_ui::gfx::{disc, measure, mix, ring, rrect, stroke, txt, Ctx};
use clouddesk_ui::{now, wall};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

pub const DOT_STEP: i32 = 18;
pub const HEIGHT: i32 = 34;
const GAP: i32 = 18;
const EDGE: i32 = 18;
const MAX_ITEMS: usize = 6;
const TEXT: i32 = 13;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Settings,
    Search,
    Spaces,
    Clock,
    Time,
    Battery,
    Wall,
    Name,
}

pub struct Slot {
    pub kind: Kind,
    pub x: i32,
    pub w: i32,
}

pub struct Layout {
    pub slots: Vec<Slot>,
}

fn kind_of(s: &str) -> Option<Kind> {
    match s.trim() {
        "settings" => Some(Kind::Settings),
        "search" => Some(Kind::Search),
        "spaces" => Some(Kind::Spaces),
        "clock" => Some(Kind::Clock),
        "time" => Some(Kind::Time),
        "battery" => Some(Kind::Battery),
        "wall" => Some(Kind::Wall),
        "name" => Some(Kind::Name),
        _ => None,
    }
}

fn parse(list: &str) -> Vec<Kind> {
    list.split(',')
        .filter_map(kind_of)
        .take(MAX_ITEMS)
        .collect()
}

fn width(k: Kind, spaces: usize) -> i32 {
    match k {
        Kind::Settings | Kind::Search => 20,
        Kind::Spaces => spaces.max(1) as i32 * DOT_STEP,
        Kind::Clock => measure("Wed 28 May", TEXT, false) + 10 + measure("00:00", TEXT, true),
        Kind::Time => measure("00:00", TEXT, true),
        Kind::Battery => 28 + measure("100%", TEXT - 1, false),
        Kind::Wall => 22 + measure("00/00", TEXT - 1, false),
        Kind::Name => 20 + measure("CloudDesk", TEXT, true),
    }
}

pub fn layout(cfg: &Config, lw: i32, spaces: usize) -> Layout {
    let groups = [
        parse(&cfg.bar_left),
        parse(&cfg.bar_center),
        parse(&cfg.bar_right),
    ];
    let mut slots = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        if g.is_empty() {
            continue;
        }
        let inner: i32 =
            g.iter().map(|k| width(*k, spaces)).sum::<i32>() + GAP * (g.len() as i32 - 1);
        let mut x = match gi {
            0 => EDGE,
            1 => (lw - inner) / 2,
            _ => lw - EDGE - inner,
        };
        for k in g {
            let kw = width(*k, spaces);
            slots.push(Slot {
                kind: *k,
                x,
                w: kw,
            });
            x += kw + GAP;
        }
    }
    Layout { slots }
}

pub fn index_at(l: &Layout, x: i32) -> Option<usize> {
    l.slots
        .iter()
        .position(|s| x >= s.x - GAP / 2 && x < s.x + s.w + GAP / 2)
}

pub fn hit(l: &Layout, x: i32) -> Option<(Kind, i32)> {
    index_at(l, x).map(|i| (l.slots[i].kind, x - l.slots[i].x))
}

struct Power {
    percent: Option<u32>,
    charging: bool,
}

fn power() -> Power {
    let mut s: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut s) } != 0;
    if !ok || s.BatteryFlag == 128 || s.BatteryLifePercent > 100 {
        return Power {
            percent: None,
            charging: false,
        };
    }
    Power {
        percent: Some(s.BatteryLifePercent as u32),
        charging: s.ACLineStatus == 1,
    }
}

fn gear(c: &Ctx, cx: f32, cy: f32, col: (u8, u8, u8)) {
    ring(c, cx, cy, 5.0, 1.9, col, 255);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let (sn, cs) = a.sin_cos();
        stroke(
            c,
            (cx + cs * 7.0, cy + sn * 7.0),
            (cx + cs * 8.6, cy + sn * 8.6),
            2.4,
            col,
            255,
        );
    }
}

fn magnifier(c: &Ctx, cx: f32, cy: f32, col: (u8, u8, u8)) {
    ring(c, cx - 1.5, cy - 1.5, 5.6, 1.9, col, 255);
    stroke(c, (cx + 2.6, cy + 2.6), (cx + 7.6, cy + 7.6), 2.2, col, 255);
}

fn battery_glyph(c: &Ctx, x: i32, cy: i32, p: &Power) {
    let frame = c.pal.dim;
    let top = cy - 5;
    rrect(c, (x, top, 21, 11), 4, frame, 90);
    rrect(c, (x + 1, top + 1, 19, 9), 3, c.pal.bg, 255);
    if let Some(v) = p.percent {
        let fill = ((17 * v.min(100)) / 100).max(2) as i32;
        let col = if p.charging {
            (60, 210, 120)
        } else if v <= 20 {
            (235, 70, 70)
        } else {
            c.pal.fg
        };
        rrect(c, (x + 2, top + 2, fill, 7), 2, col, 255);
    }
    rrect(c, (x + 22, cy - 2, 2, 4), 2, frame, 160);
}

fn photo_glyph(c: &Ctx, x: i32, cy: i32, col: (u8, u8, u8)) {
    let top = cy - 6;
    rrect(c, (x, top, 16, 12), 4, col, 255);
    rrect(c, (x + 1, top + 1, 14, 10), 3, c.pal.bg, 255);
    disc(c, x as f32 + 11.0, top as f32 + 4.0, 1.6, col, 255);
    let (fx, fy) = (x as f32, top as f32);
    stroke(c, (fx + 3.0, fy + 9.0), (fx + 6.5, fy + 5.5), 1.6, col, 255);
    stroke(c, (fx + 6.5, fy + 5.5), (fx + 9.5, fy + 8.5), 1.6, col, 255);
}

pub unsafe fn draw(c: &Ctx, l: &Layout, cfg: &Config, st: (usize, usize), hover: Option<usize>) {
    let cy = HEIGHT / 2;
    let p = c.pal;
    let line = mix(p.bg, p.fg, 0.16);
    rrect(c, (0, HEIGHT - 1, 4000, 1), 0, line, 255);
    let t = now();
    for (i, s) in l.slots.iter().enumerate() {
        if hover == Some(i) {
            rrect(c, (s.x - 9, cy - 12, s.w + 18, 24), 24, p.fg, 30);
        }
        let ink = if hover == Some(i) { p.fg } else { mix(p.fg, p.dim, 0.18) };
        match s.kind {
            Kind::Settings => gear(c, s.x as f32 + 10.0, cy as f32, ink),
            Kind::Search => magnifier(c, s.x as f32 + 10.0, cy as f32, ink),
            Kind::Spaces => {
                for i in 0..st.0.max(1) {
                    let cx = (s.x + i as i32 * DOT_STEP + DOT_STEP / 2) as f32;
                    if i + 1 == st.1 {
                        rrect(c, (cx as i32 - 8, cy - 3, 16, 6), 6, p.acc, 255);
                    } else {
                        disc(c, cx, cy as f32, 3.0, p.dim, 200);
                    }
                }
            }
            Kind::Clock => {
                txt(c, &t.date, s.x, cy, TEXT, p.dim, false);
                let dx = measure(&t.date, TEXT, false) + 10;
                txt(c, &t.hm, s.x + dx, cy, TEXT, p.fg, true);
            }
            Kind::Time => txt(c, &t.hm, s.x, cy, TEXT, p.fg, true),
            Kind::Battery => {
                let pw = power();
                battery_glyph(c, s.x, cy, &pw);
                let label = match pw.percent {
                    Some(v) => format!("{v}%"),
                    None => "AC".to_string(),
                };
                txt(c, &label, s.x + 28, cy, TEXT - 1, ink, false);
            }
            Kind::Wall => {
                photo_glyph(c, s.x, cy, ink);
                let n = wall::list().len();
                let label = if n == 0 {
                    "--".to_string()
                } else {
                    format!("{:02}/{:02}", cfg.wall_index.min(n - 1) + 1, n.min(99))
                };
                txt(c, &label, s.x + 22, cy, TEXT - 1, ink, false);
            }
            Kind::Name => {
                disc(c, s.x as f32 + 6.0, cy as f32, 5.5, p.acc, 255);
                disc(c, s.x as f32 + 6.0, cy as f32, 2.2, p.bg, 255);
                txt(c, "CloudDesk", s.x + 20, cy, TEXT, p.fg, true);
            }
        }
    }
}
