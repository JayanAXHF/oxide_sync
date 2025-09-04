use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum::Display;
use tokio::io::{AsyncRead, AsyncWrite};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Message {
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
    pub mtime: u64,       // modification time (epoch seconds)
    pub mode: u32,        // permissions (POSIX-style)
    pub uid: Option<u32>, // optional owner user id
    pub gid: Option<u32>, // optional group id
    pub is_dir: bool,     // directory marker
    pub is_symlink: bool, // symlink marker
}
