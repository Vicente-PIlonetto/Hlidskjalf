//! Minimal Hyprland IPC client for monitor layout and cursor position.

use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use hlidskjalf_input::InputError;

/// A monitor in Hyprland layout coordinates (logical pixels).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Monitor {
    #[serde(default)]
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub transform: i32,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub disabled: bool,
}

fn one() -> f64 {
    1.0
}

impl Monitor {
    /// Size in layout coordinates: physical size divided by scale, swapped when rotated.
    pub fn logical_size(&self) -> (i32, i32) {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let width = (f64::from(self.width) / scale).round() as i32;
        let height = (f64::from(self.height) / scale).round() as i32;
        if self.transform % 2 == 1 {
            (height.max(1), width.max(1))
        } else {
            (width.max(1), height.max(1))
        }
    }
}

#[derive(serde::Deserialize)]
struct CursorPosition {
    x: f64,
    y: f64,
}

pub fn monitors() -> Result<Vec<Monitor>, InputError> {
    let monitors: Vec<Monitor> = query("j/monitors")?;
    Ok(monitors
        .into_iter()
        .filter(|monitor| !monitor.disabled)
        .collect())
}

pub fn focused_monitor() -> Result<Monitor, InputError> {
    monitors()?
        .into_iter()
        .find(|monitor| monitor.focused)
        .ok_or_else(|| InputError::Backend("Hyprland focused monitor unavailable".into()))
}

pub fn cursor() -> Result<(i32, i32), InputError> {
    let position: CursorPosition = query("j/cursorpos")?;
    Ok((position.x.round() as i32, position.y.round() as i32))
}

pub fn move_cursor(x: i32, y: i32) -> Result<(), InputError> {
    let response = raw_query(&format!("dispatch movecursor {x} {y}"), |bytes| {
        bytes.starts_with(b"ok")
    })?;
    if !response.starts_with(b"ok") {
        return Err(InputError::Backend(
            "Hyprland refused to move the cursor".into(),
        ));
    }
    Ok(())
}

fn query<T: serde::de::DeserializeOwned>(request: &str) -> Result<T, InputError> {
    let response = raw_query(request, |bytes| {
        serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_ok()
    })?;
    serde_json::from_slice(&response).map_err(|error| InputError::Backend(error.to_string()))
}

/// Sends one request; stops reading once `complete` accepts the response or the socket closes.
fn raw_query(request: &str, complete: impl Fn(&[u8]) -> bool) -> Result<Vec<u8>, InputError> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| InputError::Backend("XDG_RUNTIME_DIR missing".into()))?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .ok_or_else(|| InputError::Backend("Hyprland session missing".into()))?;
    let path = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let mut socket =
        UnixStream::connect(path).map_err(|error| InputError::Backend(error.to_string()))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|error| InputError::Backend(error.to_string()))?;
    socket
        .write_all(request.as_bytes())
        .map_err(|error| InputError::Backend(error.to_string()))?;
    socket
        .shutdown(Shutdown::Write)
        .map_err(|error| InputError::Backend(error.to_string()))?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match socket.read(&mut chunk) {
            Ok(0) => break,
            Ok(size) => {
                response.extend_from_slice(&chunk[..size]);
                if complete(&response) {
                    break;
                }
                if response.len() > 262_144 {
                    return Err(InputError::Backend("Hyprland response too large".into()));
                }
            }
            Err(error) => return Err(InputError::Backend(error.to_string())),
        }
    }
    Ok(response)
}
