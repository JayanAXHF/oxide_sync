use async_trait::async_trait;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum::Display;
use tokio::io::{AsyncRead, AsyncWrite, Stdin, Stdout};

use crate::cli::ClientServerOpts;

use super::Result;

#[derive(Debug, Clone, Setters)]
pub struct SSHCommand {
    #[setters(generate = false)]
    pub host: Box<str>,
    pub port: u16,
    pub username: Box<str>,
    #[setters(generate)]
    pub password: Option<String>,
    pub remote_cmd: String,
}

#[derive(Debug, Clone)]
pub struct SSHTunnel<W: AsyncWrite + Unpin, R: AsyncRead + Unpin> {
    pub stdin: W,
    pub stdout: R,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataMessage {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub file_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display)]
pub enum Message {
    SYNC,
    ACK,
    NACK,
    Arguments(ClientServerOpts),
    Data(DataMessage),
    Redo(u32),
    Done,                   // MSG_DONE
    Error(SSHMessageError), // MSG_ERROR
    Info(String),           // MSG_INFO
    Warning(String),        // MSG_WARNING
    FlistEntry(FlistEntry), // MSG_FLIST
    FlistEnd,               // MSG_FLIST_END
    Restore(Vec<u8>),       // MSG_RESTORE
    Deleted(u32),           // MSG_DELETED
    Success(u32),           // MSG_SUCCESS
    Degenerate(u32),        // MSG_DEGENERATE
    Stats(Vec<u8>),         // MSG_STATS
    IoTimeout,              // MSG_IO_TIMEOUT
    NoSend(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, Display, PartialEq, Eq)]
pub enum SSHMessageError {
    IoError(String),
    TransferError(String),
    FatalError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlistEntry {
    pub index: u32,       // file index (assigned by sender)
    pub filename: String, // path relative to the sync root
    pub size: u64,        // file size in bytes
    pub mtime: i64,       // modification time (epoch seconds)
    pub mode: u32,        // permissions (POSIX-style)
    pub uid: Option<u32>, // optional owner user id
    pub gid: Option<u32>, // optional group id
    pub is_dir: bool,     // directory marker
    pub is_symlink: bool, // symlink marker
}

pub struct Pipeline {
    pub tunnel: Box<dyn Tunnel>,
    pub connected: PipelineState,
    pub flist: Vec<FlistEntry>,
    pub stats: Vec<u8>,
}

#[derive(Debug, Default)]
pub enum PipelineState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error(super::Error),
}

impl PartialEq for PipelineState {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (PipelineState::Disconnected, PipelineState::Disconnected)
                | (PipelineState::Connecting, PipelineState::Connecting)
                | (PipelineState::Connected, PipelineState::Connected)
                | (PipelineState::Error(_), PipelineState::Error(_))
        )
    }
}

pub struct ReceiverSSHTunnel {
    pub stdin: Stdin,
    pub stdout: Stdout,
}

#[async_trait]
pub trait Tunnel {
    async fn write_message(&mut self, msg: Message) -> Result<()>;
    async fn read_message(&mut self) -> Result<Message>;
}
