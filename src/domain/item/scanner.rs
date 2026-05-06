use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header};

pub fn scan_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> Vec<u64> {
    let mut markers = Vec::new();
    let limit = (bytes.len() * 8) as u64;
    
    // Scan bit-by-bit for plausible headers to use as anchors
    let mut probe = 0;
    while probe + 128 < limit {
        if let Some((mode, location, _x, code, flags, version, is_compact, header_len, _nudge)) = peek_item_header_at(bytes, probe, huffman, alpha) {
            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                // Heuristic: Ensure code is valid Alpha code
                if crate::domain::item::serialization::is_v105_summary_code(&code) || is_compact {
                    markers.push(probe);
                    // Fast-forward by expected header length if possible
                    probe += header_len.max(8u64);
                    continue;
                }
            }
        }
        probe += 1;
    }
    markers
}
