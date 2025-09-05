mod structs;
use std::{fmt::Display, process::Stdio};
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use bincode::error::EncodeError;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Stdin, Stdout},
    process::{ChildStdin, ChildStdout, Command},
};

pub use structs::*;
use tracing::info;
use tracing_subscriber::fmt::format;

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
    #[error("Unexpected message: {0}")]
    UnexpectedMessage(Message),
    #[error("NACK received")]
    Nack,
    #[error("IO timeout")]
    IoTimeout,
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
        let mut cmd = Command::new("ssh");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());

        cmd.arg(format!("{}@{}", command.username, command.host)); // "username@host"
        cmd.arg(command.remote_cmd.clone());
        dbg!(&cmd);
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        SSHTunnel { stdin, stdout }
    }
}

#[async_trait]
impl Tunnel for SSHTunnel<ChildStdin, ChildStdout> {
    async fn write_message(&mut self, msg: Message) -> Result<()> {
        let bin_msg = bincode::serde::encode_to_vec(msg, bincode::config::standard())?;
        let msg_len = bin_msg.len() as u32;
        self.stdin.write_all(&msg_len.to_be_bytes()).await?;
        self.stdin.write_all(&bin_msg).await?;
        self.stdin.flush().await?;
        Ok(())
    }
    async fn read_message(&mut self) -> Result<Message> {
        dbg!("read message len");
        let mut len_buf = [0u8; 4];

        self.stdout.read_exact(&mut len_buf).await?;
        dbg!("parse message len");
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        dbg!("read message");
        let mut buf = vec![0u8; msg_len];
        self.stdout.read_exact(&mut buf).await?;
        let (msg, _): (Message, usize) =
            bincode::serde::decode_from_slice(&buf, bincode::config::standard())?;
        Ok(msg)
    }
}

impl Pipeline {
    pub async fn new(command: SSHCommand) -> Result<Self> {
        let tunnel = Box::new(SSHTunnel::new(command).await);
        Ok(Self {
            tunnel,
            connected: PipelineState::Disconnected,
            flist: Vec::new(),
            stats: Vec::new(),
        })
    }
    pub async fn init(&mut self) -> Result<()> {
        self.tunnel.write_message(Message::SYNC).await?;
        self.connected = PipelineState::Connecting;
        let msg = self.tunnel.read_message().await?;
        dbg!(&msg);
        match msg {
            Message::ACK => {
                self.connected = PipelineState::Connected;
                info!("ACK");
                Ok(())
            }
            Message::NACK => {
                self.connected = PipelineState::Error(Error::Nack);
                Err(Error::Nack)
            }
            _ => {
                self.connected = PipelineState::Error(Error::UnexpectedMessage(msg.clone()));
                Err(Error::UnexpectedMessage(msg))
            }
        }
    }
    pub async fn send_arguments(&mut self, opts: crate::cli::ClientServerOpts) -> Result<()> {
        self.tunnel
            .write_message(Message::Arguments(opts.clone()))
            .await?;
        Ok(())
    }
    pub async fn receive_flist(&mut self) -> Result<()> {
        loop {
            dbg!("receive flist");
            let msg = self.tunnel.read_message().await?;
            dbg!(&msg);
            match msg {
                Message::FlistEntry(entry) => {
                    self.flist.push(entry);
                }
                Message::FlistEnd => {
                    return Ok(());
                }
                _ => {
                    self.connected = PipelineState::Error(Error::UnexpectedMessage(msg.clone()));
                    return Err(Error::UnexpectedMessage(msg));
                }
            }
        }
    }
    pub async fn receive_stats(&mut self) -> Result<()> {
        loop {
            if self.connected != PipelineState::Connected {
                return Ok(());
            }
            let msg = self.tunnel.read_message().await?;
            match msg {
                Message::Stats(stats) => {
                    self.stats = stats;
                }
                Message::IoTimeout => {
                    self.connected = PipelineState::Error(Error::IoTimeout);
                    return Err(Error::IoTimeout);
                }
                _ => {
                    self.connected = PipelineState::Error(Error::UnexpectedMessage(msg.clone()));
                    return Err(Error::UnexpectedMessage(msg));
                }
            }
        }
    }
}

impl ReceiverSSHTunnel {
    pub fn new() -> Self {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        ReceiverSSHTunnel { stdin, stdout }
    }
}

#[async_trait]
impl Tunnel for ReceiverSSHTunnel {
    #[tracing::instrument(skip(self))]
    async fn write_message(&mut self, msg: Message) -> Result<()> {
        let bin_msg = bincode::serde::encode_to_vec(msg, bincode::config::standard())?;
        let msg_len = bin_msg.len() as u32;
        info!("write message len {}", msg_len);
        self.stdout.write_all(&msg_len.to_be_bytes()).await?;
        self.stdout.write_all(&bin_msg).await?;
        self.stdout.flush().await?;
        Ok(())
    }
    async fn read_message(&mut self) -> Result<Message> {
        let mut len_buf = [0u8; 4];
        dbg!("read message len");
        self.stdin.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;

        dbg!("read message");
        let mut buf = vec![0u8; msg_len];
        self.stdin.read_exact(&mut buf).await?;
        let (msg, _): (Message, usize) =
            bincode::serde::decode_from_slice(&buf, bincode::config::standard())?;
        dbg!(&msg);
        Ok(msg)
    }
}
