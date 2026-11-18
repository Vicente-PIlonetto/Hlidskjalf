//! Windows input injection through `SendInput`, used when Windows is the controlled computer.

use std::collections::BTreeSet;

use async_trait::async_trait;
use hlidskjalf_input::{keycode, InputError, InputInjector};
use hlidskjalf_proto::{Edge, EnterScreen, Message};
use windows_sys::Win32::{
    Foundation::{LPARAM, POINT, RECT},
    Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONULL,
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE,
            KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
            KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
            MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
            MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
        },
        WindowsAndMessaging::{
            GetCursorPos, GetSystemMetrics, SetProcessDPIAware, SM_CXVIRTUALSCREEN,
            SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    },
};

use crate::{
    edge::{entry_monitor, entry_point, toward_sender, Rect},
    keymap::{compose, dead_key, dead_key_spacing, unicode_character, virtual_key, VirtualKey},
};

const MONITORINFOF_PRIMARY: u32 = 1;
const WHEEL_DELTA: i32 = 120;

/// Injects remote mouse and keyboard events into the interactive Windows session.
pub struct WindowsInjector {
    active_edge: Option<Edge>,
    return_requested: bool,
    pressed_keys: BTreeSet<(u16, bool)>,
    pressed_scancodes: BTreeSet<u32>,
    pressed_buttons: BTreeSet<u8>,
    transient_keys: BTreeSet<u32>,
    pending_dead: Option<u32>,
    shift: BTreeSet<u32>,
}

impl WindowsInjector {
    pub fn new() -> Result<Self, InputError> {
        // SAFETY: Marks this process DPI aware so cursor and monitor coordinates are physical pixels.
        unsafe { SetProcessDPIAware() };
        if monitors().is_empty() {
            return Err(InputError::Backend(
                "no Windows display is available".into(),
            ));
        }
        Ok(Self {
            active_edge: None,
            return_requested: false,
            pressed_keys: BTreeSet::new(),
            pressed_scancodes: BTreeSet::new(),
            pressed_buttons: BTreeSet::new(),
            transient_keys: BTreeSet::new(),
            pending_dead: None,
            shift: BTreeSet::new(),
        })
    }

    /// Reports, once, that the cursor crossed back through the portal.
    pub fn take_return_requested(&mut self) -> bool {
        std::mem::take(&mut self.return_requested)
    }

    fn reset(&mut self) {
        self.release_all();
        self.active_edge = None;
        self.return_requested = false;
        self.transient_keys.clear();
        self.pending_dead = None;
        self.shift.clear();
    }

    /// Never leaves a key or button held on this computer after control returns.
    fn release_all(&mut self) {
        for scancode in std::mem::take(&mut self.pressed_scancodes)
            .into_iter()
            .rev()
        {
            let _ = send_scancode(scancode, false);
        }
        for (vk, extended) in std::mem::take(&mut self.pressed_keys).into_iter().rev() {
            let _ = send_key(VirtualKey { vk, extended }, false);
        }
        for button in std::mem::take(&mut self.pressed_buttons) {
            if let Some(flags) = button_flags(button, false) {
                let _ = send_mouse(0, 0, flags.0, flags.1);
            }
        }
    }

    fn enter(&mut self, entry: EnterScreen) -> Result<(), InputError> {
        self.reset();
        self.active_edge = Some(entry.edge);
        let monitor = entry_monitor(entry.edge, &monitors())
            .ok_or_else(|| InputError::Backend("no Windows display is available".into()))?;
        let (x, y) = entry_point(entry, monitor);
        move_absolute(x, y)
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        let (x, y) = cursor()?;
        let target = (x.saturating_add(dx), y.saturating_add(dy));
        if let Some(edge) = self.active_edge {
            if toward_sender(edge, dx, dy) {
                let axis_target = match edge {
                    Edge::Left | Edge::Right => (target.0, y),
                    Edge::Top | Edge::Bottom => (x, target.1),
                };
                if !on_monitor(axis_target) {
                    self.return_requested = true;
                }
            }
        }
        move_absolute(target.0, target.1)
    }

    fn button(&mut self, button: u8, pressed: bool) -> Result<(), InputError> {
        let (flags, data) = button_flags(button, pressed)
            .ok_or_else(|| InputError::Unsupported("mouse button".into()))?;
        send_mouse(0, 0, flags, data)?;
        if pressed {
            self.pressed_buttons.insert(button);
        } else {
            self.pressed_buttons.remove(&button);
        }
        Ok(())
    }

    fn scroll(&mut self, horizontal: i32, vertical: i32) -> Result<(), InputError> {
        // Wire scroll follows the Linux receiver: positive values scroll down and right.
        if vertical != 0 {
            let amount = vertical.saturating_mul(-WHEEL_DELTA);
            send_mouse(0, 0, MOUSEEVENTF_WHEEL, amount as u32)?;
        }
        if horizontal != 0 {
            let amount = horizontal.saturating_mul(WHEEL_DELTA);
            send_mouse(0, 0, MOUSEEVENTF_HWHEEL, amount as u32)?;
        }
        Ok(())
    }

    /// A physical key from a sender that knows scancodes; this PC's own layout picks the character.
    fn physical_key(&mut self, scancode: u32, pressed: bool) -> Result<(), InputError> {
        if scancode & !0xe0ff != 0 {
            return Err(InputError::Unsupported("scancode".into()));
        }
        self.pending_dead = None;
        send_scancode(scancode, pressed)?;
        if pressed {
            self.pressed_scancodes.insert(scancode);
        } else {
            self.pressed_scancodes.remove(&scancode);
        }
        Ok(())
    }

    fn key(&mut self, code: u32, pressed: bool) -> Result<(), InputError> {
        if !pressed && self.transient_keys.remove(&code) {
            return Ok(());
        }
        if matches!(code, keycode::LEFT_SHIFT | keycode::RIGHT_SHIFT) {
            if pressed {
                self.shift.insert(code);
            } else {
                self.shift.remove(&code);
            }
        }
        if let Some(character) = unicode_character(code) {
            if pressed {
                self.transient_keys.insert(code);
                let composed = self.take_dead_for(character)?;
                send_text(composed.unwrap_or(character))?;
            }
            return Ok(());
        }
        if let Some(keysym) = dead_key(code) {
            if pressed {
                self.transient_keys.insert(code);
                if let Some(previous) = self.pending_dead.replace(keysym) {
                    if let Some(spacing) = dead_key_spacing(previous) {
                        send_text(spacing)?;
                    }
                }
            }
            return Ok(());
        }
        let mapped = virtual_key(code).ok_or_else(|| InputError::Unsupported("keycode".into()))?;
        if pressed && self.pending_dead.is_some() && !is_modifier(code) {
            if let Some(base) = self.base_character(code) {
                if let Some(composed) = self.take_dead_for(base)? {
                    self.transient_keys.insert(code);
                    return send_text(composed);
                }
            } else {
                self.flush_dead()?;
            }
        }
        send_key(mapped, pressed)?;
        let entry = (mapped.vk, mapped.extended);
        if pressed {
            self.pressed_keys.insert(entry);
        } else {
            self.pressed_keys.remove(&entry);
        }
        Ok(())
    }

    /// Consumes a pending dead key, returning the composed character when one exists.
    fn take_dead_for(&mut self, base: char) -> Result<Option<char>, InputError> {
        let Some(keysym) = self.pending_dead.take() else {
            return Ok(None);
        };
        if let Some(composed) = compose(keysym, base) {
            return Ok(Some(composed));
        }
        if let Some(spacing) = dead_key_spacing(keysym) {
            send_text(spacing)?;
        }
        Ok(None)
    }

    fn flush_dead(&mut self) -> Result<(), InputError> {
        if let Some(spacing) = self.pending_dead.take().and_then(dead_key_spacing) {
            send_text(spacing)?;
        }
        Ok(())
    }

    fn base_character(&self, code: u32) -> Option<char> {
        if code == keycode::SPACE {
            return Some(' ');
        }
        if !(65..=90).contains(&code) {
            return None;
        }
        // SAFETY: GetKeyState only reads this thread's view of the Caps Lock toggle.
        let caps = unsafe { GetKeyState(0x14) } & 1 != 0;
        let shift = !self.shift.is_empty();
        let upper = caps ^ shift;
        let letter = char::from_u32(code)?;
        Some(if upper {
            letter
        } else {
            letter.to_ascii_lowercase()
        })
    }
}

impl Drop for WindowsInjector {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[async_trait]
impl InputInjector for WindowsInjector {
    async fn inject(&mut self, event: Message) -> Result<(), InputError> {
        match event {
            Message::LeaveScreen => {
                self.reset();
                Ok(())
            }
            Message::EnterScreen(entry) => self.enter(entry),
            Message::MouseMove(motion) => self.mouse_move(motion.dx, motion.dy),
            Message::MouseButton(button) => self.button(button.button, button.pressed),
            Message::MouseScroll(scroll) => self.scroll(scroll.horizontal, scroll.vertical),
            Message::Key(key) if key.scancode != 0 => self.physical_key(key.scancode, key.pressed),
            Message::Key(key) => self.key(key.keycode, key.pressed),
            _ => Err(InputError::Unsupported("input message".into())),
        }
    }
}

fn is_modifier(code: u32) -> bool {
    matches!(
        code,
        keycode::LEFT_SHIFT
            | keycode::RIGHT_SHIFT
            | keycode::LEFT_CONTROL
            | keycode::RIGHT_CONTROL
            | keycode::LEFT_ALT
            | keycode::RIGHT_ALT
            | keycode::LEFT_META
            | keycode::RIGHT_META
            | keycode::CAPS_LOCK
    )
}

fn button_flags(button: u8, pressed: bool) -> Option<(u32, u32)> {
    Some(match (button, pressed) {
        (1, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (1, false) => (MOUSEEVENTF_LEFTUP, 0),
        (2, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (2, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (3, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (3, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (4, true) => (MOUSEEVENTF_XDOWN, 1),
        (4, false) => (MOUSEEVENTF_XUP, 1),
        (5, true) => (MOUSEEVENTF_XDOWN, 2),
        (5, false) => (MOUSEEVENTF_XUP, 2),
        _ => return None,
    })
}

fn send(inputs: &[INPUT]) -> Result<(), InputError> {
    // SAFETY: `inputs` is a valid slice of initialized INPUT structures for the call's duration.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        return Err(InputError::Backend(
            "Windows rejected the input; the secure desktop may be active".into(),
        ));
    }
    Ok(())
}

fn send_mouse(dx: i32, dy: i32, flags: u32, data: u32) -> Result<(), InputError> {
    send(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }])
}

fn keyboard_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// `scancode` is a PC set-1 make code; 0xE0xx marks the extended prefix.
fn send_scancode(scancode: u32, pressed: bool) -> Result<(), InputError> {
    let mut flags = KEYEVENTF_SCANCODE;
    if scancode & 0xff00 == 0xe000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !pressed {
        flags |= KEYEVENTF_KEYUP;
    }
    send(&[keyboard_input(0, (scancode & 0xff) as u16, flags)])
}

fn send_key(key: VirtualKey, pressed: bool) -> Result<(), InputError> {
    // SAFETY: MapVirtualKeyW is a pure lookup in the active keyboard layout.
    let scan = unsafe { MapVirtualKeyW(u32::from(key.vk), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = 0;
    if key.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !pressed {
        flags |= KEYEVENTF_KEYUP;
    }
    send(&[keyboard_input(key.vk, scan, flags)])
}

fn send_text(character: char) -> Result<(), InputError> {
    let mut units = [0_u16; 2];
    let mut inputs = Vec::with_capacity(4);
    for unit in character.encode_utf16(&mut units).iter() {
        inputs.push(keyboard_input(0, *unit, KEYEVENTF_UNICODE));
    }
    for unit in character.encode_utf16(&mut units).iter() {
        inputs.push(keyboard_input(
            0,
            *unit,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        ));
    }
    send(&inputs)
}

fn move_absolute(x: i32, y: i32) -> Result<(), InputError> {
    // SAFETY: GetSystemMetrics only reads the virtual desktop bounds.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
        )
    };
    let x = i64::from(x.clamp(left, left + width - 1) - left);
    let y = i64::from(y.clamp(top, top + height - 1) - top);
    let nx = (x * 65535 / i64::from(width - 1)) as i32;
    let ny = (y * 65535 / i64::from(height - 1)) as i32;
    send_mouse(
        nx,
        ny,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
    )
}

fn cursor() -> Result<(i32, i32), InputError> {
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: `point` is a valid output location.
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(InputError::Backend("cursor position unavailable".into()));
    }
    Ok((point.x, point.y))
}

fn on_monitor((x, y): (i32, i32)) -> bool {
    // SAFETY: MonitorFromPoint only reads the display layout.
    !unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) }.is_null()
}

fn monitors() -> Vec<(Rect, bool)> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the Vec pointer passed by `monitors` and outlives the enumeration.
        let found = unsafe { &mut *(data as *mut Vec<(Rect, bool)>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rcWork: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dwFlags: 0,
        };
        // SAFETY: `info` is initialized with its size as GetMonitorInfoW requires.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            let rect = info.rcMonitor;
            found.push((
                Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
                info.dwFlags & MONITORINFOF_PRIMARY != 0,
            ));
        }
        1
    }
    let mut found: Vec<(Rect, bool)> = Vec::new();
    // SAFETY: The callback only writes to `found`, which lives until EnumDisplayMonitors returns.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            &mut found as *mut Vec<(Rect, bool)> as LPARAM,
        )
    };
    found
}
