//! Linux sender card: pair with another computer by ID and control it through a portal edge.
//!
//! Input capture runs in a separate `hlidskjalf portal` process so a UI failure can never hold the
//! local mouse or keyboard.

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use gtk::prelude::*;
use serde_json::{json, Value};

use crate::linux_native::{label, setting_enabled, setting_path, state_dir};

const SIDES: [&str; 4] = ["Left", "Right", "Top", "Bottom"];

enum Update {
    Status(String),
    Paired(String),
    PairFailed(String),
}

fn sender_dir() -> Option<PathBuf> {
    state_dir().map(|base| base.join("sender"))
}

fn cli_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("hlidskjalf")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("hlidskjalf"))
}

fn load_config() -> Value {
    setting_path("sender.json")
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({}))
}

fn save_config(config: &Value) {
    if let Some(path) = setting_path("sender.json") {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, config.to_string());
    }
}

fn update_config(key: &str, value: Value) {
    let mut config = load_config();
    if let Some(object) = config.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
    save_config(&config);
}

/// Restarts `hlidskjalf portal` while enabled and reports its status lines to the UI.
struct Supervisor {
    enabled: AtomicBool,
    running: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Supervisor {
    fn start(self: &Arc<Self>, updates: Sender<Update>) {
        self.enabled.store(true, Ordering::SeqCst);
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let supervisor = Arc::clone(self);
        thread::spawn(move || {
            while supervisor.enabled.load(Ordering::SeqCst) {
                let _ = updates.send(Update::Status("remoteConnecting".into()));
                if let Err(error) = supervisor.run_once(&updates) {
                    let _ = updates.send(Update::Status(error.to_string()));
                }
                if !supervisor.enabled.load(Ordering::SeqCst) {
                    break;
                }
                let _ = updates.send(Update::Status("remoteReconnecting".into()));
                for _ in 0..20 {
                    if !supervisor.enabled.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            supervisor.running.store(false, Ordering::SeqCst);
            let _ = updates.send(Update::Status("remoteDisconnected".into()));
        });
    }

    fn stop(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.restart();
    }

    /// Ends the current process; the supervisor starts a new one with fresh settings.
    fn restart(&self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(mut process) = child.take() {
                let _ = process.kill();
                let _ = process.wait();
            }
        }
    }

    fn run_once(&self, updates: &Sender<Update>) -> anyhow::Result<()> {
        let config = load_config();
        let address = config
            .get("address")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("remoteMissingAddress"))?;
        let directory = sender_dir().ok_or_else(|| anyhow::anyhow!("remoteMissingIdentity"))?;
        let side = config
            .get("side")
            .and_then(Value::as_str)
            .filter(|side| SIDES.contains(side))
            .unwrap_or("Right");
        let mut command = Command::new(cli_path());
        command
            .arg("portal")
            .arg(address)
            .arg("--identity")
            .arg(directory.join("identity"))
            .arg("--peer-cert")
            .arg(directory.join("peer.der"))
            .args(["--side", side]);
        if let Some(monitor) = config.get("monitor").and_then(Value::as_str) {
            command.args(["--monitor", monitor]);
        }
        if setting_enabled("clipboard-enabled") {
            command.arg("--clipboard");
        }
        let mut process = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = process.stdout.take();
        if let Ok(mut child) = self.child.lock() {
            *child = Some(process);
        }
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let key = match line.as_str() {
                    "HLIDSKJALF_PORTAL_CONNECTED" | "HLIDSKJALF_PORTAL_RETURN" => "remoteConnected",
                    "HLIDSKJALF_PORTAL_ENTER" => "senderActive",
                    _ => match line.strip_prefix("HLIDSKJALF_PORTAL_ERROR ") {
                        Some(code) => code,
                        None => continue,
                    },
                };
                let _ = updates.send(Update::Status(key.to_owned()));
            }
        }
        if let Ok(mut child) = self.child.lock() {
            if let Some(mut process) = child.take() {
                let _ = process.wait();
            }
        }
        Ok(())
    }
}

fn pair(id: String, updates: Sender<Update>) {
    thread::spawn(move || {
        let Some(directory) = sender_dir() else {
            let _ = updates.send(Update::PairFailed("remoteMissingIdentity".into()));
            return;
        };
        let output = Command::new(cli_path())
            .arg("pair")
            .arg(&id)
            .arg("--identity")
            .arg(directory.join("identity"))
            .arg("--peer-cert")
            .arg(directory.join("peer.der"))
            .stdin(Stdio::null())
            .output();
        let update = match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("HLIDSKJALF_PAIRED "))
                .map(|address| Update::Paired(address.trim().to_owned()))
                .unwrap_or_else(|| Update::PairFailed("pairingInvalid".into())),
            Ok(output) => {
                let error = String::from_utf8_lossy(&output.stderr);
                let code = [
                    "pairingDenied",
                    "pairingTimedOut",
                    "pairingInvalidId",
                    "pairingInvalid",
                    "idNotFound",
                ]
                .into_iter()
                .find(|code| error.contains(code))
                .unwrap_or("idNotFound");
                Update::PairFailed(code.into())
            }
            Err(_) => Update::PairFailed("remoteMissingIdentity".into()),
        };
        let _ = updates.send(update);
    });
}

/// Adds the "control another computer" card to the Linux window.
pub(super) fn build(content: &gtk::Box, locale: &Value) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.style_context().add_class("card");
    let heading = crate::linux_native::rune_heading("ᚱ", &label(locale, "senderTitle"));
    heading.set_xalign(0.0);
    card.pack_start(&heading, false, false, 0);

    let pair_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let id_entry = gtk::Entry::new();
    id_entry.set_placeholder_text(Some("AB 123456"));
    id_entry.set_max_length(9);
    id_entry.set_hexpand(true);
    let pair_button = gtk::Button::with_label(&label(locale, "pairById"));
    pair_row.pack_start(&id_entry, true, true, 0);
    pair_row.pack_start(&pair_button, false, false, 0);
    card.pack_start(&pair_row, false, false, 0);

    let config = load_config();
    let layout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let monitor = gtk::ComboBoxText::new();
    if let Ok(monitors) = hlidskjalf_input_linux::hyprland::monitors() {
        for item in monitors {
            let (width, height) = item.logical_size();
            monitor.append(
                Some(&item.name),
                &format!("{} ({width}×{height})", item.name),
            );
        }
    }
    let saved_monitor = config.get("monitor").and_then(Value::as_str);
    if !saved_monitor.is_some_and(|name| monitor.set_active_id(Some(name))) {
        monitor.set_active(Some(0));
    }
    let side = gtk::ComboBoxText::new();
    for value in SIDES {
        side.append(Some(value), &label(locale, &format!("portal{value}")));
    }
    let saved_side = config
        .get("side")
        .and_then(Value::as_str)
        .unwrap_or("Right");
    side.set_active_id(Some(saved_side));
    layout_row.pack_start(&monitor, true, true, 0);
    layout_row.pack_start(&side, true, true, 0);
    card.pack_start(&layout_row, false, false, 0);

    let connect_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    connect_row.style_context().add_class("setting-row");
    let connect_label = gtk::Label::new(Some(&label(locale, "senderConnect")));
    connect_label.set_xalign(0.0);
    connect_label.set_hexpand(true);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    connect_row.pack_start(&connect_label, true, true, 0);
    connect_row.pack_end(&toggle, false, false, 0);
    card.pack_start(&connect_row, false, false, 0);

    let status = gtk::Label::new(None);
    status.style_context().add_class("status");
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    card.pack_start(&status, false, false, 0);
    content.pack_start(&card, false, false, 0);

    let paired = sender_dir().is_some_and(|dir| dir.join("peer.der").exists())
        && config.get("address").and_then(Value::as_str).is_some();
    toggle.set_sensitive(paired);
    if paired {
        status.set_text(&label(locale, "remotePeerReady"));
    }

    let supervisor = Arc::new(Supervisor {
        enabled: AtomicBool::new(false),
        running: AtomicBool::new(false),
        child: Mutex::new(None),
    });
    let (updates, received): (Sender<Update>, Receiver<Update>) = mpsc::channel();

    {
        let updates = updates.clone();
        let entry = id_entry.clone();
        let status = status.clone();
        let waiting = label(locale, "pairingWaiting");
        pair_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            status.set_text(&waiting);
            pair(entry.text().to_string(), updates.clone());
        });
    }
    {
        let supervisor = Arc::clone(&supervisor);
        monitor.connect_changed(move |combo| {
            if let Some(id) = combo.active_id() {
                update_config("monitor", json!(id.as_str()));
                supervisor.restart();
            }
        });
    }
    {
        let supervisor = Arc::clone(&supervisor);
        side.connect_changed(move |combo| {
            if let Some(id) = combo.active_id() {
                update_config("side", json!(id.as_str()));
                supervisor.restart();
            }
        });
    }
    {
        let supervisor = Arc::clone(&supervisor);
        let updates = updates.clone();
        toggle.connect_state_set(move |_, enabled| {
            update_config("enabled", json!(enabled));
            if enabled {
                supervisor.start(updates.clone());
            } else {
                supervisor.stop();
            }
            gtk::glib::Propagation::Proceed
        });
    }
    {
        let supervisor = Arc::clone(&supervisor);
        content.connect_destroy(move |_| supervisor.stop());
    }
    if paired && config.get("enabled").and_then(Value::as_bool) == Some(true) {
        toggle.set_active(true);
    }

    let locale = locale.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(150), move || {
        while let Ok(update) = received.try_recv() {
            match update {
                Update::Status(key) => status.set_text(&label(&locale, &key)),
                Update::Paired(address) => {
                    update_config("address", json!(address));
                    pair_button.set_sensitive(true);
                    toggle.set_sensitive(true);
                    status.set_text(&label(&locale, "remotePeerReady"));
                    supervisor.restart();
                }
                Update::PairFailed(key) => {
                    pair_button.set_sensitive(true);
                    status.set_text(&label(&locale, &key));
                }
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}
