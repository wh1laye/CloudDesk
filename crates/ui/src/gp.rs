use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

type Ptr = *mut c_void;

#[repr(C)]
struct StartupInput {
    version: u32,
    debug: usize,
    suppress_thread: i32,
    suppress_codecs: i32,
}

#[repr(C)]
struct PointF {
    x: f32,
    y: f32,
}

#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const StartupInput, output: *mut c_void) -> i32;
    fn GdipCreateFromHDC(hdc: Ptr, g: *mut Ptr) -> i32;
    fn GdipDeleteGraphics(g: Ptr) -> i32;
    fn GdipFlush(g: Ptr, intention: i32) -> i32;
    fn GdipSetSmoothingMode(g: Ptr, mode: i32) -> i32;
    fn GdipSetPixelOffsetMode(g: Ptr, mode: i32) -> i32;
    fn GdipSetInterpolationMode(g: Ptr, mode: i32) -> i32;
    fn GdipCreateSolidFill(argb: u32, brush: *mut Ptr) -> i32;
    fn GdipCreateLineBrush(
        p1: *const PointF,
        p2: *const PointF,
        c1: u32,
        c2: u32,
        wrap: i32,
        brush: *mut Ptr,
    ) -> i32;
    fn GdipDeleteBrush(brush: Ptr) -> i32;
    fn GdipCreatePen1(argb: u32, width: f32, unit: i32, pen: *mut Ptr) -> i32;
    fn GdipDeletePen(pen: Ptr) -> i32;
    fn GdipSetPenStartCap(pen: Ptr, cap: i32) -> i32;
    fn GdipSetPenEndCap(pen: Ptr, cap: i32) -> i32;
    fn GdipSetPenLineJoin(pen: Ptr, join: i32) -> i32;
    fn GdipCreatePath(fill: i32, path: *mut Ptr) -> i32;
    fn GdipDeletePath(path: Ptr) -> i32;
    fn GdipAddPathArc(path: Ptr, x: f32, y: f32, w: f32, h: f32, start: f32, sweep: f32) -> i32;
    fn GdipClosePathFigure(path: Ptr) -> i32;
    fn GdipFillPath(g: Ptr, brush: Ptr, path: Ptr) -> i32;
    fn GdipDrawPath(g: Ptr, pen: Ptr, path: Ptr) -> i32;
    fn GdipFillEllipse(g: Ptr, brush: Ptr, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipDrawEllipse(g: Ptr, pen: Ptr, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipDrawArc(g: Ptr, pen: Ptr, x: f32, y: f32, w: f32, h: f32, start: f32, sweep: f32)
        -> i32;
    fn GdipDrawLine(g: Ptr, pen: Ptr, x1: f32, y1: f32, x2: f32, y2: f32) -> i32;
    fn GdipSetClipPath(g: Ptr, path: Ptr, mode: i32) -> i32;
    fn GdipResetClip(g: Ptr) -> i32;
    fn GdipLoadImageFromFile(file: *const u16, image: *mut Ptr) -> i32;
    fn GdipDisposeImage(image: Ptr) -> i32;
    fn GdipGetImageThumbnail(
        image: Ptr,
        w: u32,
        h: u32,
        thumb: *mut Ptr,
        cb: *const c_void,
        data: *const c_void,
    ) -> i32;
    fn GdipDrawImageRectI(g: Ptr, image: Ptr, x: i32, y: i32, w: i32, h: i32) -> i32;
}

static START: Once = Once::new();
static TOKEN: AtomicUsize = AtomicUsize::new(0);

pub fn init() {
    START.call_once(|| {
        let input = StartupInput {
            version: 1,
            debug: 0,
            suppress_thread: 0,
            suppress_codecs: 0,
        };
        let mut token = 0usize;
        let st = unsafe { GdiplusStartup(&mut token, &input, ptr::null_mut()) };
        if st == 0 {
            TOKEN.store(token, Ordering::Relaxed);
        }
    });
}

pub fn ready() -> bool {
    init();
    TOKEN.load(Ordering::Relaxed) != 0
}

pub fn argb(rgb: (u8, u8, u8), alpha: u8) -> u32 {
    (alpha as u32) << 24 | (rgb.0 as u32) << 16 | (rgb.1 as u32) << 8 | rgb.2 as u32
}

pub struct Gfx {
    g: Ptr,
}

impl Gfx {
    pub unsafe fn new(hdc: *mut c_void) -> Option<Gfx> {
        if !ready() || hdc.is_null() {
            return None;
        }
        let mut g: Ptr = ptr::null_mut();
        let st = unsafe { GdipCreateFromHDC(hdc, &mut g) };
        if st != 0 || g.is_null() {
            return None;
        }
        unsafe {
            GdipSetSmoothingMode(g, 4);
            GdipSetPixelOffsetMode(g, 2);
            GdipSetInterpolationMode(g, 7);
        }
        Some(Gfx { g })
    }

    pub fn flush(&self) {
        unsafe { GdipFlush(self.g, 1) };
    }

    fn solid(&self, color: u32) -> Option<Ptr> {
        let mut b: Ptr = ptr::null_mut();
        if unsafe { GdipCreateSolidFill(color, &mut b) } == 0 && !b.is_null() {
            Some(b)
        } else {
            None
        }
    }

    fn pen(&self, color: u32, width: f32) -> Option<Ptr> {
        let mut p: Ptr = ptr::null_mut();
        if unsafe { GdipCreatePen1(color, width, 0, &mut p) } == 0 && !p.is_null() {
            unsafe {
                GdipSetPenStartCap(p, 2);
                GdipSetPenEndCap(p, 2);
                GdipSetPenLineJoin(p, 2);
            }
            Some(p)
        } else {
            None
        }
    }

    fn round_path(&self, x: f32, y: f32, w: f32, h: f32, d: f32) -> Option<Ptr> {
        let mut p: Ptr = ptr::null_mut();
        if unsafe { GdipCreatePath(0, &mut p) } != 0 || p.is_null() {
            return None;
        }
        let d = d.min(w).min(h).max(0.0);
        unsafe {
            if d < 0.5 {
                GdipAddPathArc(p, x, y, 0.01, 0.01, 180.0, 90.0);
                GdipAddPathArc(p, x + w, y, 0.01, 0.01, 270.0, 90.0);
                GdipAddPathArc(p, x + w, y + h, 0.01, 0.01, 0.0, 90.0);
                GdipAddPathArc(p, x, y + h, 0.01, 0.01, 90.0, 90.0);
            } else {
                GdipAddPathArc(p, x, y, d, d, 180.0, 90.0);
                GdipAddPathArc(p, x + w - d, y, d, d, 270.0, 90.0);
                GdipAddPathArc(p, x + w - d, y + h - d, d, d, 0.0, 90.0);
                GdipAddPathArc(p, x, y + h - d, d, d, 90.0, 90.0);
            }
            GdipClosePathFigure(p);
        }
        Some(p)
    }

    pub fn fill_round(&self, x: f32, y: f32, w: f32, h: f32, d: f32, color: u32) {
        let (Some(b), Some(p)) = (self.solid(color), self.round_path(x, y, w, h, d)) else {
            return;
        };
        unsafe {
            GdipFillPath(self.g, b, p);
            GdipDeletePath(p);
            GdipDeleteBrush(b);
        }
    }

    pub fn fill_round_grad(
        &self,
        r: (f32, f32, f32, f32),
        d: f32,
        top: u32,
        bottom: u32,
    ) {
        let mut b: Ptr = ptr::null_mut();
        let p1 = PointF { x: r.0, y: r.1 };
        let p2 = PointF {
            x: r.0,
            y: r.1 + r.3,
        };
        if unsafe { GdipCreateLineBrush(&p1, &p2, top, bottom, 0, &mut b) } != 0 || b.is_null() {
            return;
        }
        if let Some(p) = self.round_path(r.0, r.1, r.2, r.3, d) {
            unsafe {
                GdipFillPath(self.g, b, p);
                GdipDeletePath(p);
            }
        }
        unsafe { GdipDeleteBrush(b) };
    }

    pub fn stroke_round(&self, r: (f32, f32, f32, f32), d: f32, width: f32, color: u32) {
        let (Some(pen), Some(p)) = (
            self.pen(color, width),
            self.round_path(r.0, r.1, r.2, r.3, d),
        ) else {
            return;
        };
        unsafe {
            GdipDrawPath(self.g, pen, p);
            GdipDeletePath(p);
            GdipDeletePen(pen);
        }
    }

    pub fn fill_dot(&self, cx: f32, cy: f32, r: f32, color: u32) {
        let Some(b) = self.solid(color) else {
            return;
        };
        unsafe {
            GdipFillEllipse(self.g, b, cx - r, cy - r, r * 2.0, r * 2.0);
            GdipDeleteBrush(b);
        }
    }

    pub fn ring(&self, cx: f32, cy: f32, r: f32, width: f32, color: u32) {
        let Some(pen) = self.pen(color, width) else {
            return;
        };
        unsafe {
            GdipDrawEllipse(self.g, pen, cx - r, cy - r, r * 2.0, r * 2.0);
            GdipDeletePen(pen);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn arc(&self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, width: f32, color: u32) {
        let Some(pen) = self.pen(color, width) else {
            return;
        };
        unsafe {
            GdipDrawArc(self.g, pen, cx - r, cy - r, r * 2.0, r * 2.0, start, sweep);
            GdipDeletePen(pen);
        }
    }

    pub fn line(&self, a: (f32, f32), b: (f32, f32), width: f32, color: u32) {
        let Some(pen) = self.pen(color, width) else {
            return;
        };
        unsafe {
            GdipDrawLine(self.g, pen, a.0, a.1, b.0, b.1);
            GdipDeletePen(pen);
        }
    }

    pub fn image_round(&self, img: &Image, r: (i32, i32, i32, i32), d: f32) {
        let Some(p) = self.round_path(r.0 as f32, r.1 as f32, r.2 as f32, r.3 as f32, d) else {
            return;
        };
        unsafe {
            GdipSetClipPath(self.g, p, 0);
            GdipDrawImageRectI(self.g, img.0, r.0, r.1, r.2, r.3);
            GdipResetClip(self.g);
            GdipDeletePath(p);
        }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe { GdipDeleteGraphics(self.g) };
    }
}

pub struct Image(Ptr);

impl Image {
    pub fn load(path: &[u16]) -> Option<Image> {
        if !ready() {
            return None;
        }
        let mut img: Ptr = ptr::null_mut();
        if unsafe { GdipLoadImageFromFile(path.as_ptr(), &mut img) } == 0 && !img.is_null() {
            Some(Image(img))
        } else {
            None
        }
    }

    pub fn thumb(&self, w: u32, h: u32) -> Option<Image> {
        let mut t: Ptr = ptr::null_mut();
        let st = unsafe { GdipGetImageThumbnail(self.0, w, h, &mut t, ptr::null(), ptr::null()) };
        if st == 0 && !t.is_null() {
            Some(Image(t))
        } else {
            None
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { GdipDisposeImage(self.0) };
    }
}
