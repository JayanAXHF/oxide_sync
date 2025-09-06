use clap::Parser;
use cli::{Cli, ClientServerOpts};
use color_eyre::eyre::eyre;
use cryptography::{
    Delta, IndexTable, MODULUS, WeakSignature, WeakSignatureBlock, compute_strong_signature,
};
use ignore::Walk;
use itertools::Itertools;
use pipeline::{
    DataMessage, FlistEntry, Message, Pipeline, ReceiverSSHTunnel, SSHCommand, SSHMessageError,
    Tunnel,
};
use regex_lite::Regex;
use std::mem;
use std::path::Path;
use std::{
    fs::{File, read_dir},
    io::{Read, Seek},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};
use tracing::info;

pub mod cli;
pub mod cryptography;
mod errors;
mod logging;
pub mod pipeline;

// #[global_allocator]
// static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    let cli = Cli::parse();
    if !cli.quiet {
        crate::logging::init()?;
    }
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
                    info!("server: flist start");
                    for (entry, idx) in files.iter().zip(0..) {
                        let indexed_file = FlistEntry {
                            index: idx,
                            ..entry.clone()
                        };
                        let msg = Message::FlistEntry(indexed_file.clone());
                        tunnel.write_message(msg).await?;
                        flist.push(indexed_file);
                        info!("server: flist entry: {:?}", entry);
                    }
                    let msg = Message::FlistEnd;
                    tunnel.write_message(msg).await?;
                    info!("server: flist end");
                }
                Message::Arguments(args) => {
                    info!("arguments: {:?}", args);
                    opts = args;
                }
                Message::FileIndex(index) => {
                    let block_size = 128;
                    let file = flist[index as usize].clone();
                    let mut base = Vec::new();
                    File::open(&file.filename)?.read_to_end(&mut base)?;
                    let mut index_table = IndexTable::new();

                    // Build index table from base file
                    let signer_base = WeakSignature::new(block_size, base.clone().into());
                    if base.len() < block_size {
                        let strong = compute_strong_signature(&base);
                        // store a dummy weak signature (e.g. hash of entire base)
                        let weak_val: i64 = base.iter().map(|&b| b as i64).sum::<i64>() % MODULUS;
                        let weak = WeakSignatureBlock::new(0, weak_val, weak_val, weak_val);
                        index_table.add(weak, strong, 0);
                    } else {
                        // Normal case: compute rolling weak + strong for each base block
                        let mut prev_hash: Option<WeakSignatureBlock> = None;
                        for (i, block) in base.chunks_exact(block_size).enumerate() {
                            if i == 0 {
                                let sign = signer_base.sign(0);
                                let strong = compute_strong_signature(block);
                                index_table.add(sign.clone(), strong, 0);
                                prev_hash = Some(sign);
                            } else {
                                // roll from previous
                                let rolling =
                                    signer_base.compute_next_signature(prev_hash.clone().unwrap());
                                let strong = compute_strong_signature(block);
                                index_table.add(rolling.clone(), strong, i);
                                prev_hash = Some(rolling);
                            }
                        }
                    }

                    let msg = Message::Data(DataMessage {
                        map: index_table,
                        file_index: index,
                    });
                    tunnel.write_message(msg).await?;
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
        for entry in pipeline.flist {
            println!("{:?}", entry);
            pipeline
                .tunnel
                .write_message(Message::FileIndex(entry.index))
                .await?;
            let msg = pipeline.tunnel.read_message().await?;
            if let Message::Data(data) = msg {
                let path = PathBuf::from(&entry.filename);
                let path = match path.strip_prefix(cli.to.clone().expect("to is not set")) {
                    Ok(path) => cli.from.clone().unwrap().join(path),
                    Err(_) => path,
                };

                let Ok(mut file_) = File::open(path) else {
                    println!("error opening file {:?}", entry.clone());
                    continue;
                };
                let mut delta = Delta::new();
                let block_size = 128;
                let mut new = Vec::new();
                let index_table = data.map;
                file_.read_to_end(&mut new)?;

                // If the new file is shorter than block_size, nothing to roll — emit whole new as block.
                if new.len() < block_size {
                    if !new.is_empty() {
                        delta.add_block(new.to_vec());
                    }
                    println!("{:?}", delta);
                    continue;
                }

                // Prepare to scan `new`
                let signer_new = WeakSignature::new(block_size, new.clone().into());
                let mut unmatched_buffer: Vec<u8> = Vec::new();
                let mut i: usize = 0;

                // Initialize prev_hash for position 0
                let mut prev_hash: Option<WeakSignatureBlock> = Some(signer_new.sign(0));

                // Slide while there is a full window
                while i + block_size <= new.len() {
                    // Ensure we have a hash for current position
                    let cur_hash = match prev_hash.clone() {
                        Some(h) => h,
                        None => {
                            // If we don't have a prev_hash, compute it directly
                            let s = signer_new.sign(i);
                            prev_hash = Some(s.clone());
                            s
                        }
                    };

                    // Check index table for weak match
                    if let Some((base_index, strong)) = index_table.find(cur_hash.get_signature()) {
                        // Verify with strong signature on the new window
                        let strong2 = compute_strong_signature(&new[i..i + block_size]);
                        if strong == strong2 {
                            // Found a match — flush any unmatched data first
                            if !unmatched_buffer.is_empty() {
                                delta.add_block(mem::take(&mut unmatched_buffer));
                            }
                            // Emit index referring to base block
                            delta.add_index(base_index);

                            // Jump forward by a full block
                            i += block_size;

                            // If we still can produce full windows, set prev_hash to sign(i)
                            if i + block_size <= new.len() {
                                prev_hash = Some(signer_new.sign(i));
                            } else {
                                prev_hash = None;
                            }
                            continue;
                        }
                    }

                    // No match at current window:
                    // Append a single byte (the current byte) to unmatched buffer and slide by 1
                    unmatched_buffer.push(new[i]);
                    i += 1;

                    // Update rolling hash for the new window if possible
                    if i + block_size <= new.len() {
                        // roll from previous cur_hash
                        let next_hash = signer_new.compute_next_signature(cur_hash);
                        prev_hash = Some(next_hash);
                    } else {
                        // not enough bytes left for a full window -> no further rolling hashes
                        prev_hash = None;
                    }
                }

                // Append any remaining tail bytes (less than a full block) to the unmatched buffer
                if i < new.len() {
                    unmatched_buffer.extend_from_slice(&new[i..]);
                }

                // Flush unmatched buffer if non-empty
                if !unmatched_buffer.is_empty() {
                    delta.add_block(unmatched_buffer);
                }
                println!("{:?}", delta);
            }
        }
    }
    Ok(())
}
