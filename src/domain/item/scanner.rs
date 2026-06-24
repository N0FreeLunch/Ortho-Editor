use crate::item::{
    HuffmanTree,
    peek_item_header_at,
    peek_item_header_at_specific_gap,
    is_plausible_item_header,
    verify_marker_lookahead,
};
use serde::{Serialize, Deserialize};

use rayon::prelude::*;

fn is_alpha_v105_shadow_marker(code: &str) -> bool {
    let trimmed = code.trim();
    // Shadow markers observed in Alpha v105 runewords/socketed items.
    matches!(trimmed, "c8xr" | "wa2" | "rhd") || (trimmed.starts_with('r') && trimmed.len() <= 3 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
}

fn is_alpha_v105_authority_marker(code: &str) -> bool {
    matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2")
}

fn is_alpha_v105_socket_child_marker(code: &str) -> bool {
    let trimmed = code.trim();
    if matches!(trimmed, "jew" | "gcw" | "ww") {
        return true;
    }

    let bytes = trimmed.as_bytes();
    bytes.len() >= 2
        && bytes.len() <= 3
        && bytes[0] == b'r'
        && bytes[1..].iter().all(|b| b.is_ascii_digit())
}

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

    let limit_bits = (bytes.len() * 8) as u64;
    
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
        .into_iter()
        .flat_map(|chunk_idx| {
            let start_byte = chunk_idx * SCAN_CHUNK_SIZE;
            let end_byte = ((chunk_idx + 1) * SCAN_CHUNK_SIZE).min(bytes.len());
            
            let start_bit = (start_byte * 8) as u64;
            let _end_bit = ((end_byte * 8) as u64 + 256).min(limit_bits);
            
            let mut local_markers: Vec<(u64, u32, String)> = Vec::new();
            let section_header_bits = if alpha && chunk_idx == 0 {
                let mut p = 32;
                if let Some((version, _, _, _, _, _, _, _, _, _)) = peek_item_header_at(bytes, 32, huffman, alpha, 0) {
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

                let nudge_range = 8;
                for offset in 0..nudge_range {
                    let scan_pos = probe + offset;
                    let safety_margin = 72;
                    if scan_pos + safety_margin > limit_bits { continue; }
                    
                    let mut header_candidate =
                        peek_item_header_at(bytes, scan_pos, huffman, alpha, 0);
                    if alpha {
                        let reg = crate::domain::forensic::registry::get_registry();
                        for alt_gap in [6u64, 35, 46] {
                            if let Some((mode, location, _x, code, flags, version, is_compact, _header_len, _nudge, has_checksum)) =
                                peek_item_header_at_specific_gap(bytes, scan_pos, huffman, alpha, alt_gap)
                            {
                                let trimmed_alt = code.trim();
                                let mut is_auth = false;
                                if let Some(overrides) = &reg.item_overrides {
                                    if let Some(map) = overrides.get(trimmed_alt) {
                                        if let Some(&val) = map.get("is_authority_overlap") { is_auth = val != 0; }
                                    }
                                }
                                if is_auth {
                                    header_candidate = Some((
                                        mode,
                                        location,
                                        _x,
                                        code,
                                        flags,
                                        version,
                                        is_compact,
                                        _header_len,
                                        _nudge,
                                        has_checksum,
                                    ));
                                    break;
                                }
                            }
                        }
                    }

                    if let Some((mode, location, _x, code, flags, version, is_compact, _header_len, _nudge, has_checksum)) = header_candidate {   
                        if is_plausible_item_header(mode, location, code.as_bytes(), flags, version, alpha) {
                            let trimmed_code = code.trim();
                            let is_known = crate::domain::forensic::v105::axioms::is_v105_summary_code(&code) 
                                || crate::domain::item::serialization::item_template(&code).is_some();
                            let reg = crate::domain::forensic::registry::get_registry();
                            let override_noncompact = reg.item_overrides.as_ref()
                                .and_then(|overrides| overrides.get(trimmed_code))
                                .and_then(|map| map.get("is_compact"))
                                .map(|&val| val == 0)
                                .unwrap_or(false);
                            
                            if alpha && version == 5 && !has_checksum && !is_known { continue; }
                            if alpha && chunk_idx == 0 && scan_pos < section_header_bits { continue; }
                            
                            let mut is_forced = false;
                            let absolute_offset = section_bit_offset + scan_pos;
                            if force_length_map.contains_key(&absolute_offset) {
                                is_forced = true;
                            }

                            let is_v105_summary = crate::domain::forensic::v105::axioms::is_v105_summary_code(&code);
                            
                            let mut forced_80 = false;
                            if alpha && !is_compact && !is_forced {
                                if is_v105_summary {
                                    if let Some(next_header) = peek_item_header_at(bytes, scan_pos + 80, huffman, alpha, 0) {
                                        let (n_mode, n_loc, _, n_code, n_flags, n_ver, _, _, _, _) = next_header;
                                        if is_plausible_item_header(n_mode, n_loc, n_code.as_bytes(), n_flags, n_ver, alpha) {
                                            forced_80 = true;
                                        }
                                    }
                                }
                            }

                            let mut confidence = if is_known { 500 } else { 50 };
                            let header_axiom = crate::domain::header::entity::HeaderAxiom::new(version, alpha);
                            let is_alpha_runeword_candidate = alpha && header_axiom.is_runeword(flags, Some(&code), has_checksum);
                            if override_noncompact {
                                confidence += 300;
                            }

                            if alpha && !is_compact && !is_forced && !forced_80 && !(is_alpha_runeword_candidate && has_checksum) && !is_v105_summary && !override_noncompact {
                                if !verify_marker_lookahead(bytes, scan_pos + _header_len, huffman, alpha) {
                                    continue;
                                }
                            }

                            if alpha && (trimmed_code == "hp1" || trimmed_code == "wyws" || trimmed_code == "xrs" || trimmed_code == "wa2") {
                                confidence += 200;
                            }
                            if alpha && is_alpha_runeword_candidate && (trimmed_code == "xrs" || trimmed_code == "wa2") {
                                confidence += 300;
                            }
                            if alpha && version == 5 {
                                confidence += 100;
                            }
                            if alpha && has_checksum {
                                confidence += 100;
                            }
                            
                            if alpha {
                                let rem = scan_pos % 8;
                                let body_start = scan_pos + _header_len;
                                if body_start % 8 == 5 {
                                    confidence += 500;
                                }
                                
                                if trimmed_code == "hp1" || trimmed_code == "mp1" {
                                    if has_checksum {
                                        confidence += 1000;
                                    } else {
                                        continue;
                                    }
                                }
                                
                                if trimmed_code == "xrs" || trimmed_code == "c8xr" || trimmed_code == "rhd" || trimmed_code == "wa2" {
                                     if rem == 2 {
                                         confidence += 1000;
                                     } else if (trimmed_code == "wa2" || trimmed_code == "rhd") && rem == 6 {
                                         confidence += 1000;
                                     } else {
                                         continue;
                                     }
                                }
                            }

                            if confidence > max_confidence 
                                || (alpha && confidence == max_confidence && (scan_pos % 8 == 2))
                            {
                                max_confidence = confidence;
                                best_offset = scan_pos;
                                best_code = code.clone();
                            }
                        }
                    }
                }

                if max_confidence > 0 {
                    local_markers.push((best_offset, max_confidence, best_code.clone()));

                    let jump = if alpha {
                        if let Some((_, _, _, _, f, v, _, _, _, _)) = peek_item_header_at(bytes, best_offset, huffman, alpha, 0) {
                             let mut j = crate::domain::forensic::v105::axioms::get_v105_target_width(v, &best_code, f, Some(local_markers.len())) as u64;
                             if (best_code.trim() == "xrs" || best_code.trim() == "wa2") && j == 0 {
                                 j = 150; 
                             }
                             if j > 0 { j } else { 72 }
                        } else { 72 }

                    } else {
                        72 
                    };
                    probe = best_offset + jump;
                } else {
                    probe += 8;
                }
            }
            local_markers
        })
        .collect();

    let mut final_markers = markers;
    final_markers.sort_unstable_by_key(|m| m.0);
    
    let mut all_markers: Vec<ItemMarker> = Vec::new();
    let mut filtered_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    
    let mut i = 0;
    let mut last_offset = 0;
    let mut last_code = String::new();
    let mut accepted_count = 0;
    let mut since_last_auth: Option<(u64, u64)> = None;
    
    while i < final_markers.len() {
        let (offset, confidence, code_str) = &final_markers[i];
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

            if let Some((auth_off, auth_limit)) = since_last_auth {
                let dist = offset - auth_off;
                if dist < auth_limit
                    && is_alpha_v105_shadow_marker(code_str)
                    && !is_alpha_v105_socket_child_marker(code_str)
                {
                    max_score -= 1500;
                }
            }
        }
        
        let lookahead_limit = if alpha {
            if is_alpha_v105_compact_limit_item(code_str) { offset + 72 } else { offset + 128 }
        } else {
            offset + 120
        };
        let mut j = i + 1;
        while j < final_markers.len() && final_markers[j].0 < lookahead_limit {
            let (o_offset, o_conf, o_code) = &final_markers[j];
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

                if let Some((auth_off, auth_limit)) = since_last_auth {
                    let dist = o_offset - auth_off;
                    if dist < auth_limit
                        && is_alpha_v105_shadow_marker(o_code)
                        && !is_alpha_v105_socket_child_marker(o_code)
                    {
                        score -= 1500;
                    }
                }

                if alpha {
                    let next_window = if is_alpha_v105_compact_limit_item(o_code) { o_offset + 72 } else { o_offset + 128 };
                    let mut k = j + 1;
                    while k < final_markers.len() && final_markers[k].0 < next_window {
                        let k_offset = final_markers[k].0;
                        let k_diff = k_offset - o_offset;
                        if is_v105_aligned(k_diff) {
                            if k_diff == 80 || k_diff == 72 {
                                score += 300; 
                            } else {
                                score += 100;
                            }
                            break;
                        }
                        k += 1;
                    }
                }
            }
            
            if let Some(expected) = expected_count {
                if accepted_count >= expected as usize {
                    let is_aligned = if accepted_count == 0 { false } else { is_v105_aligned(o_offset - last_offset) };
                    if !is_aligned {
                        let following_auth = since_last_auth
                            .map(|(off, limit)| o_offset - off < limit)
                            .unwrap_or(false);
                        if !following_auth || !is_alpha_v105_socket_child_marker(o_code) {
                            score -= 500;
                        } else {
                            score -= 100;
                        }
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

        if alpha && max_score < 150 {
            if let Some(expected) = expected_count {
                let following_auth = since_last_auth
                    .map(|(off, limit)| best_offset - off < limit)
                    .unwrap_or(false);
                if following_auth && is_alpha_v105_socket_child_marker(best_code_str) {
                    status = MarkerStatus::Accepted;
                } else if following_auth && is_alpha_v105_shadow_marker(best_code_str) {
                    status = MarkerStatus::Phantom;
                } else if accepted_count < expected as usize {
                } else {
                    status = MarkerStatus::Phantom;
                }
            } else {
                status = MarkerStatus::Phantom;
            }
        }

        if status == MarkerStatus::Accepted || status == MarkerStatus::Phantom {
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
                last_offset = *best_offset;
                last_code = best_code_str.clone();
                
                if is_alpha_v105_authority_marker(&last_code) {
                    let limit = if last_code.trim() == "rhd" { 128 } else { 512 };
                    since_last_auth = Some((last_offset, limit));
                } else if let Some((off, limit)) = since_last_auth {
                    if last_offset - off >= limit {
                        since_last_auth = None;
                    }
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
        } else if verbose {
            all_markers.push(ItemMarker {
                offset: *best_offset,
                confidence: *best_confidence,
                code: best_code_str.clone(),
                score: max_score,
                status: status,
            });
        }
        
        let skip_until = best_offset + 72;
        i = best_idx + 1;
        while i < final_markers.len() && final_markers[i].0 < skip_until {
            if verbose && !filtered_indices.contains(&i) {
                let (o_offset, o_conf, o_code) = &final_markers[i];
                all_markers.push(ItemMarker {
                    offset: *o_offset,
                    confidence: *o_conf,
                    code: o_code.clone(),
                    score: *o_conf as i32,
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
    if matches!(trimmed, 
        "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|
        "whp1"|"whp2"|"whp3"|"whp4"|"whp5"|"wmp1"|"wmp2"|"wmp3"|"wmp4"|"wmp5"|
        "rvs"|"rvl"|"vps"|"tsc"|"isc"|"jav"|"yps"|"wps"|"w8cs"|"w88w"|"xrs"|
        "wyws"|
        "6cs"|"7mgw"|"fsh"|"7pus"|"ww7c"|"mxh"|"d ew"|"ghm"|"amu"|"rin"|"cm1"|
        "vbt"|"vgl"|"hbl"|"tri"|"dr1"|"key"|"mac"|"ulss"|"9tr"|"swsp"
    ) { return true; }
    crate::domain::forensic::v105::axioms::is_v105_summary_code(code)
}

fn is_alpha_v105_compact_limit_item(code: &str) -> bool {
    let trimmed = code.trim();
    if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed) {
        return true;
    }
    matches!(trimmed, 
        "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|
        "whp1"|"whp2"|"whp3"|"whp4"|"whp5"|"wmp1"|"wmp2"|"wmp3"|"wmp4"|"wmp5"|
        "rvs"|"rvl"|"vps"|"tsc"|"isc"|"yps"|"wps"|"w8cs"|"w88w"|
        "wyws"|"e w"|"key"|"mac"|"ulss"|"9tr"
    )
}

fn is_v105_aligned(diff: u64) -> bool {
    matches!(diff, 72 | 73 | 74 | 80 | 81 | 82 | 88 | 89 | 90 | 144 | 145 | 146 | 152 | 153 | 154 | 160 | 161 | 162 | 168 | 176 | 216 | 224 | 232 | 240)
}
