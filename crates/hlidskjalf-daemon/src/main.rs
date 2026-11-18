use std::{net::SocketAddr, path::PathBuf};

use async_trait::async_trait;
#[cfg(any(target_os = "linux", windows))]
use hlidskjalf_input::InputInjector;
use hlidskjalf_input::{InputError, PeerDirectory};
#[cfg(any(target_os = "linux", windows))]
use hlidskjalf_proto::Message;
#[cfg(target_os = "linux")]
mod file_clipboard;
use hlidskjalf_vylink::{VyLinkCredentials, VyLinkDirectory};

#[cfg(any(target_os = "linux", windows))]
mod pairing;

#[cfg(target_os = "linux")]
type PlatformInjector = hlidskjalf_input_linux::WaylandInjector;
#[cfg(windows)]
type PlatformInjector = hlidskjalf_input_windows::WindowsInjector;

const KEYRING_SERVICE: &str = "com.hlidskjalf";
const KEYRING_ACCOUNT: &str = "vylink-bearer-token";

struct SystemCredentials;

#[async_trait]
impl VyLinkCredentials for SystemCredentials {
    async fn bearer_token(&self) -> Result<String, InputError> {
        let result = tokio::task::spawn_blocking(|| {
            keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .and_then(|entry| entry.get_password())
        })
        .await
        .map_err(|_| InputError::Backend("credential store task failed".into()))?
        .map_err(|_| {
            InputError::Backend("VyLink token unavailable in system credential store".into())
        })?;
        Ok(result)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut bind: SocketAddr = "0.0.0.0:24800".parse()?;
    let mut identity = None;
    let mut peer_certificate = None;
    let mut vylink_server = None;
    let mut vylink_network = None;
    let mut input_enabled = false;
    #[cfg(any(target_os = "linux", windows))]
    let mut clipboard_supported = false;
    #[cfg(target_os = "linux")]
    let mut audio_supported = false;
    while let Some(option) = args.next() {
        if option == "--input" {
            input_enabled = true;
            continue;
        }
        #[cfg(any(target_os = "linux", windows))]
        if option == "--clipboard" {
            clipboard_supported = true;
            continue;
        }
        #[cfg(target_os = "linux")]
        if option == "--audio" {
            audio_supported = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing value for {option}"))?;
        match option.as_str() {
            "--bind" => bind = value.parse()?,
            "--identity" => identity = Some(PathBuf::from(value)),
            "--peer-cert" => peer_certificate = Some(PathBuf::from(value)),
            "--vylink-server" => vylink_server = Some(value),
            "--vylink-network" => vylink_network = Some(value),
            _ => anyhow::bail!("unknown option: {option}"),
        }
    }
    let identity = identity
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing --identity DIRECTORY"))?;
    let peer_certificate = peer_certificate
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing --peer-cert FILE"))?;
    let directory: Option<std::sync::Arc<dyn PeerDirectory>> = match (vylink_server, vylink_network)
    {
        (Some(server), Some(network)) => {
            let directory =
                VyLinkDirectory::new(&server, network, std::sync::Arc::new(SystemCredentials))?;
            Some(std::sync::Arc::new(directory))
        }
        (None, None) => None,
        _ => anyhow::bail!("--vylink-server and --vylink-network must be provided together"),
    };
    if input_enabled {
        #[cfg(target_os = "linux")]
        {
            return run_input_server(
                bind,
                identity,
                peer_certificate,
                directory,
                clipboard_supported,
                audio_supported,
            )
            .await;
        }
        #[cfg(windows)]
        {
            return run_input_server(
                bind,
                identity,
                peer_certificate,
                directory,
                clipboard_supported,
                false,
            )
            .await;
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        anyhow::bail!("input injection is not available on this platform");
    }
    match directory {
        Some(directory) => {
            hlidskjalf_net::serve_with_peer_directory(bind, identity, peer_certificate, directory)
                .await
        }
        None => hlidskjalf_net::serve(bind, identity, peer_certificate).await,
    }
}

#[cfg(any(target_os = "linux", windows))]
async fn run_input_server(
    bind: SocketAddr,
    identity: &std::path::Path,
    peer_certificate: &std::path::Path,
    directory: Option<std::sync::Arc<dyn PeerDirectory>>,
    clipboard_supported: bool,
    audio_supported: bool,
) -> anyhow::Result<()> {
    let mut injector = PlatformInjector::new()?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);
    let (returns, _) = tokio::sync::broadcast::channel(4);
    let return_signal = returns.clone();
    let (clipboard_out, _) = tokio::sync::broadcast::channel(256);
    let (audio_out, _) = tokio::sync::broadcast::channel(16);
    #[cfg(target_os = "linux")]
    if audio_supported {
        tokio::spawn(capture_audio(audio_out.clone()));
    }
    #[cfg(windows)]
    let _ = audio_supported;
    let clipboard_sender = clipboard_out.clone();
    let last_clipboard = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
    let last_for_events = std::sync::Arc::clone(&last_clipboard);
    let last_files = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
    let files_for_events = std::sync::Arc::clone(&last_files);
    if clipboard_supported {
        tokio::spawn(poll_clipboard(clipboard_sender, last_clipboard, last_files));
    }
    #[cfg(target_os = "linux")]
    let mut incoming_files = file_clipboard::IncomingFiles::default();
    #[cfg(windows)]
    let _ = &files_for_events;
    let (reload_tx, mut reload_rx) = tokio::sync::mpsc::channel(2);
    let server = async {
        // After a new pairing the previous listener may hold the port for a moment.
        let mut failures = 0_u32;
        loop {
            if !peer_certificate.exists() {
                if reload_rx.recv().await.is_none() {
                    anyhow::bail!("pairing listener stopped");
                }
                continue;
            }
            tokio::select! {
                result = hlidskjalf_net::serve_with_media(bind, identity, peer_certificate, directory.clone(), sender.clone(), returns.clone(), clipboard_out.clone(), audio_out.clone()) => {
                    let Err(error) = result else { break Ok(()) };
                    failures += 1;
                    if failures >= 20 {
                        break Err(error);
                    }
                    tracing::warn!(%error, "input listener stopped; retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                updated = reload_rx.recv() => {
                    if updated.is_none() { anyhow::bail!("pairing listener stopped"); }
                    let _ = sender.try_send(Message::LeaveScreen);
                    failures = 0;
                }
            }
        }
    };
    tokio::select! {
        result = pairing::serve(identity, peer_certificate, reload_tx) => result,
        result = server => result,
        result = async {
            while let Some(event) = receiver.recv().await {
                if let Message::ClipboardText(text) = event {
                    if clipboard_supported && clipboard_enabled() && text.len() <= 16 * 1024 {
                        *last_for_events.lock().await = Some(text.clone());
                        tokio::spawn(async move { let _ = write_clipboard_text(text).await; });
                    }
                    continue;
                }
                if matches!(event, Message::ClipboardOffer(_) | Message::ClipboardData(_)) {
                    #[cfg(target_os = "linux")]
                    if clipboard_supported && clipboard_enabled() {
                        if let Some((id, data)) = incoming_files.accept(event) {
                            let last = files_for_events.clone();
                            tokio::spawn(async move {
                                if let Err(error) = file_clipboard::save_files(id, data, last).await {
                                    tracing::warn!(%error, "could not receive clipboard files");
                                }
                            });
                        }
                    }
                    continue;
                }
                let presence = match &event {
                    Message::EnterScreen(_) => Some(true),
                    Message::LeaveScreen => Some(false),
                    _ => None,
                };
                if let Err(error) = injector.inject(event).await {
                    tracing::error!(%error, "input injection failed; restoring local control");
                    let _ = injector.inject(Message::LeaveScreen).await;
                    report_presence(false);
                    let _ = return_signal.send(());
                    continue;
                }
                if let Some(entered) = presence {
                    report_presence(entered);
                }
                if injector.take_return_requested() {
                    let _ = return_signal.send(());
                }
            }
            anyhow::bail!("input event channel closed")
        } => result,
    }
}

#[cfg(any(target_os = "linux", windows))]
async fn poll_clipboard(
    sender: tokio::sync::broadcast::Sender<Message>,
    last_clipboard: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
    last_files: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
) {
    let mut poll = tokio::time::interval(std::time::Duration::from_millis(500));
    #[cfg(windows)]
    let _ = &last_files;
    #[cfg(windows)]
    let mut sequence = 0;
    loop {
        poll.tick().await;
        if !clipboard_enabled() || sender.receiver_count() == 0 {
            continue;
        }
        #[cfg(windows)]
        {
            let current = hlidskjalf_input_windows::WindowsClipboard::sequence();
            if current == sequence {
                continue;
            }
            sequence = current;
        }
        #[cfg(target_os = "linux")]
        if let Ok(Some((signature, paths))) = file_clipboard::read_selection().await {
            let mut last = last_files.lock().await;
            if last.as_deref() != Some(signature.as_str()) {
                *last = Some(signature);
                let sender = sender.clone();
                tokio::spawn(async move {
                    if let Err(error) = file_clipboard::send_files(&sender, &paths).await {
                        tracing::warn!(%error, "could not send clipboard files");
                    }
                });
            }
            continue;
        }
        if let Ok(Some(text)) = read_clipboard_text().await {
            let mut last = last_clipboard.lock().await;
            if last.as_deref() != Some(text.as_str()) {
                *last = Some(text.clone());
                let _ = sender.send(Message::ClipboardText(text));
            }
        }
    }
}

/// Receiver settings written by the desktop app.
#[cfg(any(target_os = "linux", windows))]
fn settings_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(|base| PathBuf::from(base).join("Hlidskjalf").join("receiver"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|base| base.join("hlidskjalf"))
    }
}

#[cfg(any(target_os = "linux", windows))]
fn clipboard_enabled() -> bool {
    settings_dir().is_some_and(|base| {
        std::fs::read(base.join("clipboard-enabled"))
            .map(|value| value == b"1")
            .unwrap_or(true)
    })
}

#[cfg(target_os = "linux")]
fn setting_enabled(name: &str) -> bool {
    settings_dir().is_some_and(|base| std::fs::read(base.join(name)).ok().as_deref() == Some(b"1"))
}

#[cfg(target_os = "linux")]
async fn capture_audio(sender: tokio::sync::broadcast::Sender<Vec<u8>>) {
    use tokio::io::AsyncReadExt;
    loop {
        if !setting_enabled("audio-enabled") {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        }
        let child = tokio::process::Command::new("parec")
            .args([
                "--device=@DEFAULT_MONITOR@",
                "--raw",
                "--format=s16le",
                "--rate=48000",
                "--channels=2",
                "--latency-msec=40",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(mut child) = child else {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            continue;
        };
        loop {
            if !setting_enabled("audio-enabled") {
                break;
            }
            let mut frame = vec![0_u8; 3840];
            match tokio::time::timeout(
                std::time::Duration::from_secs(1),
                stdout.read_exact(&mut frame),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let _ = sender.send(frame);
                }
                _ => break,
            }
        }
        let _ = child.kill().await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(target_os = "linux")]
async fn read_clipboard_text() -> anyhow::Result<Option<String>> {
    let mut command = tokio::process::Command::new("wl-paste");
    command
        .args(["--no-newline", "--type", "text/plain;charset=utf-8"])
        .kill_on_drop(true);
    let output =
        tokio::time::timeout(std::time::Duration::from_secs(1), command.output()).await??;
    if !output.status.success() || output.stdout.len() > 16 * 1024 {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout).ok())
}

#[cfg(target_os = "linux")]
async fn write_clipboard_text(text: String) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await??;
    Ok(())
}

#[cfg(windows)]
async fn read_clipboard_text() -> anyhow::Result<Option<String>> {
    use hlidskjalf_input::ClipboardProvider;
    Ok(hlidskjalf_input_windows::WindowsClipboard
        .read_text()
        .await?)
}

#[cfg(windows)]
async fn write_clipboard_text(text: String) -> anyhow::Result<()> {
    use hlidskjalf_input::ClipboardProvider;
    Ok(hlidskjalf_input_windows::WindowsClipboard
        .write_text(text)
        .await?)
}

/// Tells the desktop app whether the remote cursor is on this computer.
#[cfg(any(target_os = "linux", windows))]
fn report_presence(entered: bool) {
    use std::io::Write;
    let line = if entered {
        "HLIDSKJALF_RECEIVER_ENTER"
    } else {
        "HLIDSKJALF_RECEIVER_LEAVE"
    };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}
