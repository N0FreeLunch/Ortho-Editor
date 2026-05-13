use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header, verify_marker_lookahead};
use crate::domain::item::quality::ItemQuality;
use crate::domain::stats::axiom::StatsAxiom;
use rayon::prelude::*;

const SCAN_CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks for parallel scanning

pub fn scan_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool, section_bit_offset: u64) -> Vec<u64> {
    if bytes.is_empty() {
        return Vec::new();
    }

    // Tier 1: Parallel Structural Indexing using Rayon
    // We split the byte stream into chunks and scan each chunk in parallel.
    // To avoid missing markers straddling chunk boundaries, we overlap chunks slightly.
    let limit_bits = (bytes.len() * 8) as u64;
    
    // Slice 8: Parse D2R_FORCE_LENGTH to ensure true markers are not dropped
    let mut force_length_map = std::collections::HashMap::new();
    if let Ok(env_val) = std::env::var("D2R_FORCE_LENGTH") {
        for pair in env_val.split(',') {
            let parts: Vec<&str> = pair.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(offset), Ok(length)) = (parts[0].trim().parse::<u64>(), parts[1].trim().parse::<u64>()) {
                    force_length_map.insert(offset, length);
                }
            }
        }
    }
    
    let chunk_count = (bytes.len() + SCAN_CHUNK_SIZE - 1) / SCAN_CHUNK_SIZE;
    
    let markers: Vec<(u64, u32)> = (0..chunk_count)
        .into_par_iter()
        .flat_map(|chunk_idx| {
            let start_byte = chunk_idx * SCAN_CHUNK_SIZE;
            let end_byte = ((chunk_idx + 1) * SCAN_CHUNK_SIZE).min(bytes.len());
            
            let start_bit = (start_byte * 8) as u64;
            // Overlap by 256 bits (sufficient for any item header + some buffer)
            let _end_bit = ((end_byte * 8) as u64 + 256).min(limit_bits);
            
            let mut local_markers: Vec<(u64, u32)> = Vec::new();
            let mut probe = start_bit;
            
            while probe < (end_byte * 8) as u64 && probe < limit_bits {
                let mut best_offset = 0;
                let mut max_confidence = 0;

                // Try 8 possible bit-alignments within a byte (0-7)
                for offset in 0..8 {
                    let scan_pos = probe + offset;
                    if scan_pos + 128 > limit_bits { continue; }
                    
                    if let Some((mode, location, _x, code, flags, version, is_compact, _header_len, _nudge, has_checksum)) = peek_item_header_at(bytes, scan_pos, huffman, alpha) {
                        if (section_bit_offset + scan_pos) == 7256 {
                            println!("[DEBUG-SLICE12] SCAN item at 7256: mode={}, loc={}, code={}, flags=0x{:X}, v={}, ck={}", mode, location, code, flags, version, has_checksum);
                        }
                        if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                            let is_known = crate::domain::item::serialization::is_v105_summary_code(&code) || crate::domain::item::serialization::item_template(&code).is_some();
                            if alpha && !is_known {
                                continue;
                            }
                            
                            // Slice 8: Targeted Oracle. If forced, skip lookahead.
                            let mut is_forced = false;
                            let absolute_offset = section_bit_offset + scan_pos;
                            if force_length_map.contains_key(&absolute_offset) {
                                is_forced = true;
                            }

                            // Axiom 0344: Forced 80-bit slot check for Alpha v105 summary items (potions, etc.)
                            let mut forced_80 = false;
                            if alpha && !is_compact && !is_forced {
                                let is_v105_summary = crate::domain::item::serialization::is_v105_summary_code(&code);
                                if is_v105_summary {
                                    if let Some(next_header) = peek_item_header_at(bytes, scan_pos + 80, huffman, alpha) {
                                        let (n_mode, n_loc, _, n_code, n_flags, n_ver, _, _, _, _) = next_header;
                                        if is_plausible_item_header(n_mode, n_loc, &n_code, n_flags, n_ver, alpha) {
                                            forced_80 = true;
                                        }
                                    }
                                }
                            }

                            if alpha && !is_compact && !is_forced && !forced_80 {
                                if !verify_marker_lookahead(bytes, scan_pos + _header_len, huffman, alpha) {
                                    continue;
                                }
                            }

                            let mut confidence = if is_known { 200 } else { 50 };
                            if alpha && version == 5 {
                                confidence += 100;
                            }
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
                    local_markers.push((best_offset, max_confidence));
                    probe = best_offset + 8;
                } else {
                    probe += 8;
                }
            }
            local_markers
        })
        .collect();


    // Consolidate markers: sort and remove duplicates caused by overlapping scan
    let mut final_markers = markers;
    final_markers.sort_unstable_by_key(|m| m.0);
    
    // Slice 14: Tighten dedup to avoid ghost markers causing "limit" truncation in read_section.
    // We use a 32-bit sliding window to pick the highest confidence marker in a neighborhood.
    // 32 bits is a safe minimum (width of a JM header) to prune redundant hits on the same marker.
    let mut filtered: Vec<(u64, u32)> = Vec::new();
    for (offset, confidence) in final_markers {
        if let Some(last) = filtered.last_mut() {
            if offset < last.0 + 32 {
                if confidence > last.1 {
                    *last = (offset, confidence);
                }
                continue;
            }
        }
        filtered.push((offset, confidence));
    }
    filtered.into_iter().map(|(off, _)| off).collect()
}
