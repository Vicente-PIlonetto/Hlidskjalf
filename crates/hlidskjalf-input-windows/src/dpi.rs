//! Process DPI awareness shared by the capture and injection processes.

use windows_sys::Win32::UI::{
    HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    WindowsAndMessaging::SetProcessDPIAware,
};

/// Makes cursor and monitor coordinates physical pixels on every monitor.
///
/// System-wide awareness (`SetProcessDPIAware`) still scales coordinates on monitors whose
/// scale differs from the primary one, which puts edges and the capture center in the wrong
/// place. The desktop window uses per-monitor awareness, so every process must agree with it.
pub fn become_per_monitor_aware() {
    // SAFETY: Both calls only change this process's DPI awareness; failure leaves it unchanged.
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            SetProcessDPIAware();
        }
    }
}
