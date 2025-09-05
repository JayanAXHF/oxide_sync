use std::{fs::read_dir, os::unix::fs::MetadataExt, path::PathBuf};

use clap::Parser;
use cli::{Cli, ClientServerOpts};
use color_eyre::eyre::eyre;
use ignore::Walk;
use itertools::Itertools;
use pipeline::{
    FlistEntry, Message, Pipeline, ReceiverSSHTunnel, SSHCommand, SSHMessageError, Tunnel,
};
use regex::Regex;
use tracing::info;

pub mod cli;
pub mod cryptography;
mod errors;
mod logging;
pub mod pipeline;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    let cli = Cli::parse();
    let server = cli.server;
    if server {
        let mut tunnel = ReceiverSSHTunnel::new();
        let mut flist: Vec<FlistEntry> = Vec::new();
        let mut opts = ClientServerOpts::default();
        loop {
            let msg = tunnel.read_message().await?;
            match msg {
                Message::SYNC => {
                    info!("SYNC");
                    let msg = Message::ACK;
                    tunnel.write_message(msg).await?;
                }
                Message::ACK => {
                    info!("ACK");

                    let files = if opts.recursive {
                        Walk::new(&opts.to)
                            .filter_map(|e| {
                                e.ok().and_then(|e| {
                                    if e.file_type()?.is_file() {
                                        if opts.exclude.iter().any(|p| {
                                            e.path().starts_with(p) || e.path().ends_with(p)
                                        }) {
                                            info!("skipping {:?}", opts.exclude);
                                            return None;
                                        }
                                        Some(e)
                                    } else {
                                        None
                                    }
                                })
                            })
                            .enumerate()
                            .map(|(idx, e)| {
                                let uid = match e.metadata() {
                                    Ok(m) => Some(m.uid()),
                                    Err(_) => None,
                                };
                                let gid = match e.metadata() {
                                    Ok(m) => Some(m.gid()),
                                    Err(_) => None,
                                };
                                FlistEntry {
                                    index: idx as u32,
                                    filename: e.path().to_string_lossy().to_string(),
                                    size: e.metadata().unwrap().len(),
                                    mtime: e.metadata().unwrap().mtime(),
                                    mode: e.metadata().unwrap().mode(),
                                    uid,
                                    gid,
                                    is_dir: false,
                                    is_symlink: false,
                                }
                            })
                            .collect_vec()
                    } else {
                        let read_dir_res = read_dir(&opts.to);
                        if let Err(e) = read_dir_res {
                            return Err(eyre!(
                                "Error while reading directory {:?}: {}",
                                opts.to,
                                e
                            ));
                        }
                        let files = read_dir_res.unwrap();
                        files
                            .filter_map(|e| {
                                let Ok(e) = e else {
                                    return None;
                                };
                                let Ok(file_type) = e.file_type() else {
                                    return None;
                                };
                                let uid = match e.metadata() {
                                    Ok(m) => Some(m.uid()),
                                    Err(_) => None,
                                };
                                let gid = match e.metadata() {
                                    Ok(m) => Some(m.gid()),
                                    Err(_) => None,
                                };
                                if !opts
                                    .exclude
                                    .iter()
                                    .any(|p| e.path().starts_with(p) || e.path().ends_with(p))
                                {
                                    return None;
                                }

                                Some(FlistEntry {
                                    index: 0,
                                    filename: e.path().to_string_lossy().to_string(),
                                    size: e.metadata().unwrap().len(),
                                    mtime: e.metadata().unwrap().mtime(),
                                    mode: e.metadata().unwrap().mode(),
                                    uid,
                                    gid,
                                    is_dir: file_type.is_dir(),
                                    is_symlink: file_type.is_symlink(),
                                })
                            })
                            .collect_vec()
                    };
                    for (entry, idx) in files.iter().zip(0..) {
                        let indexed_file = FlistEntry {
                            index: idx,
                            ..entry.clone()
                        };
                        let msg = Message::FlistEntry(indexed_file.clone());
                        tunnel.write_message(msg).await?;
                        flist.push(indexed_file);
                        info!("flist entry: {:?}", entry);
                    }
                }
                Message::Arguments(args) => {
                    info!("arguments: {:?}", args);
                    opts = args;
                }
                _ => {
                    let msg = Message::Error(SSHMessageError::FatalError(
                        "Unknown message received".to_string(),
                    ));
                    tunnel.write_message(msg).await?;
                }
            }
        }
    } else {
        println!("Client mode");
        let to = cli.to.clone().unwrap().to_string_lossy().to_string();
        let regex = Regex::new(r"^([a-zA-Z0-9._-]+)@([a-zA-Z0-9.-]+):(.*)$")?;
        let caps = regex.captures(&to).unwrap();
        let username = caps.get(1).unwrap().as_str();
        let host = caps.get(2).unwrap().as_str();
        let remote_path = caps.get(3).unwrap().as_str();
        let port = cli.port;
        let opts = ClientServerOpts {
            to: PathBuf::from(remote_path),
            ..(&cli).into()
        };

        let mut pipeline = Pipeline::new(SSHCommand {
            host: host.into(),
            port,
            username: username.into(),
            password: None,
            remote_cmd: "/Users/jayansunil/Dev/rust/oxide_sync/target/debug/oxide_sync --server"
                .to_string(),
        })
        .await?;
        pipeline.init().await?;
        pipeline.send_arguments(opts).await?;
        pipeline.tunnel.write_message(Message::ACK).await?;
        pipeline.receive_flist().await?;
        println!("flist: {:?}", pipeline.flist);
        drop(pipeline);
    }
    Ok(())
}
