//! A short gold glow along the portal edge when the cursor crosses it.
//!
//! The glow is a layered, click-through, topmost popup drawn with per-pixel alpha, so it never
//! takes focus or input from the window underneath. Windows' "Animation effects" setting off
//! skips it.

use std::{ptr, sync::OnceLock, thread, time::Duration, time::Instant};

use windows_sys::{
    core::w,
    Win32::{
        Foundation::{HWND, POINT, SIZE},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
            AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
            DIB_RGB_COLORS,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
            RegisterClassW, ShowWindow, SystemParametersInfoW, TranslateMessage,
            UpdateLayeredWindow, MSG, PM_REMOVE, SPI_GETCLIENTAREAANIMATION, SW_SHOWNOACTIVATE,
            ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
            WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
};

use crate::portal_edge::{Portal, Side};

/// Draupnir gold, the same accent as the app (`--accent` in tokens.css).
const GOLD: (f32, f32, f32) = (214.0, 168.0, 74.0);
/// Brighter core right at the edge.
const CORE: (f32, f32, f32) = (255.0, 232.0, 170.0);
/// How far the glow reaches into the monitor, in physical pixels.
const DEPTH: i32 = 72;
const DURATION: Duration = Duration::from_millis(560);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// The cursor left this computer through the portal: the glow is drawn into the edge.
    Leave,
    /// Control came back: the glow spreads out of the edge.
    Return,
}

/// Plays the glow on its own thread; returns immediately.
pub fn show(portal: Portal, direction: Direction) {
    if !animations_enabled() {
        return;
    }
    thread::spawn(move || {
        let _ = play(portal, direction);
    });
}

fn animations_enabled() -> bool {
    let mut enabled: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL to the provided pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            &mut enabled as *mut i32 as *mut core::ffi::c_void,
            0,
        )
    };
    ok == 0 || enabled != 0
}

fn window_class() -> Option<*const u16> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let class = w!("HlidskjalfPortalGlow");
    let ok = *REGISTERED.get_or_init(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            // SAFETY: A null module name returns this executable's handle.
            hInstance: unsafe { GetModuleHandleW(ptr::null()) },
            lpszClassName: class,
            ..unsafe { std::mem::zeroed() }
        };
        // SAFETY: `wc` is fully initialized and the class name is a static wide string.
        unsafe { RegisterClassW(&wc) != 0 }
    });
    ok.then_some(class)
}

/// The glow rectangle: the portal segment, extended `DEPTH` pixels into the monitor.
fn bounds(portal: &Portal) -> Option<(i32, i32, i32, i32)> {
    let (x, y, w, h) = portal.segment();
    let rect = match portal.side {
        Side::Left => (x, y, DEPTH, h),
        Side::Right => (x - DEPTH, y, DEPTH, h),
        Side::Top => (x, y, w, DEPTH),
        Side::Bottom => (x, y - DEPTH, w, DEPTH),
    };
    (rect.2 > 0 && rect.3 > 0).then_some(rect)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fills `pixels` (premultiplied BGRA, top-down) for animation progress `t` in 0..=1.
fn paint(pixels: &mut [u32], width: i32, height: i32, side: Side, direction: Direction, t: f32) {
    // Leave: flare fast, then the glow is pulled into the edge. Return: it spreads and fades.
    let (strength, reach) = match direction {
        Direction::Leave => (
            smoothstep(t / 0.18) * (1.0 - smoothstep((t - 0.25) / 0.75)),
            1.0 - 0.75 * smoothstep(t),
        ),
        Direction::Return => (
            1.0 - smoothstep((t - 0.15) / 0.85),
            0.35 + 0.65 * smoothstep(t / 0.7),
        ),
    };
    let reach = (DEPTH as f32 * reach).max(1.0);
    let length = if side.vertical() { height } else { width } as f32;
    let taper_span = (length * 0.12).clamp(8.0, 160.0);
    for row in 0..height {
        for col in 0..width {
            let (distance, along) = match side {
                Side::Left => (col, row),
                Side::Right => (width - 1 - col, row),
                Side::Top => (row, col),
                Side::Bottom => (height - 1 - row, col),
            };
            let along = along as f32;
            let taper = smoothstep(along.min(length - 1.0 - along) / taper_span);
            let falloff = (1.0 - distance as f32 / reach).max(0.0);
            let glow = falloff * falloff;
            let core = (1.0 - distance as f32 / 3.0).max(0.0);
            let alpha = ((glow * 0.85 + core * 0.15).min(1.0) * taper * strength).clamp(0.0, 1.0);
            let mix = core * 0.8;
            let channel = |base: f32, hot: f32| ((base + (hot - base) * mix) * alpha) as u32;
            pixels[(row * width + col) as usize] = ((alpha * 255.0) as u32) << 24
                | channel(GOLD.0, CORE.0) << 16
                | channel(GOLD.1, CORE.1) << 8
                | channel(GOLD.2, CORE.2);
        }
    }
}

struct Gdi {
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    old: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
}

impl Drop for Gdi {
    fn drop(&mut self) {
        // SAFETY: These handles were created by `play` and are released exactly once.
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

struct Window(HWND);

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: The window belongs to this thread and is destroyed once.
        unsafe { DestroyWindow(self.0) };
    }
}

fn play(portal: Portal, direction: Direction) -> Option<()> {
    let class = window_class()?;
    let (x, y, width, height) = bounds(&portal)?;
    // SAFETY: The class is registered; the window is click-through and never activated.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!(""),
            WS_POPUP,
            x,
            y,
            width,
            height,
            ptr::null_mut(),
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        return None;
    }
    let window = Window(hwnd);

    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { std::mem::zeroed() }
    };
    let mut bits: *mut core::ffi::c_void = ptr::null_mut();
    // SAFETY: A memory DC compatible with the screen; the DIB is 32-bit top-down.
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
    if dc.is_null() {
        return None;
    }
    let bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0) };
    if bitmap.is_null() || bits.is_null() {
        unsafe { DeleteDC(dc) };
        return None;
    }
    let old = unsafe { SelectObject(dc, bitmap) };
    let gdi = Gdi { dc, bitmap, old };
    // SAFETY: The DIB holds width * height 32-bit pixels and lives as long as `gdi`.
    let pixels =
        unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize) };

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let origin = POINT { x, y };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT { x: 0, y: 0 };
    let started = Instant::now();
    let mut shown = false;
    loop {
        let t = started.elapsed().as_secs_f32() / DURATION.as_secs_f32();
        paint(pixels, width, height, portal.side, direction, t.min(1.0));
        // SAFETY: All pointers reference live locals; the DC holds the painted DIB.
        unsafe {
            UpdateLayeredWindow(
                window.0,
                ptr::null_mut(),
                &origin,
                &size,
                gdi.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
        }
        if !shown {
            unsafe { ShowWindow(window.0, SW_SHOWNOACTIVATE) };
            shown = true;
        }
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: Drains this thread's queue so the window stays responsive.
        while unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if t >= 1.0 {
            break;
        }
        thread::sleep(Duration::from_millis(16));
    }
    drop(gdi);
    drop(window);
    Some(())
}
