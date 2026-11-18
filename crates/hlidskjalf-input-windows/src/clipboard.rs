//! Windows Unicode text clipboard for the receiver daemon.

use async_trait::async_trait;
use hlidskjalf_input::{ClipboardProvider, InputError};
use windows_sys::Win32::{
    Foundation::GlobalFree,
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
            OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
    },
};

const CF_UNICODETEXT: u32 = 13;
/// Matches the 16 KiB text limit used by the sender.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

/// Reads and writes plain text on the Windows clipboard.
#[derive(Default)]
pub struct WindowsClipboard;

impl WindowsClipboard {
    /// Changes whenever any application updates the clipboard.
    pub fn sequence() -> u32 {
        // SAFETY: GetClipboardSequenceNumber has no preconditions.
        unsafe { GetClipboardSequenceNumber() }
    }
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Result<Self, InputError> {
        // SAFETY: A null owner is permitted for reading and writing the current clipboard.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
            return Err(InputError::Backend("clipboard unavailable".into()));
        }
        Ok(Self)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: The clipboard was opened exactly once by ClipboardGuard::open.
        unsafe { CloseClipboard() };
    }
}

fn read() -> Result<Option<String>, InputError> {
    let _guard = ClipboardGuard::open()?;
    // SAFETY: The clipboard stays open while the global handle is locked and copied.
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            return Ok(None);
        }
        let bytes = GlobalSize(handle);
        if bytes == 0 || bytes > MAX_TEXT_BYTES * 2 + 2 {
            return Ok(None);
        }
        let pointer = GlobalLock(handle) as *const u16;
        if pointer.is_null() {
            return Ok(None);
        }
        let units = std::slice::from_raw_parts(pointer, bytes / 2);
        let end = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        let text = String::from_utf16(&units[..end]).ok();
        GlobalUnlock(handle);
        Ok(text.filter(|text| text.len() <= MAX_TEXT_BYTES))
    }
}

fn write(text: &str) -> Result<(), InputError> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let _guard = ClipboardGuard::open()?;
    // SAFETY: The movable allocation is sized for `wide`; the clipboard owns it only after SetClipboardData succeeds.
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        if handle.is_null() {
            return Err(InputError::Backend("clipboard allocation failed".into()));
        }
        let pointer = GlobalLock(handle) as *mut u16;
        if pointer.is_null() {
            GlobalFree(handle);
            return Err(InputError::Backend("clipboard lock failed".into()));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
        GlobalUnlock(handle);
        if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, handle).is_null() {
            GlobalFree(handle);
            return Err(InputError::Backend("clipboard update failed".into()));
        }
    }
    Ok(())
}

#[async_trait]
impl ClipboardProvider for WindowsClipboard {
    async fn read_text(&mut self) -> Result<Option<String>, InputError> {
        tokio::task::spawn_blocking(read)
            .await
            .map_err(|_| InputError::Backend("clipboard task failed".into()))?
    }

    async fn write_text(&mut self, text: String) -> Result<(), InputError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(InputError::Unsupported(
                "clipboard text is too large".into(),
            ));
        }
        tokio::task::spawn_blocking(move || write(&text))
            .await
            .map_err(|_| InputError::Backend("clipboard task failed".into()))?
    }
}
