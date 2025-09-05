#[derive(Debug, Clone)]
pub struct Block {
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct OutputBlock {
    data: Vec<u8>,
}

impl OutputBlock {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}
