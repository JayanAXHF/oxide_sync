use std::fmt::{Debug, Write as _};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use crate::cryptography::MODULUS;

use super::{IndexTable, WeakSignature, WeakSignatureBlock, compute_strong_signature, index_table};

#[derive(Debug, Clone)]
pub enum Ops {
    Index(usize),
    Block(Vec<u8>),
}

#[derive(Debug, Clone, Default)]
pub struct Delta {
    pub ops: Vec<Ops>,
}

impl Delta {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn add_block(&mut self, block: Vec<u8>) {
        self.ops.push(Ops::Block(block));
    }

    pub fn add_index(&mut self, index: usize) {
        self.ops.push(Ops::Index(index));
    }

    pub fn add_byte(&mut self, byte: u8) {
        if self.ops.is_empty() {
            self.add_block(vec![byte]);
            return;
        }
        match self.ops.last_mut().unwrap() {
            Ops::Block(block) => block.push(byte),
            Ops::Index(_) => self.add_block(vec![byte]),
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.ops.is_empty()
    }

    pub fn dump(&self) -> String {
        let mut s = String::new();
        for op in self.ops.iter() {
            match op {
                Ops::Index(index) => write!(&mut s, "<b*{}*>", index).unwrap(),
                Ops::Block(block) => {
                    s.push_str(core::str::from_utf8(block).expect("Error with UTF-8 string"))
                }
            }
        }
        s
    }

    /// Apply this delta to the given base file bytes.
    pub fn apply(&self, base: &[u8], block_size: usize) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();

        for op in &self.ops {
            match op {
                Ops::Index(index) => {
                    let start = index * block_size;
                    let end = std::cmp::min(start + block_size, base.len());
                    if start >= base.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Invalid block index {} for base length {}",
                                index,
                                base.len()
                            ),
                        ));
                    }
                    output.extend_from_slice(&base[start..end]);
                }
                Ops::Block(bytes) => {
                    output.extend_from_slice(bytes);
                }
            }
        }

        Ok(output)
    }

    /// Apply this delta to a base file and write the result to another file.
    pub fn patch_file<P: AsRef<Path>>(
        &self,
        old_path: P,
        out_path: P,
        block_size: usize,
    ) -> io::Result<()> {
        // Read base file
        let mut old_bytes = Vec::new();
        File::open(&old_path)?.read_to_end(&mut old_bytes)?;

        // Apply delta
        let new_bytes = self.apply(&old_bytes, block_size)?;

        // Write patched file
        let mut out_file = File::create(out_path)?;
        out_file.write_all(&new_bytes)?;

        Ok(())
    }

    pub fn diff(base: &[u8], new: &[u8], block_size: usize) -> Self {
        use std::mem;

        let mut index_table = IndexTable::new();

        // Build index table from base file
        let signer_base = WeakSignature::new(block_size, base.into());
        if base.len() < block_size {
            let strong = compute_strong_signature(base);
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
                    let rolling = signer_base.compute_next_signature(prev_hash.clone().unwrap());
                    let strong = compute_strong_signature(block);
                    index_table.add(rolling.clone(), strong, i);
                    prev_hash = Some(rolling);
                }
            }
        }

        let mut delta = Delta::new();

        // If the new file is shorter than block_size, nothing to roll — emit whole new as block.
        if new.len() < block_size {
            if !new.is_empty() {
                delta.add_block(new.to_vec());
            }
            return delta;
        }

        // Prepare to scan `new`
        let signer_new = WeakSignature::new(block_size, new.into());
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

        delta
    }
}

impl IntoIterator for Delta {
    type Item = Ops;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.ops.into_iter()
    }
}
