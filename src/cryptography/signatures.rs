use blake2::{Blake2s256, Digest};
use std::fmt::Write;

pub const MODULUS: i64 = 1 << 16;

#[derive(Debug, Clone)]
pub struct WeakSignature {
    block_size: usize,
    data: Box<[u8]>,
}

#[derive(Debug, Clone)]
pub struct WeakSignatureBlock {
    pub offset: u64,
    pub signature: i64,
    pub r1: i64,
    pub r2: i64,
}

impl WeakSignature {
    pub fn new(block_size: usize, data: Box<[u8]>) -> Self {
        Self { block_size, data }
    }

    pub fn sign(&self, offset: usize) -> WeakSignatureBlock {
        let block = &self.data[offset..offset + self.block_size];

        let r1 = block.iter().map(|&b| b as i64).sum::<i64>() % MODULUS;

        let r2 = block
            .iter()
            .enumerate()
            .map(|(i, &a)| (self.block_size as i64 - i as i64) * a as i64)
            .sum::<i64>()
            % MODULUS;

        let r = (r1 + MODULUS * r2) % (MODULUS * MODULUS);
        WeakSignatureBlock::new(offset as u64, r, r1, r2)
    }

    pub fn compute_next_signature(&self, prev: WeakSignatureBlock) -> WeakSignatureBlock {
        let new_offset = prev.offset + 1;
        let old_idx = prev.offset as usize;
        let new_idx = old_idx + self.block_size;

        if new_idx >= self.data.len() {
            // Can't roll further
            return prev;
        }

        let old_byte = self.data[old_idx] as i64;
        let new_byte = self.data[new_idx] as i64;

        let mut r1 = prev.r1 - old_byte + new_byte;
        r1 = ((r1 % MODULUS) + MODULUS) % MODULUS; // keep positive

        let mut r2 = prev.r2 - (self.block_size as i64 * old_byte) + r1;
        r2 = ((r2 % MODULUS) + MODULUS) % MODULUS;

        let r = (r1 + MODULUS * r2) % (MODULUS * MODULUS);

        WeakSignatureBlock::new(new_offset, r, r1, r2)
    }
}

impl WeakSignatureBlock {
    pub fn new(offset: u64, signature: i64, r1: i64, r2: i64) -> Self {
        Self {
            offset,
            signature,
            r1,
            r2,
        }
    }
    pub fn get_signature(&self) -> i64 {
        self.signature
    }
}

pub fn compute_strong_signature(data: &[u8]) -> String {
    let mut hasher = Blake2s256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let mut out = String::with_capacity(hash.len() * 2);
    for byte in hash {
        write!(&mut out, "{:02x}", byte).unwrap();
    }
    out
}
