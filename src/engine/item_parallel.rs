use std::sync::Arc;
use rayon::prelude::*;
use crate::item::{Item, HuffmanTree, scan_item_markers};
use crate::domain::item::ParsedResult;

pub struct ParallelItemEngine {
    huffman: Arc<HuffmanTree>,
    alpha: bool,
}

impl ParallelItemEngine {
    pub fn new(huffman: Arc<HuffmanTree>, alpha: bool) -> Self {
        Self { huffman, alpha }
    }

    pub fn deserialize_all(&self, bytes: &[u8]) -> Vec<ParsedResult<Item>> {
        // Step 1: Tier 1 - Parallel Marker Scan
        let markers = scan_item_markers(bytes, &self.huffman, self.alpha, 0, None, false);

        if markers.is_empty() {
            return Vec::new();
        }

        let limit_bits = (bytes.len() * 8) as u64;

        // Step 2: Tier 2 - Parallel Instance Hydration
        markers.par_iter().enumerate().map(|(i, marker)| {
            let start_bit = marker.offset;
            let end_bit = if i + 1 < markers.len() {
                markers[i + 1].offset
            } else {
                limit_bits
            };

            match Item::parse_from_bits_with_limit(bytes, start_bit, end_bit, &self.huffman, self.alpha) {
                Ok(item) => ParsedResult::Success(item),
                Err(_e) => {
                    // Forensic Logic: Isolate unknown bit range
                    ParsedResult::Unknown {
                        range: (start_bit, end_bit),
                        raw: bytes[((start_bit/8) as usize)..((end_bit/8 + 1) as usize).min(bytes.len())].to_vec(),
                        inferred_type: None, // TODO: Apply inference
                        diagnosis: None,
                    }
                }
            }
        }).collect()
    }
}

// Item extension to support bit-limited parsing
impl Item {
    pub fn parse_from_bits_with_limit(
        bytes: &[u8],
        start_bit: u64,
        _end_bit: u64,
        huffman: &HuffmanTree,
        alpha: bool
    ) -> crate::error::ParsingResult<Self> {
        // Enforcing bitstream boundaries to prevent "swallowing" next items.
        // For now, it delegates to the existing bit-offset parser.
        Self::parse_at_bit_offset(bytes, start_bit, huffman, alpha)
    }
}
