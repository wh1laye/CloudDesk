use std::mem::zeroed;

use clouddesk_ui::config::Config;
use clouddesk_ui::gfx::{dot, pill, surface, text, tw, Ctx};
use clouddesk_ui::{now, wall};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

pub const PILL_Y: i32 = 3;
pub const PILL_H: i32 = 30;
pub const DOT_STEP: i32 = 18;
const PAD: i32 = 12;
const GAP: i32 = 10;
const EDGE: i32 = 8;
const MAX_ITEMS: usize = 6;

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
    pub pills: Vec<(i32, i32)>,
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
        Kind::Settings | Kind::Search => 28,
        Kind::Spaces => spaces.max(1) as i32 * DOT_STEP,
        Kind::Clock => tw("Wed 28 May 00:00", 3),
        Kind::Time => tw("00:00", 3),
        Kind::Battery => tw("100%", 3),
        Kind::Wall => tw("Wall 00/00", 3),
        Kind::Name => tw("CloudDesk", 3),
    }
}

pub fn layout(cfg: &Config, lw: i32, spaces: usize) -> Layout {
    let groups = [
        parse(&cfg.bar_left),
        parse(&cfg.bar_center),
        parse(&cfg.bar_right),
    ];
    let mut pills = Vec::new();
    let mut slots = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        if g.is_empty() {
            continue;
        }
        let inner: i32 =
            g.iter().map(|k| width(*k, spaces)).sum::<i32>() + GAP * (g.len() as i32 - 1);
        let w = inner + PAD * 2;
        let x0 = match gi {
            0 => EDGE,
            1 => (lw - w) / 2,
            _ => lw - EDGE - w,
        };
        pills.push((x0, w));
        let mut x = x0 + PAD;
        for k in g {
            let kw = width(*k, spaces);
            slots.push(Slot { kind: *k, x, w: kw });
            x += kw + GAP;
        }
    }
    Layout { pills, slots }
}

pub fn hit(l: &Layout, x: i32) -> Option<(Kind, i32)> {
    l.slots
        .iter()
        .find(|s| x >= s.x - GAP / 2 && x < s.x + s.w + GAP / 2)
        .map(|s| (s.kind, x - s.x))
}

fn battery() -> String {
    let mut s: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0
        || s.BatteryFlag == 128
        || s.BatteryLifePercent > 100
    {
        return "AC".to_string();
    }
    format!("{}%", s.BatteryLifePercent)
}

pub unsafe fn draw(c: &Ctx, l: &Layout, cfg: &Config, st: (usize, usize)) {
    let cy = PILL_Y + PILL_H / 2;
    for (x, w) in &l.pills {
        pill(c, *x, PILL_Y, *w, PILL_H, surface(c));
    }
    let t = now();
    for s in &l.slots {
        match s.kind {
            Kind::Settings => {
                let cx = s.x + s.w / 2;
                dot(c, c.fg, cx, cy, 6);
                dot(c, surface(c), cx, cy, 3);
                for (dx, dy) in [(0, -9), (0, 9), (-9, 0), (9, 0)] {
                    dot(c, c.fg, cx + dx, cy + dy, 2);
                }
            }
            Kind::Search => {
                let cx = s.x + 12;
                let ly = cy - 3;
                dot(c, c.fg, cx, ly, 7);
                dot(c, surface(c), cx, ly, 4);
                dot(c, c.fg, cx + 8, ly + 8, 2);
                dot(c, c.fg, cx + 11, ly + 11, 2);
            }
            Kind::Spaces => {
                for i in 0..st.0.max(1) {
                    let cx = s.x + i as i32 * DOT_STEP + DOT_STEP / 2;
                    if i + 1 == st.1 {
                        dot(c, c.acc, cx, cy, 5);
                    } else {
                        dot(c, c.dim, cx, cy, 3);
                    }
                }
            }
            Kind::Clock => {
                let stamp = format!("{} {}", t.date, t.hm);
                text(c, &stamp, s.x, cy - 10, 3, c.fg);
            }
            Kind::Time => text(c, &t.hm, s.x, cy - 10, 3, c.fg),
            Kind::Battery => text(c, &battery(), s.x, cy - 10, 3, c.fg),
            Kind::Wall => {
                let n = wall::list().len();
                let label = if n == 0 {
                    "No wall".to_string()
                } else {
                    format!("Wall {:02}/{:02}", cfg.wall_index.min(n - 1) + 1, n.min(99))
                };
                text(c, &label, s.x, cy - 10, 3, c.dim);
            }
            Kind::Name => {
                text(c, "CloudDesk", s.x, cy - 10, 3, c.fg);
            }
        }
    }
}
