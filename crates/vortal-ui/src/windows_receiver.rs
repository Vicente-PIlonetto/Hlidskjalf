//! Lets this Windows computer be controlled: supervises `vortald --input` and its pairing requests.

use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use anyhow::{bail, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

pub(super) struct Receiver {
    dir: PathBuf,
    enabled: AtomicBool,
    running: AtomicBool,
    controlled: Arc<AtomicBool>,
    child: Mutex<Option<Child>>,
    error: Mutex<Option<String>>,
}

impl Receiver {
    pub fn new(data_dir: &Path) -> Arc<Self> {
        let dir = data_dir.join("receiver");
        let enabled = fs::read(dir.join("receiver-enabled")).is_ok_and(|value| value == b"1");
        let receiver = Arc::new(Self {
            dir,
            enabled: AtomicBool::new(enabled),
            running: AtomicBool::new(false),
            controlled: Arc::new(AtomicBool::new(false)),
            child: Mutex::new(None),
            error: Mutex::new(None),
        });
        if enabled {
            receiver.spawn_supervisor();
        }
        receiver
    }

    /// True while a remote cursor is on this computer; the local portal must stay closed.
    pub fn is_controlled(&self) -> bool {
        self.controlled.load(Ordering::SeqCst)
    }

    pub fn configure(self: &Arc<Self>, args: Value) -> anyhow::Result<Value> {
        if let Some(enabled) = args.as_bool() {
            fs::create_dir_all(&self.dir)?;
            fs::write(
                self.dir.join("receiver-enabled"),
                if enabled { "1" } else { "0" },
            )?;
            self.enabled.store(enabled, Ordering::SeqCst);
            if enabled {
                self.spawn_supervisor();
            } else {
                self.stop_child();
            }
        }
        self.state()
    }

    pub fn set_clipboard(&self, enabled: bool) {
        if fs::create_dir_all(&self.dir).is_ok() {
            let _ = fs::write(
                self.dir.join("clipboard-enabled"),
                if enabled { "1" } else { "0" },
            );
        }
    }

    pub fn decide(&self, args: Value) -> anyhow::Result<Value> {
        let nonce = args
            .get("nonce")
            .and_then(Value::as_str)
            .filter(|value| value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .context("Invalid pairing request")?;
        let action = match args.get("action").and_then(Value::as_str) {
            Some("allow") => "allow",
            Some("deny") => "deny",
            _ => bail!("Invalid pairing decision"),
        };
        let pending: Value =
            serde_json::from_slice(&fs::read(self.dir.join("pairing-request.json"))?)?;
        if pending.get("nonce").and_then(Value::as_str) != Some(nonce) {
            bail!("pairingTimedOut");
        }
        fs::write(
            self.dir.join("pairing-decision"),
            format!("{nonce}:{action}"),
        )?;
        Ok(Value::Bool(true))
    }

    pub fn shutdown(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.stop_child();
    }

    fn state(&self) -> anyhow::Result<Value> {
        let id = fs::read(self.identity_dir().join("cert.der"))
            .ok()
            .map(|certificate| pairing_id(&certificate));
        let pending = fs::read(self.dir.join("pairing-request.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .map(|value| {
                json!({
                    "nonce": value.get("nonce").cloned().unwrap_or(Value::Null),
                    "name": value.get("name").cloned().unwrap_or(Value::Null),
                    "fingerprint": value.get("fingerprint").cloned().unwrap_or(Value::Null),
                })
            });
        let error = self.error.lock().ok().and_then(|error| error.clone());
        Ok(json!({
            "enabled": self.enabled.load(Ordering::SeqCst),
            "running": self.running.load(Ordering::SeqCst),
            "controlled": self.is_controlled(),
            "paired": self.dir.join("peer.der").exists(),
            "id": id,
            "pending": pending,
            "error": error,
        }))
    }

    fn identity_dir(&self) -> PathBuf {
        self.dir.join("identity")
    }

    fn set_error(&self, error: Option<String>) {
        if let Ok(mut slot) = self.error.lock() {
            *slot = error;
        }
    }

    fn stop_child(&self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(mut process) = child.take() {
                let _ = process.kill();
                let _ = process.wait();
            }
        }
        self.controlled.store(false, Ordering::SeqCst);
    }

    fn spawn_supervisor(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let receiver = Arc::clone(self);
        thread::spawn(move || {
            while receiver.enabled.load(Ordering::SeqCst) {
                if let Err(error) = receiver.run_once() {
                    receiver.set_error(Some(error.to_string()));
                }
                receiver.controlled.store(false, Ordering::SeqCst);
                for _ in 0..20 {
                    if !receiver.enabled.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            receiver.running.store(false, Ordering::SeqCst);
        });
    }

    fn run_once(&self) -> anyhow::Result<()> {
        self.ensure_identity()?;
        let directory = std::env::current_exe()?
            .parent()
            .context("desktop executable has no parent directory")?
            .to_path_buf();
        let mut process = hidden_command(directory.join("vortald.exe"))
            .args([
                "--input",
                "--clipboard",
                "--bind",
                "0.0.0.0:24800",
                "--identity",
            ])
            .arg(self.identity_dir())
            .arg("--peer-cert")
            .arg(self.dir.join("peer.der"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("could not start the Windows receiver")?;
        let stdout = process.stdout.take();
        if let Ok(mut child) = self.child.lock() {
            *child = Some(process);
        }
        if !self.enabled.load(Ordering::SeqCst) {
            self.stop_child();
            return Ok(());
        }
        self.set_error(None);
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match line.as_str() {
                    "VORTAL_RECEIVER_ENTER" => self.controlled.store(true, Ordering::SeqCst),
                    "VORTAL_RECEIVER_LEAVE" => self.controlled.store(false, Ordering::SeqCst),
                    _ => {}
                }
            }
        }
        let status = self
            .child
            .lock()
            .ok()
            .and_then(|mut child| child.take())
            .map(|mut process| process.wait());
        match status {
            Some(Ok(status)) if !status.success() && self.enabled.load(Ordering::SeqCst) => {
                bail!("receiverStopped")
            }
            _ => Ok(()),
        }
    }

    fn ensure_identity(&self) -> anyhow::Result<()> {
        let identity = self.identity_dir();
        let key = identity.join("key.der");
        let cert = identity.join("cert.der");
        if key.exists() && cert.exists() {
            return Ok(());
        }
        if key.exists() || cert.exists() {
            bail!("Identity files are incomplete");
        }
        fs::create_dir_all(&self.dir)?;
        let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "windows".into());
        let cli = std::env::current_exe()?
            .parent()
            .context("desktop executable has no parent directory")?
            .join("vortal.exe");
        let output = hidden_command(cli)
            .args(["identity", &hostname, "--dir"])
            .arg(&identity)
            .output()?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(())
    }
}

fn hidden_command(program: PathBuf) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn pairing_id(certificate: &[u8]) -> String {
    let hash = Sha256::digest(certificate);
    let number = u32::from_be_bytes([hash[2], hash[3], hash[4], hash[5]]) % 1_000_000;
    format!(
        "{}{}{number:06}",
        (b'A' + hash[0] % 26) as char,
        (b'A' + hash[1] % 26) as char
    )
}
