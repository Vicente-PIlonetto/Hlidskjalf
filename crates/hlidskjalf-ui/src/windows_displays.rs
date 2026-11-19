//! Reads the Windows monitor layout in physical pixels, the same space as `GetCursorPos`.

use crate::portal_edge::ScreenRect;
use windows_sys::Win32::{
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
        MONITORINFO, MONITORINFOEXW,
    },
};

const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DisplayInfo {
    /// Windows device name such as `\\.\DISPLAY2`; it survives reordering, unlike an index.
    pub id: String,
    /// The number Windows shows in Settings › Display.
    pub number: u32,
    pub label: String,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl DisplayInfo {
    pub fn rect(&self) -> ScreenRect {
        ScreenRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end]).trim().to_owned()
}

/// `\\.\DISPLAY12` → 12.
fn device_number(device: &str) -> Option<u32> {
    let digits: String = device
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

fn monitor_name(device: &[u16]) -> Option<String> {
    let mut info: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
    info.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: `device` is the NUL-terminated name from MONITORINFOEXW and `info.cb` is set.
    if unsafe { EnumDisplayDevicesW(device.as_ptr(), 0, &mut info, 0) } == 0 {
        return None;
    }
    let name = wide_to_string(&info.DeviceString);
    (!name.is_empty()).then_some(name)
}

/// Lists the monitors as Windows currently arranges them, in Windows' own numbering.
pub(super) fn enumerate() -> Vec<DisplayInfo> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the Vec passed by `enumerate`, alive until EnumDisplayMonitors returns.
        let found = unsafe { &mut *(data as *mut Vec<DisplayInfo>) };
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: MONITORINFOEXW starts with MONITORINFO and cbSize announces the larger struct.
        if unsafe {
            GetMonitorInfoW(
                monitor,
                &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
            )
        } == 0
        {
            return 1;
        }
        let rect = info.monitorInfo.rcMonitor;
        let id = wide_to_string(&info.szDevice);
        let number = device_number(&id).unwrap_or(found.len() as u32 + 1);
        let label = monitor_name(&info.szDevice).unwrap_or_else(|| id.clone());
        found.push(DisplayInfo {
            id,
            number,
            label,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0) as u32,
            height: (rect.bottom - rect.top).max(0) as u32,
        });
        1
    }
    let mut found: Vec<DisplayInfo> = Vec::new();
    // SAFETY: The callback only writes to `found`, which outlives the enumeration.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            &mut found as *mut Vec<DisplayInfo> as LPARAM,
        )
    };
    found.sort_by_key(|display| (display.number, display.x, display.y));
    found
}

#[cfg(test)]
mod tests {
    use super::device_number;

    #[test]
    fn reads_the_windows_display_number() {
        assert_eq!(device_number(r"\\.\DISPLAY1"), Some(1));
        assert_eq!(device_number(r"\\.\DISPLAY12"), Some(12));
        assert_eq!(device_number("DISPLAY"), None);
    }
}
