use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header};

pub fn scan_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> Vec<u64> {
    let mut markers = Vec::new();
    let limit = (bytes.len() * 8) as u64;
    
    let mut probe = 0;
    while probe < limit {
        let mut best_offset = 0;
        let mut max_confidence = 0;

        for offset in 0..8 {
            let scan_pos = probe + offset;
            if scan_pos + 128 > limit { continue; }
            
            if let Some((mode, location, _x, code, flags, version, _is_compact, _header_len, _nudge, has_checksum)) = peek_item_header_at(bytes, scan_pos, huffman, alpha) {
                if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                    let mut confidence = if crate::domain::item::serialization::is_v105_summary_code(&code) { 100 } else { 50 };
                    if alpha && has_checksum {
                        confidence += 100;
                    }
                    if confidence > max_confidence {
                        max_confidence = confidence;
                        best_offset = scan_pos;
                    }
                }
            }
        }
        
        if max_confidence > 0 {
            markers.push(best_offset);
            probe = best_offset + 32; 
        } else {
            probe += 8;
        }
    }
    markers.sort();
    markers.dedup();
    markers
}
