use std::collections::HashMap;

use super::WeakSignatureBlock;

#[derive(Debug, Clone)]
pub struct IndexTableChunk {
    strong_signature: String,
    index: usize,
}

#[derive(Debug, Clone)]
pub struct IndexTable {
    map: HashMap<i64, IndexTableChunk>,
}

impl IndexTable {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    pub fn add(
        &mut self,
        weak_signature: WeakSignatureBlock,
        strong_signature: String,
        index: usize,
    ) {
        self.map.insert(
            weak_signature.get_signature(),
            IndexTableChunk {
                strong_signature,
                index,
            },
        );
    }
    pub fn find(&self, signature: i64) -> Option<(usize, String)> {
        let chunk = self.map.get(&signature)?;
        Some((chunk.index, chunk.strong_signature.clone()))
    }
    pub fn find_index(&self, strong_signature: String) -> Option<usize> {
        for (_, chunk) in self.map.iter() {
            if chunk.strong_signature == strong_signature {
                return Some(chunk.index);
            }
        }
        None
    }
}
