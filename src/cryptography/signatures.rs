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
        let r1 = (self.data[offset..offset + self.block_size]
            .iter()
            .sum::<u8>() as i64)
            % MODULUS;
        let r2 = (self.data[offset..offset + self.block_size]
            .iter()
            .enumerate()
            .map(|(i, a)| (self.block_size - i) * *a as usize))
        .sum::<usize>() as i64
            % MODULUS;
        let r = r1 + MODULUS * r2;
        return WeakSignatureBlock::new(offset as u64, r, r1, r2);
    }
    pub fn compute_next_signature(&self, prev_sig: WeakSignatureBlock) -> WeakSignatureBlock {
        let new_offset = prev_sig.offset + 1;
        let r1 = (prev_sig.r1 - self.data[prev_sig.offset as usize] as i64
            + self.data[prev_sig.offset as usize + self.block_size] as i64)
            % MODULUS;
        let r2 = (prev_sig.r2
            - self.block_size as i64 * self.data[prev_sig.offset as usize] as i64
            + r1)
            % MODULUS;
        let r = r1 + MODULUS * r2;
        return WeakSignatureBlock::new(new_offset, r, r1, r2);
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
