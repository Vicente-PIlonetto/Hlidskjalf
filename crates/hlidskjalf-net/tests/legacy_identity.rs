//! Identities created before the rename to Hliðskjálf must keep connecting after migration.

use std::{net::UdpSocket, path::Path, time::Duration};

use tokio::sync::{broadcast, mpsc, oneshot};

/// Writes an identity exactly as Vortal created it: a self-signed `vortal-peer` certificate.
fn write_legacy_identity(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let certified = rcgen::generate_simple_self_signed(vec!["vortal-peer".into()])?;
    std::fs::write(
        directory.join("key.der"),
        certified.key_pair.serialize_der(),
    )?;
    std::fs::write(directory.join("cert.der"), certified.cert.der())?;
    std::fs::write(directory.join("device-id"), "linux")?;
    Ok(())
}

#[tokio::test]
async fn migrated_vortal_receiver_accepts_a_new_sender() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!(
        "hlidskjalf-legacy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let server_identity = directory.join("server");
    let client_identity = directory.join("client");
    write_legacy_identity(&server_identity)?;
    hlidskjalf_net::generate_identity(&client_identity, "windows")?;
    let address = {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.local_addr()?
    };
    let (input_tx, _input_rx) = mpsc::channel(16);
    let (return_tx, _) = broadcast::channel(4);
    let server_peer_cert = client_identity.join("cert.der");
    let server = tokio::spawn(async move {
        hlidskjalf_net::serve_with_events_and_returns(
            address,
            &server_identity,
            &server_peer_cert,
            input_tx,
            return_tx,
        )
        .await
    });
    let (_outgoing, outgoing_rx) = mpsc::channel(16);
    let (ready_tx, ready_rx) = oneshot::channel();
    let (return_tx, _return_rx) = mpsc::channel(4);
    let cleanup = directory.clone();
    let client = tokio::spawn(async move {
        hlidskjalf_net::connect_with_events(
            address,
            &client_identity,
            &directory.join("server/cert.der"),
            outgoing_rx,
            ready_tx,
            return_tx,
        )
        .await
    });
    let ready = tokio::time::timeout(Duration::from_secs(3), ready_rx).await;
    client.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(cleanup);
    assert!(
        matches!(ready, Ok(Ok(()))),
        "a sender must authenticate against a receiver whose certificate predates the rename"
    );
    Ok(())
}
