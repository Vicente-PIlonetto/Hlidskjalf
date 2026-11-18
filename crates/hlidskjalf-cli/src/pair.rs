//! Pairing by ID: finds the receiver on the LAN or Tailscale and exchanges public certificates.

use std::{
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
    path::Path,
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const INPUT_PORT: u16 = 24800;
const PAIRING_PORT: u16 = 24801;
const DISCOVERY_PORT: u16 = 24802;

/// Normalizes `AB 123456` to `AB123456`, rejecting anything else.
pub fn normalize_id(value: &str) -> anyhow::Result<String> {
    let id: String = value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect();
    if id.len() != 8
        || !id.as_bytes()[..2].iter().all(u8::is_ascii_uppercase)
        || !id.as_bytes()[2..].iter().all(u8::is_ascii_digit)
    {
        bail!("pairingInvalidId");
    }
    Ok(id)
}

pub fn pairing_id(certificate: &[u8]) -> String {
    let hash = Sha256::digest(certificate);
    let number = u32::from_be_bytes([hash[2], hash[3], hash[4], hash[5]]) % 1_000_000;
    format!(
        "{}{}{number:06}",
        (b'A' + hash[0] % 26) as char,
        (b'A' + hash[1] % 26) as char
    )
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|name| name.trim().to_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "computer".into())
        .chars()
        .take(128)
        .collect()
}

/// Pairs with the receiver showing `id`, saves its certificate, and returns its input address.
pub fn pair(id: &str, identity: &Path, peer_certificate: &Path) -> anyhow::Result<SocketAddr> {
    let id = normalize_id(id)?;
    let certificate_path = identity.join("cert.der");
    if !certificate_path.exists() {
        hlidskjalf_net::generate_identity(identity, &device_name())?;
    }
    let certificate = std::fs::read(&certificate_path).context("remoteMissingIdentity")?;
    let mut peers = local_peers(&id);
    peers.extend(tailscale_peers());
    peers.sort_unstable();
    peers.dedup();
    if peers.is_empty() {
        bail!("idNotFound");
    }
    let (sender, receiver) = mpsc::channel();
    for address in peers.iter().copied() {
        let sender = sender.clone();
        let id = id.clone();
        let certificate = certificate.clone();
        thread::spawn(move || {
            let _ = sender.send(pair_at_address(address, &id, &certificate));
        });
    }
    drop(sender);
    let (mut denied, mut timed_out) = (false, false);
    for result in receiver.into_iter().take(peers.len()) {
        match result {
            Ok((address, peer)) => {
                let temporary = peer_certificate.with_extension("der.tmp");
                if let Some(parent) = peer_certificate.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&temporary, peer)?;
                std::fs::rename(temporary, peer_certificate)?;
                return Ok(SocketAddr::new(IpAddr::V4(address), INPUT_PORT));
            }
            Err(error) => {
                denied |= error.to_string().contains("pairingDenied");
                timed_out |= error.to_string().contains("pairingTimedOut");
            }
        }
    }
    if denied {
        bail!("pairingDenied");
    }
    if timed_out {
        bail!("pairingTimedOut");
    }
    bail!("idNotFound")
}

fn pair_at_address(
    address: Ipv4Addr,
    id: &str,
    certificate: &[u8],
) -> anyhow::Result<(Ipv4Addr, Vec<u8>)> {
    let peer = SocketAddr::new(IpAddr::V4(address), PAIRING_PORT);
    let mut stream = TcpStream::connect_timeout(&peer, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(Duration::from_secs(65)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let request = json!({
        "id": id,
        "certificate": base64::engine::general_purpose::STANDARD.encode(certificate),
        "name": device_name(),
    });
    writeln!(stream, "{request}")?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).map_err(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                anyhow::anyhow!("pairingTimedOut")
            } else {
                error.into()
            }
        })?;
        if read == 0 || line.len() > 32768 {
            bail!("pairingInvalid")
        }
        let response: Value = serde_json::from_str(&line).context("pairingInvalid")?;
        match response.get("status").and_then(Value::as_str) {
            Some("pending") => continue,
            Some("denied") => bail!("pairingDenied"),
            Some("approved") => {
                let encoded = response
                    .get("certificate")
                    .and_then(Value::as_str)
                    .context("pairingInvalid")?;
                let peer = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .context("pairingInvalid")?;
                if peer.is_empty() || peer.len() > 16384 || pairing_id(&peer) != id {
                    bail!("pairingInvalid");
                }
                return Ok((address, peer));
            }
            _ => bail!("idNotFound"),
        }
    }
}

fn local_peers(id: &str) -> Vec<Ipv4Addr> {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    let request = format!("HLIDSKJALF_DISCOVER {id}");
    if socket.set_broadcast(true).is_err()
        || socket
            .send_to(request.as_bytes(), ("255.255.255.255", DISCOVERY_PORT))
            .is_err()
    {
        return Vec::new();
    }
    let expected = format!("HLIDSKJALF_HERE {id}");
    let mut addresses = Vec::new();
    let mut buffer = [0_u8; 64];
    let deadline = Instant::now() + Duration::from_secs(2);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if remaining.is_zero() || socket.set_read_timeout(Some(remaining)).is_err() {
            break;
        }
        let Ok((length, source)) = socket.recv_from(&mut buffer) else {
            break;
        };
        if buffer[..length] == *expected.as_bytes() {
            if let IpAddr::V4(address) = source.ip() {
                if address.is_private() {
                    addresses.push(address);
                }
            }
        }
    }
    addresses
}

fn tailscale_peers() -> Vec<Ipv4Addr> {
    let program = if cfg!(windows) {
        std::env::var_os("ProgramFiles")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| r"C:\Program Files".into())
            .join(r"Tailscale\tailscale.exe")
    } else {
        "tailscale".into()
    };
    let Ok(output) = Command::new(program).args(["status", "--json"]).output() else {
        return Vec::new();
    };
    let Ok(status) = serde_json::from_slice::<Value>(&output.stdout) else {
        return Vec::new();
    };
    tailscale_addresses(&status)
}

/// Online Tailscale peers' IPv4 addresses, on any operating system.
fn tailscale_addresses(status: &Value) -> Vec<Ipv4Addr> {
    let Some(peers) = status.get("Peer").and_then(Value::as_object) else {
        return Vec::new();
    };
    peers
        .values()
        .filter(|peer| peer.get("Online").and_then(Value::as_bool) == Some(true))
        .filter_map(|peer| peer.get("TailscaleIPs").and_then(Value::as_array))
        .flatten()
        .filter_map(|value| value.as_str()?.parse::<Ipv4Addr>().ok())
        .filter(|ip| ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ids() {
        assert_eq!(normalize_id("ab 123456").ok().as_deref(), Some("AB123456"));
        assert!(normalize_id("A1234567").is_err());
        assert!(normalize_id("AB12345").is_err());
    }

    #[test]
    fn reads_online_tailscale_peers_of_any_os() {
        let status = json!({"Peer": {
            "a": {"Online": true, "OS": "windows", "TailscaleIPs": ["100.64.1.2", "fd7a::1"]},
            "b": {"Online": false, "OS": "linux", "TailscaleIPs": ["100.64.1.3"]},
            "c": {"Online": true, "OS": "linux", "TailscaleIPs": ["100.100.1.4"]}
        }});
        let mut found = tailscale_addresses(&status);
        found.sort_unstable();
        assert_eq!(
            found,
            vec![Ipv4Addr::new(100, 64, 1, 2), Ipv4Addr::new(100, 100, 1, 4)]
        );
    }
}
