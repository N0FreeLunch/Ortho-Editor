use crate::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header, verify_marker_lookahead};
use serde::{Serialize, Deserialize};

use rayon::prelude::*;

const SCAN_CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks for parallel scanning

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerStatus {
    Accepted,
    Rejected,
    Phantom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMarker {
    pub offset: u64,
    pub confidence: u32,
    pub code: String,
    pub score: i32,
    pub status: MarkerStatus,
}

pub fn scan_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool, section_bit_offset: u64, expected_count: Option<u16>, verbose: bool) -> Vec<ItemMarker> {
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
            let section_header_bits = if alpha && chunk_idx == 0 {
                let mut p = 32;
                if let Some((_, _, _, _, _, version, _, _, _, _)) = peek_item_header_at(bytes, 32, huffman, alpha) {
                    p = crate::domain::forensic::v105::axioms::V105JmMarkerAxiom::default().header_bits(version) as u64;
                }
                p
            } else {
                0
            };

            let mut probe = if alpha && chunk_idx == 0 { 
                section_header_bits
            } else { 
                start_bit 
            };
            
            while probe < (end_byte * 8) as u64 && probe < limit_bits {

                let mut best_offset = 0;
                let mut max_confidence = 0;
                let mut best_code = String::new();

                for offset in 0..8 {
                    let scan_pos = probe + offset;
                    let safety_margin = 72;
                    if scan_pos + safety_margin > limit_bits { continue; }
                    
                    if let Some((mode, location, _x, code, flags, version, is_compact, _header_len, _nudge, has_checksum)) = peek_item_header_at(bytes, scan_pos, huffman, alpha) {
                        if is_plausible_item_header(mode, location, code.as_bytes(), flags, version, alpha) {
                            let is_known = crate::domain::forensic::v105::axioms::is_v105_summary_code(&code) || crate::domain::item::serialization::item_template(&code).is_some();
                            
                            // Slice S3: Stricter parity. Alpha v105 items must have a valid checksum unless they are known summary/templated items.
                            if alpha && !has_checksum && !is_known { continue; }
                            
                            // Alpha v105: We start at section_header_bits, so any marker found must be at or after it.
                            if alpha && chunk_idx == 0 && scan_pos < section_header_bits { continue; }
                            
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
                                        if is_plausible_item_header(n_mode, n_loc, n_code.as_bytes(), n_flags, n_ver, alpha) {
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
                    // Slice S3: Safe algorithmic jump. If we are highly confident (is_known),
                    // we can safely skip the known minimum item length (72 bits) to avoid phantoms.
                    if max_confidence >= 500 {
                        probe = best_offset + 72;
                    } else {
                        probe = best_offset + 8;
                    }
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
    // eprintln!("[DEBUG-SLICE13] raw candidates found: {}", final_markers.len());
    
    // Slice 14.1: Slot-Aligned Competitive Advancement.
    // We use a lookahead window to pick the highest confidence marker,
    // prioritizing those that align with the previous item's slot boundary.
    let mut all_markers: Vec<ItemMarker> = Vec::new();
    let mut filtered_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    
    let mut i = 0;
    let mut last_offset = 0;
    let mut last_code = String::new();
    let mut accepted_count = 0;
    
    while i < final_markers.len() {
        let (offset, confidence, _code) = &final_markers[i];
        
        // Find the best candidate in a lookahead window
        let mut best_idx = i;
        let mut max_score = *confidence as i32;

        if alpha && accepted_count > 0 {
            let diff = offset - last_offset;
            if is_alpha_v105_slot_item(&last_code) {
                if diff == 80 { max_score += 350; }
                else if diff == 72 || diff == 73 { max_score += 250; }
                else if is_v105_aligned(diff) { max_score += 150; }
            } else if is_v105_aligned(diff) {
                max_score += 100;
            }

            if let Some(expected) = expected_count {
                if accepted_count >= expected as usize {
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
            let (o_offset, o_conf, _o_code) = &final_markers[j];
            let mut score = *o_conf as i32;
            
            if alpha && accepted_count > 0 {
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
                if accepted_count >= expected as usize {
                    let is_aligned = if accepted_count == 0 { false } else { is_v105_aligned(o_offset - last_offset) };
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

            let (best_offset, best_confidence, best_code_str) = &final_markers[best_idx];
            let mut status = MarkerStatus::Accepted;

            // Competitive Stopgate: if even the best score is too low or inconsistent, 
            // stop and defer to isolation.
            if alpha && max_score < 150 {
             if let Some(expected) = expected_count {
                 if accepted_count < expected as usize {
                     // Still keep it if we really need items, but with fragile confidence
                 } else {
                     status = MarkerStatus::Phantom;
                 }
             } else {
                 status = MarkerStatus::Phantom;
             }
            }

        if crate::item::item_trace_enabled() {
            eprintln!("[DEBUG-SLICE13] marker processed: offset={}, code='{}', confidence={}, score={}, status={:?}", best_offset, best_code_str, best_confidence, max_score, status);
        }
        
        if status == MarkerStatus::Accepted || status == MarkerStatus::Phantom {
            last_offset = *best_offset;
            last_code = best_code_str.clone();
            if status == MarkerStatus::Accepted {
                accepted_count += 1;
                filtered_indices.insert(best_idx);
                all_markers.push(ItemMarker {
                    offset: *best_offset,
                    confidence: *best_confidence,
                    code: best_code_str.clone(),
                    score: max_score,
                    status: MarkerStatus::Accepted,
                });
            } else if verbose {
                all_markers.push(ItemMarker {
                    offset: *best_offset,
                    confidence: *best_confidence,
                    code: best_code_str.clone(),
                    score: max_score,
                    status: status,
                });
            }
        } else if verbose {
            all_markers.push(ItemMarker {
                offset: *best_offset,
                confidence: *best_confidence,
                code: best_code_str.clone(),
                score: max_score,
                status: status,
            });
        }
        
        // Advance i to after the lookahead window and skip the rest of this item's space
        let skip_until = best_offset + 72;
        i = best_idx + 1;
        while i < final_markers.len() && final_markers[i].0 < skip_until {
            if verbose && !filtered_indices.contains(&i) {
                let (o_offset, o_conf, o_code) = &final_markers[i];
                all_markers.push(ItemMarker {
                    offset: *o_offset,
                    confidence: *o_conf,
                    code: o_code.clone(),
                    score: *o_conf as i32, // Raw score for skipped
                    status: MarkerStatus::Rejected,
                });
            }
            i += 1;
        }
    }

    if verbose {
        all_markers.sort_unstable_by_key(|m| m.offset);
        all_markers
    } else {
        all_markers.into_iter().filter(|m| m.status == MarkerStatus::Accepted).collect()
    }
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
