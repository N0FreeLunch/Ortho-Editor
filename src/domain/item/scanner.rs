use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header, verify_marker_lookahead};
use serde::{Serialize, Deserialize};

use rayon::prelude::*;

const SCAN_CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks for parallel scanning

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMarker {
    pub offset: u64,
    pub confidence: u32,
    pub code: String,
}

pub fn scan_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool, section_bit_offset: u64, expected_count: Option<u16>) -> Vec<ItemMarker> {
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
    
    let num_chunks = (bytes.len() + SCAN_CHUNK_SIZE - 1) / SCAN_CHUNK_SIZE;
    let markers: Vec<(u64, u32, String)> = (0..num_chunks)
        .into_par_iter()
        .flat_map(|chunk_idx| {
            let start_byte = chunk_idx * SCAN_CHUNK_SIZE;
            let end_byte = ((chunk_idx + 1) * SCAN_CHUNK_SIZE).min(bytes.len());
            
            let start_bit = (start_byte * 8) as u64;
            // Overlap by 256 bits (sufficient for any item header + some buffer)
            let _end_bit = ((end_byte * 8) as u64 + 256).min(limit_bits);
            
            let mut local_markers: Vec<(u64, u32, String)> = Vec::new();
            let mut probe = if alpha && chunk_idx == 0 { 
                // Alpha v105 forensic: Section head is JM (16) + Count (16).
                // First item starts immediately at bit 32.
                32 
            } else { 
                start_bit 
            };
            
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
                            // Alpha v105: We start at 32, so any marker found must be at or after 32.
                            if alpha && chunk_idx == 0 && scan_pos < 32 { continue; }
                            let is_known = crate::domain::forensic::v105::axioms::is_v105_summary_code(&code) || crate::domain::item::serialization::item_template(&code).is_some();
                            
                            // Slice 8: Targeted Oracle. If forced, skip lookahead.
                            let mut is_forced = false;
                            let absolute_offset = section_bit_offset + scan_pos;
                            if force_length_map.contains_key(&absolute_offset) {
                                is_forced = true;
                            }

                            // Axiom 0344: Forced 80-bit slot check for Alpha v105 summary items (potions, etc.)
                            let mut forced_80 = false;
                            if alpha && !is_compact && !is_forced {
                                let is_v105_summary = crate::domain::forensic::v105::axioms::is_v105_summary_code(&code) || code == "Þ.";
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

                            let mut confidence = if is_known { 500 } else { 50 };
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
    eprintln!("[DEBUG-SLICE13] raw candidates found: {}", final_markers.len());
    
    // Slice 14.1: Slot-Aligned Competitive Advancement.
    // We use a lookahead window to pick the highest confidence marker,
    // prioritizing those that align with the previous item's slot boundary.
    let mut filtered: Vec<(u64, u32, String)> = Vec::new();
    let mut i = 0;
    let mut last_offset = 0;
    let mut last_code = String::new();
    
    while i < final_markers.len() {
        let (offset, confidence, code) = &final_markers[i];
        
        // Find the best candidate in a lookahead window
        let mut best_idx = i;
        let mut max_score = *confidence as i32;

        if alpha && !filtered.is_empty() {
            let diff = offset - last_offset;
            if is_alpha_v105_slot_item(&last_code) {
                if diff == 80 { max_score += 350; }
                else if diff == 72 || diff == 73 { max_score += 250; }
                else if is_v105_aligned(diff) { max_score += 150; }
            } else if is_v105_aligned(diff) {
                max_score += 100;
            }

            if let Some(expected) = expected_count {
                if filtered.len() >= expected as usize {
                    if !is_v105_aligned(diff) {
                        max_score -= 500;
                    }
                }
            }
        }
        
        // Look ahead to see if there's a better (e.g. aligned) marker nearby
        let lookahead_limit = if alpha { offset + 64 } else { offset + 120 };
        let mut j = i + 1;
        while j < final_markers.len() && final_markers[j].0 < lookahead_limit {
            let (o_offset, o_conf, o_code) = &final_markers[j];
            let mut score = *o_conf as i32;
            
            if alpha && !filtered.is_empty() {
                let diff = o_offset - last_offset;
                let mut alignment_bonus = 0;
                if is_alpha_v105_slot_item(&last_code) {
                    if diff == 80 {
                        alignment_bonus = 350;
                    } else if diff == 72 || diff == 73 {
                        alignment_bonus = 250;
                    } else if is_v105_aligned(diff) {
                        alignment_bonus = 150;
                    }
                } else if is_v105_aligned(diff) {
                    alignment_bonus = 100;
                }
                score += alignment_bonus;

                // Recursive Alignment Check (Slice 7): 
                // Check if THIS lookahead candidate itself has an aligned successor.
                if alignment_bonus > 0 {
                    let next_window = o_offset + 96;
                    let mut k = j + 1;
                    while k < final_markers.len() && final_markers[k].0 < next_window {
                        let k_offset = final_markers[k].0;
                        let k_diff = k_offset - o_offset;
                        if is_v105_aligned(k_diff) {
                            score += 100; // Multi-hop alignment bonus
                            break;
                        }
                        k += 1;
                    }
                }
            }
            
            // Phantom Suppression (Slice 7): 
            // If we have an expected_count and we are over it, be more aggressive 
            // in suppressing low-confidence non-aligned markers.
            if let Some(expected) = expected_count {
                if filtered.len() >= expected as usize {
                    let is_aligned = if filtered.is_empty() { false } else { is_v105_aligned(o_offset - last_offset) };
                    if !is_aligned {
                        score -= 500; // Even higher penalty for extra unaligned markers
                    }
                }
            }

            if score > max_score {
                max_score = score;
                best_idx = j;
            }
            j += 1;
        }

            let best = &final_markers[best_idx];

            // Competitive Stopgate: if even the best score is too low or inconsistent, 
            // stop and defer to isolation.
            if alpha && max_score < 150 {
             if let Some(expected) = expected_count {
                 if filtered.len() < expected as usize {
                     // Still keep it if we really need items, but with fragile confidence
                 } else {
                     i = best_idx + 1;
                     continue;
                 }
             }
            }

        if crate::item::item_trace_enabled() {
            eprintln!("[DEBUG-SLICE13] marker accepted: offset={}, code='{}', confidence={}, score={}", best.0, best.2, best.1, max_score);
        }
        
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
    filtered.into_iter().map(|(offset, confidence, code)| ItemMarker { offset, confidence, code }).collect()
}

fn is_alpha_v105_slot_item(code: &str) -> bool {
    let trimmed = code.trim();
    // Potions, Scrolls, and other common inventory slot items in Alpha v105.
    // Also include 'w' prefixed versions common in Alpha.
    if matches!(trimmed, 
        "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|
        "whp1"|"whp2"|"whp3"|"whp4"|"whp5"|"wmp1"|"wmp2"|"wmp3"|"wmp4"|"wmp5"|
        "rvs"|"rvl"|"vps"|"tsc"|"isc"|"yps"|"wps"|"us g"|"w8cs"|"w88w"|"xrs"|
        "6cs"|"7mgw"|"fsh"|"7pus"|"ww7c"|"mxh"|"d ew"|"ghm"|"amu"|"rin"|"cm1"|
        "vbt"|"vgl"|"hbl"|"tri"|"dr1"|"key"|"mac"|"ulss"|"9tr"|"swsp"
    ) { return true; }
    
    // Check if it's a summary code from axioms
    crate::domain::forensic::v105::axioms::is_v105_summary_code(code)
}

fn is_v105_aligned(diff: u64) -> bool {
    // Standard Alpha v105 slot sizes are 72, 73, 80, 88.
    // We also allow sums of these for empty slots.
    matches!(diff, 72 | 73 | 80 | 88 | 144 | 145 | 152 | 153 | 160 | 161 | 168 | 176 | 216 | 224 | 232 | 240)
}
