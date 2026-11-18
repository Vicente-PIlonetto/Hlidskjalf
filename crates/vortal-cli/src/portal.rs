//! Linux sender: opens a portal at a Hyprland monitor edge and forwards captured input.
//!
//! Local input is grabbed only while the cursor is on the other computer. Any failure — network,
//! device, or channel — ends the remote session, and dropping the capture returns the devices to
//! the compositor at once.

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, Mutex};
use vortal_input_linux::{
    capture::{Capture, CaptureEvent},
    hyprland::{self, Monitor},
    keymap,
};
use vortal_proto::{Edge, EnterScreen, Key, Message, MouseButton, MouseMove, MouseScroll};

const MAX_CLIPBOARD_TEXT: usize = 16 * 1024;

pub struct PortalOptions {
    pub peer: SocketAddr,
    pub identity: PathBuf,
    pub peer_certificate: PathBuf,
    pub monitor: Option<String>,
    pub side: Edge,
    pub shortcut: (u8, u16),
    pub clipboard: bool,
    pub sensitivity: f64,
}

pub fn parse_edge(value: &str) -> anyhow::Result<Edge> {
    Ok(match value {
        "Left" => Edge::Left,
        "Right" => Edge::Right,
        "Top" => Edge::Top,
        "Bottom" => Edge::Bottom,
        _ => anyhow::bail!("invalid edge: {value}"),
    })
}

fn status(line: &str) {
    println!("VORTAL_PORTAL_{line}");
}

pub async fn run(options: PortalOptions) -> anyhow::Result<()> {
    let (events, events_rx) = mpsc::channel(1024);
    let (connected_tx, connected_rx) = tokio::sync::oneshot::channel();
    let (return_tx, mut return_rx) = mpsc::channel(4);
    let (clipboard_tx, mut clipboard_rx) = mpsc::channel(16);
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(4);
    let connected = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let returned = Arc::new(AtomicBool::new(false));

    let worker = {
        let events = events.clone();
        let connected = Arc::clone(&connected);
        let stop = Arc::clone(&stop);
        let returned = Arc::clone(&returned);
        let monitor = options.monitor.clone();
        let (side, shortcut, sensitivity) = (options.side, options.shortcut, options.sensitivity);
        thread::spawn(move || {
            Worker {
                events,
                connected,
                stop,
                returned,
                monitor,
                side,
                shortcut,
                sensitivity,
            }
            .run();
        })
    };
    {
        let connected = Arc::clone(&connected);
        tokio::spawn(async move {
            if connected_rx.await.is_ok() {
                connected.store(true, Ordering::SeqCst);
                status("CONNECTED");
            }
        });
    }
    {
        let returned = Arc::clone(&returned);
        tokio::spawn(async move {
            while return_rx.recv().await.is_some() {
                returned.store(true, Ordering::SeqCst);
            }
        });
    }
    tokio::spawn(async move { while audio_rx.recv().await.is_some() {} });
    let last_clipboard = Arc::new(Mutex::new(None::<String>));
    if options.clipboard {
        tokio::spawn(poll_clipboard(
            events.clone(),
            Arc::clone(&connected),
            Arc::clone(&last_clipboard),
        ));
    }
    let clipboard = options.clipboard;
    tokio::spawn(async move {
        while let Some(message) = clipboard_rx.recv().await {
            if let Message::ClipboardText(text) = message {
                if clipboard && text.len() <= MAX_CLIPBOARD_TEXT {
                    *last_clipboard.lock().await = Some(text.clone());
                    let _ = write_clipboard(text).await;
                }
            }
        }
    });
    drop(events);

    let result = vortal_net::connect_with_events_and_clipboard(
        options.peer,
        &options.identity,
        &options.peer_certificate,
        events_rx,
        connected_tx,
        return_tx,
        clipboard_tx,
        audio_tx,
    )
    .await;
    stop.store(true, Ordering::SeqCst);
    let _ = tokio::task::spawn_blocking(move || worker.join()).await;
    result
}

struct Worker {
    events: mpsc::Sender<Message>,
    connected: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    returned: Arc<AtomicBool>,
    monitor: Option<String>,
    side: Edge,
    shortcut: (u8, u16),
    sensitivity: f64,
}

/// The normalized entry position when the cursor has just arrived at the portal edge.
fn crossing(
    side: Edge,
    monitor: &Monitor,
    previous: (i32, i32),
    cursor: (i32, i32),
) -> Option<u16> {
    let (width, height) = monitor.logical_size();
    let (x, y) = cursor;
    let inside = |(px, py): (i32, i32)| {
        px >= monitor.x && px < monitor.x + width && py >= monitor.y && py < monitor.y + height
    };
    if !inside(cursor) || !inside(previous) {
        return None;
    }
    let reached = match side {
        Edge::Right => x >= monitor.x + width - 1 && x > previous.0,
        Edge::Left => x <= monitor.x && x < previous.0,
        Edge::Bottom => y >= monitor.y + height - 1 && y > previous.1,
        Edge::Top => y <= monitor.y && y < previous.1,
    };
    if !reached {
        return None;
    }
    let fraction = match side {
        Edge::Left | Edge::Right => f64::from(y - monitor.y) / f64::from((height - 1).max(1)),
        Edge::Top | Edge::Bottom => f64::from(x - monitor.x) / f64::from((width - 1).max(1)),
    };
    Some((fraction * 65535.0).round().clamp(0.0, 65535.0) as u16)
}

fn inward(side: Edge, monitor: &Monitor, cursor: (i32, i32)) -> (i32, i32) {
    let (width, height) = monitor.logical_size();
    let margin = 24.min(width / 4).min(height / 4);
    match side {
        Edge::Right => (monitor.x + width - 1 - margin, cursor.1),
        Edge::Left => (monitor.x + margin, cursor.1),
        Edge::Bottom => (cursor.0, monitor.y + height - 1 - margin),
        Edge::Top => (cursor.0, monitor.y + margin),
    }
}

impl Worker {
    fn run(self) {
        let mut previous = None;
        let mut cooldown = Instant::now();
        let mut monitors: Vec<Monitor> = Vec::new();
        let mut refreshed = Instant::now() - Duration::from_secs(10);
        while !self.stop.load(Ordering::SeqCst) {
            if !self.connected.load(Ordering::SeqCst) {
                previous = None;
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            thread::sleep(Duration::from_millis(10));
            if refreshed.elapsed() > Duration::from_secs(2) {
                refreshed = Instant::now();
                match hyprland::monitors() {
                    Ok(found) => monitors = found,
                    Err(_) => status("ERROR hyprlandUnavailable"),
                }
            }
            let Ok(cursor) = hyprland::cursor() else {
                continue;
            };
            let Some(old) = previous.replace(cursor) else {
                continue;
            };
            if Instant::now() < cooldown {
                continue;
            }
            let monitor = match &self.monitor {
                Some(name) => monitors.iter().find(|monitor| &monitor.name == name),
                None => monitors.iter().find(|monitor| monitor.focused),
            };
            let Some(monitor) = monitor.cloned() else {
                continue;
            };
            if let Some(position) = crossing(self.side, &monitor, old, cursor) {
                let pause = self.remote_session(position);
                let (x, y) = inward(self.side, &monitor, cursor);
                let _ = hyprland::move_cursor(x, y);
                previous = None;
                cooldown = Instant::now() + pause;
            }
        }
    }

    /// Runs one visit to the other computer; returns how long to wait before the next one.
    fn remote_session(&self, position: u16) -> Duration {
        let mut capture = match Capture::open() {
            Ok(capture) => capture,
            Err(error) => {
                status(&format!("ERROR {error}"));
                return Duration::from_secs(3);
            }
        };
        if capture.any_pressed() {
            return Duration::from_millis(300);
        }
        if let Err(error) = capture.grab() {
            status(&format!("ERROR {error}"));
            return Duration::from_secs(3);
        }
        self.returned.store(false, Ordering::SeqCst);
        let entry = Message::EnterScreen(EnterScreen {
            edge: self.side,
            position,
        });
        if self.events.blocking_send(entry).is_err() {
            return Duration::from_millis(500);
        }
        status("ENTER");
        let mut modifiers = 0_u8;
        let mut remainder = (0.0_f64, 0.0_f64);
        let mut batch = Vec::with_capacity(64);
        'session: while !self.stop.load(Ordering::SeqCst)
            && !self.returned.swap(false, Ordering::SeqCst)
        {
            batch.clear();
            if capture.poll(&mut batch).is_err() {
                break;
            }
            if batch.is_empty() {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            for event in batch.drain(..) {
                let message = match event {
                    CaptureEvent::Key { code, pressed } => {
                        let bit = keymap::modifier_bit(code);
                        if pressed {
                            modifiers |= bit;
                        } else {
                            modifiers &= !bit;
                        }
                        let (required, key) = self.shortcut;
                        if pressed && code == key && modifiers & required == required {
                            break 'session;
                        }
                        let scancode = keymap::scancode(code).unwrap_or(0);
                        let keycode = keymap::wire_code(code);
                        if scancode == 0 && keycode == 0 {
                            continue;
                        }
                        Message::Key(Key {
                            scancode,
                            keycode,
                            pressed,
                            modifiers: 0,
                        })
                    }
                    CaptureEvent::Motion { dx, dy } => {
                        let x = f64::from(dx) * self.sensitivity + remainder.0;
                        let y = f64::from(dy) * self.sensitivity + remainder.1;
                        remainder = (x.fract(), y.fract());
                        let (dx, dy) = (x.trunc() as i32, y.trunc() as i32);
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        // Motion may be dropped under back-pressure; keys and buttons may not.
                        if let Err(mpsc::error::TrySendError::Closed(_)) = self
                            .events
                            .try_send(Message::MouseMove(MouseMove { dx, dy }))
                        {
                            break 'session;
                        }
                        continue;
                    }
                    CaptureEvent::Button { button, pressed } => {
                        Message::MouseButton(MouseButton { button, pressed })
                    }
                    CaptureEvent::Scroll {
                        horizontal,
                        vertical,
                    } => Message::MouseScroll(MouseScroll {
                        horizontal,
                        vertical,
                    }),
                };
                if self.events.blocking_send(message).is_err() {
                    break 'session;
                }
            }
        }
        let _ = self.events.try_send(Message::LeaveScreen);
        drop(capture);
        status("RETURN");
        Duration::from_millis(500)
    }
}

async fn poll_clipboard(
    events: mpsc::Sender<Message>,
    connected: Arc<AtomicBool>,
    last: Arc<Mutex<Option<String>>>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let mut primed = false;
    loop {
        interval.tick().await;
        if !connected.load(Ordering::SeqCst) {
            continue;
        }
        let Ok(Some(text)) = read_clipboard().await else {
            continue;
        };
        let mut last = last.lock().await;
        if last.as_deref() == Some(text.as_str()) {
            continue;
        }
        *last = Some(text.clone());
        if !primed {
            primed = true;
            continue;
        }
        if events.send(Message::ClipboardText(text)).await.is_err() {
            break;
        }
    }
}

async fn read_clipboard() -> anyhow::Result<Option<String>> {
    let mut command = tokio::process::Command::new("wl-paste");
    command
        .args(["--no-newline", "--type", "text/plain;charset=utf-8"])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(1), command.output()).await??;
    if !output.status.success() || output.stdout.len() > MAX_CLIPBOARD_TEXT {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout).ok())
}

async fn write_clipboard(text: String) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    tokio::time::timeout(Duration::from_secs(2), child.wait()).await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> Monitor {
        Monitor {
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
            scale: 2.0,
            transform: 0,
            focused: true,
            disabled: false,
        }
    }

    #[test]
    fn crosses_only_when_arriving_at_the_portal_edge() {
        let screen = monitor();
        assert_eq!(
            crossing(Edge::Right, &screen, (1900, 0), (1919, 0)),
            Some(0)
        );
        assert_eq!(
            crossing(Edge::Right, &screen, (1919, 1079), (1919, 1079)),
            None
        );
        assert_eq!(
            crossing(Edge::Left, &screen, (5, 1079), (0, 1079)),
            Some(65535)
        );
        assert_eq!(crossing(Edge::Top, &screen, (100, 4), (100, 4)), None);
        assert_eq!(inward(Edge::Right, &screen, (1919, 500)), (1895, 500));
    }
}
