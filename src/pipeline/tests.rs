#![cfg(test)]
use super::*;
use pretty_assertions::assert_eq;
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

#[tokio::test]
#[ignore] // run manually with `cargo test -- --ignored`
async fn roundtrip_over_duplex() -> std::io::Result<()> {
    let (mut client, mut server) = duplex(1024);

    let msg_out = Message::Done;

    // Spawn server task that echoes back what it reads
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        let n = server.read(&mut buf).await.unwrap();
        server.write_all(&buf[..n]).await.unwrap();
    });

    // Encode + send
    let bin_msg = bincode::serde::encode_to_vec(&msg_out, bincode::config::standard()).unwrap();
    let msg_len = bin_msg.len() as u32;
    client.write_all(&msg_len.to_be_bytes()).await?;
    client.write_all(&bin_msg).await?;

    // Read response
    let mut len_buf = [0u8; 4];
    client.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; resp_len];
    client.read_exact(&mut buf).await?;

    let (msg_in, _): (Message, usize) =
        bincode::serde::decode_from_slice(&buf, bincode::config::standard()).unwrap();

    assert_eq!(msg_in, msg_out);

    Ok(())
}

#[tokio::test]
#[ignore] // run manually with `cargo test -- --ignored`
async fn ssh_send_receive_roundtrip() -> Result<()> {
    // Assumes you can SSH into localhost without password (ssh-agent or ssh-copy-id)
    let cmd = SSHCommand {
        username: whoami::username().into_boxed_str(),
        host: "127.0.0.1".to_string().into_boxed_str(),
        password: None,
        port: 22,
        remote_cmd: "cat".to_string(),
    };

    let mut tunnel = SSHTunnel::new(cmd).await;

    // Send a test message
    let msg_out = Message::Done;
    tunnel.write_message(msg_out.clone()).await?;

    // Read it back
    let msg_in = tunnel.read_message().await?;

    assert_eq!(format!("{:?}", msg_in), format!("{:?}", msg_out));
    let msg_out = Message::FlistEnd;
    tunnel.write_message(msg_out.clone()).await?;
    let msg_in = tunnel.read_message().await?;
    assert_eq!(format!("{:?}", msg_in), format!("{:?}", msg_out));

    Ok(())
}
