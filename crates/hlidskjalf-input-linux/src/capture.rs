//! Exclusive evdev capture of local mice and keyboards while the cursor is on another computer.
//!
//! Devices are opened only when the portal opens and are grabbed with `EVIOCGRAB`, so the
//! compositor stops seeing them. Dropping [`Capture`] (or the process exiting) closes the file
//! descriptors, which makes the kernel hand the devices back to the compositor immediately.

use std::io::ErrorKind;

use evdev::{
    AbsoluteAxisCode, Device, EventSummary, KeyCode, PropType, RelativeAxisCode,
    SynchronizationCode,
};
use hlidskjalf_input::InputError;

/// A local input event, before translation to the wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    Motion {
        dx: i32,
        dy: i32,
    },
    Button {
        button: u8,
        pressed: bool,
    },
    Scroll {
        horizontal: i32,
        vertical: i32,
    },
    /// An evdev key code; `pressed` is true for presses and auto-repeats.
    Key {
        code: u16,
        pressed: bool,
    },
}

struct Source {
    device: Device,
    touchpad: bool,
    touching: bool,
    last_abs: (Option<i32>, Option<i32>),
    pending: (i32, i32),
}

pub struct Capture {
    sources: Vec<Source>,
}

fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_A) && keys.contains(KeyCode::KEY_ENTER))
}

fn is_mouse(device: &Device) -> bool {
    device
        .supported_relative_axes()
        .is_some_and(|axes| axes.contains(RelativeAxisCode::REL_X))
}

fn is_touchpad(device: &Device) -> bool {
    !device.properties().contains(PropType::DIRECT)
        && device
            .supported_absolute_axes()
            .is_some_and(|axes| axes.contains(AbsoluteAxisCode::ABS_X))
        && device
            .supported_keys()
            .is_some_and(|keys| keys.contains(KeyCode::BTN_TOUCH))
}

impl Capture {
    /// Opens every keyboard, mouse, and touchpad without grabbing them yet.
    pub fn open() -> Result<Self, InputError> {
        let sources: Vec<Source> = evdev::enumerate()
            .filter_map(|(_, device)| {
                let touchpad = !is_mouse(&device) && is_touchpad(&device);
                (is_keyboard(&device) || is_mouse(&device) || touchpad).then_some(Source {
                    device,
                    touchpad,
                    touching: false,
                    last_abs: (None, None),
                    pending: (0, 0),
                })
            })
            .collect();
        if sources.is_empty() {
            return Err(InputError::Backend("inputDevicesUnavailable".into()));
        }
        Ok(Self { sources })
    }

    /// True when any key or button is held; entering then would leave it stuck locally.
    pub fn any_pressed(&self) -> bool {
        self.sources.iter().any(|source| {
            source.device.get_key_state().is_ok_and(|keys| {
                keys.iter()
                    .any(|key| key.code() < 0x100 || (0x110..0x118).contains(&key.code()))
            })
        })
    }

    /// Grabs every device exclusively; on any failure all grabs are released.
    pub fn grab(&mut self) -> Result<(), InputError> {
        for index in 0..self.sources.len() {
            let source = &mut self.sources[index];
            let result = source
                .device
                .set_nonblocking(true)
                .and_then(|()| source.device.grab());
            if let Err(error) = result {
                self.release();
                return Err(InputError::Backend(format!(
                    "could not grab input: {error}"
                )));
            }
        }
        Ok(())
    }

    pub fn release(&mut self) {
        for source in &mut self.sources {
            let _ = source.device.ungrab();
        }
    }

    /// Drains pending events without blocking.
    pub fn poll(&mut self, out: &mut Vec<CaptureEvent>) -> Result<(), InputError> {
        for source in &mut self.sources {
            let events = match source.device.fetch_events() {
                Ok(events) => events.collect::<Vec<_>>(),
                Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
                Err(error) => return Err(InputError::Backend(error.to_string())),
            };
            for event in events {
                match event.destructure() {
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, value) => {
                        source.pending.0 += value;
                    }
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_Y, value) => {
                        source.pending.1 += value;
                    }
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, value) => {
                        // Wire scroll is positive downward; evdev wheel is positive upward.
                        out.push(CaptureEvent::Scroll {
                            horizontal: 0,
                            vertical: -value,
                        });
                    }
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_HWHEEL, value) => {
                        out.push(CaptureEvent::Scroll {
                            horizontal: value,
                            vertical: 0,
                        });
                    }
                    EventSummary::AbsoluteAxis(_, axis, value) if source.touchpad => {
                        let slot = match axis {
                            AbsoluteAxisCode::ABS_X => &mut source.last_abs.0,
                            AbsoluteAxisCode::ABS_Y => &mut source.last_abs.1,
                            _ => continue,
                        };
                        if source.touching {
                            if let Some(previous) = slot.replace(value) {
                                let delta = (value - previous) / 2;
                                if axis == AbsoluteAxisCode::ABS_X {
                                    source.pending.0 += delta;
                                } else {
                                    source.pending.1 += delta;
                                }
                            }
                        }
                    }
                    EventSummary::Key(_, KeyCode::BTN_TOUCH, value) => {
                        source.touching = value != 0;
                        source.last_abs = (None, None);
                    }
                    EventSummary::Key(_, key, value) => {
                        let pressed = value != 0;
                        let button = match key {
                            KeyCode::BTN_LEFT => Some(1),
                            KeyCode::BTN_RIGHT => Some(2),
                            KeyCode::BTN_MIDDLE => Some(3),
                            KeyCode::BTN_SIDE => Some(4),
                            KeyCode::BTN_EXTRA => Some(5),
                            _ => None,
                        };
                        if let Some(button) = button {
                            if value != 2 {
                                out.push(CaptureEvent::Button { button, pressed });
                            }
                        } else if key.code() < 0x100 {
                            out.push(CaptureEvent::Key {
                                code: key.code(),
                                pressed,
                            });
                        }
                    }
                    EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) => {
                        let (dx, dy) = std::mem::take(&mut source.pending);
                        if dx != 0 || dy != 0 {
                            out.push(CaptureEvent::Motion { dx, dy });
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.release();
    }
}
