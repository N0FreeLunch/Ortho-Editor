use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header, verify_marker_lookahead};
use crate::domain::item::serialization::is_v105_summary_code;
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
    
    let markers: Vec<(u64, u32, String)> = (0..chunk_count)
        .into_par_iter()
        .flat_map(|chunk_idx| {
            let start_byte = chunk_idx * SCAN_CHUNK_SIZE;
            let end_byte = ((chunk_idx + 1) * SCAN_CHUNK_SIZE).min(bytes.len());
            
            let start_bit = (start_byte * 8) as u64;
            // Overlap by 256 bits (sufficient for any item header + some buffer)
            let _end_bit = ((end_byte * 8) as u64 + 256).min(limit_bits);
            
            let mut local_markers: Vec<(u64, u32, String)> = Vec::new();
            let mut probe = start_bit;
            
            while probe < (end_byte * 8) as u64 && probe < limit_bits {
                let mut best_offset = 0;
                let mut max_confidence = 0;
                let mut best_code = String::new();

                // Try 8 possible bit-alignments within a byte (0-7)
                for offset in 0..8 {
                    let scan_pos = probe + offset;
                    if scan_pos + 128 > limit_bits { continue; }
                    
                    if let Some((mode, location, _x, code, flags, version, is_compact, _header_len, _nudge, has_checksum)) = peek_item_header_at(bytes, scan_pos, huffman, alpha) {
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
                                best_code = code.clone();
                            }
                        }
                    }
                }
                
                if max_confidence > 0 {
                    local_markers.push((best_offset, max_confidence, best_code));
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
    
    // Slice 14.1: Slot-Aligned Competitive Advancement.
    // We use a lookahead window to pick the highest confidence marker,
    // prioritizing those that align with the previous item's slot boundary.
    let mut filtered: Vec<(u64, u32, String)> = Vec::new();
    let mut i = 0;
    let mut last_offset = 0;
    let mut last_code = String::new();
    
    while i < final_markers.len() {
        let (offset, confidence, _code) = &final_markers[i];
        
        // Find the best candidate in a lookahead window
        let mut best_idx = i;
        let mut max_score = *confidence as i32;
        
        // Alignment bonus for the current candidate
        if alpha && !filtered.is_empty() && is_alpha_v105_slot_item(&last_code) && is_v105_aligned(offset - last_offset) {
            max_score += 150;
        }
        
        // Look ahead to see if there's a better (e.g. aligned) marker nearby
        let lookahead_limit = offset + 120;
        let mut j = i + 1;
        while j < final_markers.len() && final_markers[j].0 < lookahead_limit {
            let (o_offset, o_conf, _o_code) = &final_markers[j];
            let mut score = *o_conf as i32;
            if alpha && !filtered.is_empty() && is_alpha_v105_slot_item(&last_code) && is_v105_aligned(o_offset - last_offset) {
                score += 150;
            }
            
            if score > max_score {
                max_score = score;
                best_idx = j;
            }
            j += 1;
        }
        
        let best = &final_markers[best_idx];
        
        last_offset = best.0;
        last_code = best.2.clone();
        filtered.push((best.0, max_score as u32, best.2.clone()));
        
        // Advance i to after the lookahead window and skip the rest of this item's space
        let skip_until = best.0 + 72;
        i = best_idx + 1;
        while i < final_markers.len() && final_markers[i].0 < skip_until {
            i += 1;
        }
    }
    filtered.into_iter().map(|(off, _, _)| off).collect()
}

fn is_alpha_v105_slot_item(code: &str) -> bool {
    let trimmed = code.trim();
    // Potions, Scrolls, and other common inventory slot items in Alpha v105.
    matches!(trimmed, 
        "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|
        "rvs"|"rvl"|"vps"|"tsc"|"isc"|"yps"|"wps"|"us g"|"w8cs"|"w88w"|"xrs"|
        "6cs"|"7mgw"|"fsh"|"7pus"|"ww7c"|"mxh"|"d ew"|"ghm"|"amu"|"rin"|"cm1"|
        "vbt"|"vgl"|"hbl"|"tri"|"dr1"|"key"|"mac"|"ulss"|"9tr"
    )
}

fn is_v105_aligned(diff: u64) -> bool {
    // Standard Alpha v105 slot sizes are 72, 80, 88.
    // We also allow sums of these (e.g., 144, 152, 160) for empty slots.
    matches!(diff, 72 | 80 | 88 | 144 | 152 | 160 | 168 | 176 | 216 | 224 | 232 | 240)
}
