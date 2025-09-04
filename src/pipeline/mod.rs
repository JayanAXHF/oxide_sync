mod structs;
use std::{fmt::Display, process::Stdio};
mod tests;

use bincode::error::EncodeError;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, ChildStdout},
};

pub use structs::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Eror while reading or writing to the SSH tunnel: {0}")]
    Message(#[from] SSHMessageError),
    #[error("Error while processing stdin/stdout: {0}")]
    IO(#[from] tokio::io::Error),
    #[error("Error while encoding: {0}")]
    Encoding(#[from] EncodeError),
    #[error("Error while decoding: {0}")]
    Decoding(#[from] bincode::error::DecodeError),
}

type Result<T> = color_eyre::Result<T, Error>;

impl SSHCommand {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        password: Option<String>,
        remote_cmd: String,
    ) -> Self {
        SSHCommand {
            host: host.into_boxed_str(),
            port,
            username: username.into_boxed_str(),
            password,
            remote_cmd,
        }
    }
}

impl From<String> for SSHCommand {
    fn from(s: String) -> Self {
        let mut split = s.split('@');
        let username = split.next().unwrap().to_string().into_boxed_str();
        let host = split.next().unwrap();
        let mut split = host.split(':');
        let host = split.next().unwrap().to_string().into_boxed_str();
        let port = match split.next() {
            Some(port) => port.parse().unwrap(),
            None => 22,
        };

        SSHCommand {
            host,
            port,
            username,
            password: None,
            remote_cmd: String::new(),
        }
    }
}

impl Display for SSHCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.username, self.host)
    }
}

impl SSHTunnel<ChildStdin, ChildStdout> {
    pub async fn new(command: SSHCommand) -> Self {
        let mut cmd = tokio::process::Command::new("ssh");
        cmd.arg("-o").arg("StrictHostKeyChecking=no");
        cmd.arg("-o").arg("UserKnownHostsFile=/dev/null");
        cmd.arg("-o").arg("LogLevel=ERROR");
        cmd.arg("-o").arg("ConnectTimeout=10");
        cmd.arg("-o").arg("ServerAliveInterval=10");
        cmd.arg("-o").arg("ServerAliveCountMax=3");
        cmd.arg("-p").arg(command.port.to_string());
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.arg(command.to_string());
        cmd.arg(command.remote_cmd);
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        SSHTunnel { stdin, stdout }
    }
    pub async fn write_message(&mut self, msg: Message) -> Result<()> {
        let bin_msg = bincode::serde::encode_to_vec(msg, bincode::config::standard())?;
        let msg_len = bin_msg.len() as u32;
        self.stdin.write_all(&msg_len.to_be_bytes()).await?;
        self.stdin.write_all(&bin_msg).await?;
        self.stdin.flush().await?;
        Ok(())
    }
    pub async fn read_message(&mut self) -> Result<Message> {
        let mut len_buf = [0u8; 4];
        self.stdout.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; msg_len];
        self.stdout.read_exact(&mut buf).await?;
        let (msg, _): (Message, usize) =
            bincode::serde::decode_from_slice(&buf, bincode::config::standard())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(msg)
    }
}
