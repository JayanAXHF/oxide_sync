use std::fmt::{Debug, Write as _};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

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
}

impl IntoIterator for Delta {
    type Item = Ops;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.ops.into_iter()
    }
}
