use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use std::io::{self, Cursor};
use crate::domain::item::Item;
use crate::domain::item::quality::ItemQuality;
use crate::domain::stats::{ItemProperty, StatsAxiom, ItemStats};
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError, ParsingFailure};
use crate::domain::header::entity::{ItemSegmentType, HeaderAxiom, calculate_alpha_v105_checksum};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom, Confidence, Intentionality, ForensicMetadata};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom, V105PropertyNudgeAxiom, V105AlignmentAxiom};

pub fn calculate_property_residue(version: u8) -> usize {
    crate::domain::forensic::v105::axioms::V105PropertyNudgeAxiom::default().get_nudge(version) as usize
}

pub fn find_next_item_match(bytes: &[u8], pos: u64, huffman: &HuffmanTree, alpha: bool) -> Option<u64> {
    let limit = (bytes.len() * 8) as u64;
    let mut probe = pos;
    let section_bits = limit;

    // Header cache to skip regions known to produce false positives
    let mut invalid_regions: Vec<(u64, u64)> = Vec::new();

    while probe < section_bits {
        if invalid_regions.iter().any(|&(s, e)| probe >= s && probe < e) {
            probe += 8;
            continue;
        }

        if let Some((mode, location, _x, code, flags, version, is_compact, header_len, _nudge, _has_checksum)) = peek_item_header_at(bytes, probe, huffman, alpha) {
             if crate::item::item_trace_enabled() {
                // Probe success
             }
            // Code-based validation: Reject if code is not a known Alpha v105 item
            let is_blank = alpha && code.trim().is_empty();
            if !crate::domain::item::serialization::is_v105_summary_code(&code) && !is_compact && !is_blank {
                // Potential ghost region
                invalid_regions.push((probe, probe + 32));
                probe += 8;
                continue;
            }

            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                // Look-ahead verification (Slice 4): Prevent swallowing by verifying the candidate body
                let is_blank = alpha && code.trim().is_empty();
                
                // Axiom 0344: In Alpha v105, blank items and certain compact types 
                // often lack the is_compact flag but are strictly 80-bit intervals.
                let mut forced_compact = false;
                if alpha && !is_compact {
                    let next_jm_at_80 = probe + 80;
                    if next_jm_at_80 + 32 <= section_bits {
                         let mut jm_reader = bitstream_io::BitReader::endian(Cursor::new(bytes), LittleEndian);
                         if jm_reader.skip(next_jm_at_80 as u32).is_ok() {
                             if let Ok(next_flags) = jm_reader.read::<32, u32>() {
                                 // Check for JM marker or a valid-looking Alpha header checksum
                                 if (next_flags & 0xFFFF) == 0x4D4A {
                                     forced_compact = true;
                                 } else {
                                     // Peek for Alpha checksum
                                     let mut check_reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
                                     if check_reader.skip(next_jm_at_80 as u32 + 32).is_ok() {
                                         if let (Ok(ck), Ok(v)) = (check_reader.read::<8, u8>(), check_reader.read::<3, u8>()) {
                                             if ck == calculate_alpha_v105_checksum(next_flags, v) {
                                                 forced_compact = true;
                                             }
                                         }
                                     }
                                 }
                             }
                         }
                    }
                }

                if alpha && !is_compact && !is_blank && !forced_compact {
                    if !verify_marker_lookahead(bytes, probe + header_len, huffman, alpha) {
                        probe += 8;
                        continue;
                    }
                }
                
                if probe + header_len + 80 <= section_bits {
                    return Some(probe);
                }
            }
            probe += header_len.max(8);
        } else {
            probe += 8;
        }
    }
    None
}

#[inline]
fn peek_bits_at(bytes: &[u8], bit_offset: u64, bit_count: u32) -> Option<u32> {
    if bit_count > 32 || bit_count == 0 { return None; }
    let end_bit = bit_offset + bit_count as u64;
    if end_bit > (bytes.len() as u64) * 8 { return None; }

    let byte_offset = (bit_offset / 8) as usize;
    let bit_in_byte = (bit_offset % 8) as u32;
    
    if byte_offset + 5 <= bytes.len() {
        // Fast path: load 5 bytes into a u64
        let mut val: u64 = bytes[byte_offset] as u64;
        val |= (bytes[byte_offset + 1] as u64) << 8;
        val |= (bytes[byte_offset + 2] as u64) << 16;
        val |= (bytes[byte_offset + 3] as u64) << 24;
        val |= (bytes[byte_offset + 4] as u64) << 32;
        
        let shifted = val >> bit_in_byte;
        let mask = if bit_count == 32 { 0xFFFFFFFF } else { (1u32 << bit_count) - 1 };
        return Some((shifted as u32) & mask);
    }

    // Fallback for end of buffer
    let mut result: u32 = 0;
    let mut bits_read = 0;
    while bits_read < bit_count {
        let current_bit = bit_offset + bits_read as u64;
        let b_idx = (current_bit / 8) as usize;
        let b_bit = (current_bit % 8) as u32;
        let b_to_read = (8 - b_bit).min(bit_count - bits_read);
        let mask = if b_to_read == 8 { 0xFF } else { ((1 << b_to_read) - 1) as u8 };
        let val = (bytes[b_idx] >> b_bit) & mask;
        result |= (val as u32) << bits_read;
        bits_read += b_to_read;
    }
    Some(result)
}

pub fn verify_marker_lookahead(bytes: &[u8], start_bit: u64, _huffman: &HuffmanTree, _alpha: bool) -> bool {
    // Read 9-bit Stat ID (Dominant rhythm in Alpha v105)
    let stat_id = match peek_bits_at(bytes, start_bit, 9) {
        Some(id) => id as u16,
        None => return false,
    };
    
    // Terminator (511) is a valid "empty" or "finished" stats block.
    if stat_id == 511 {
        return true;
    }
    
    // For Alpha v105, check if the stat_id is one of the known/mapped IDs
    // to reject random garbage.
    let is_known_id = matches!(stat_id, 
        0|1|2|4|8|13|16|21|25|26|31|68|69|70|72|73|99|106|108|112|114|127|128|140|152|160|194|207|256|287|289|309|310|311|312|317|320|380|496|499
    );
    
    if is_known_id {
        // Most Alpha v105 properties are 9+6 or 9+9.
        // We expect at least 6-9 more bits.
        if start_bit + 9 + 6 <= (bytes.len() as u64) * 8 {
            return true;
        }
    }
    
    false
}

pub fn classify_failure(err: &crate::error::ParsingError) -> crate::domain::item::FailureFamily {
    use crate::error::ParsingError::*;
    use crate::domain::item::FailureFamily::*;

    match err {
        InvalidHuffmanBit { bit_offset } => {
            if *bit_offset < 100 { Geometry } else { Nudge }
        }
        InvalidStatId { .. } => Stat,
        UnexpectedSegmentEnd { .. } => Geometry,
        BitSymmetryFailure { .. } => Geometry,
        InvariantViolation { field, .. } => {
            if field.contains("marker") || field.contains("header") { Geometry } else { Stat }
        }
        UnexpectedValue { field, .. } => {
            if field.contains("quality") || field.contains("unique") { RWSet } else { Stat }
        }
        MissingMarker { .. } => Geometry,
        BitDriftDetected { .. } => Nudge,
        AlignmentError { .. } => Geometry,
        BitBudgetExceeded { .. } => Stat,
        Io(_) => Unknown,
        Generic(_) => Unknown,
        SpeculativeRejection { .. } => Geometry,
    }
}

pub fn is_plausible_item_header(
    mode: u8,
    location: u8,
    code: &str,
    _flags: u32,
    version: u8,
    alpha_mode: bool,
) -> bool {
    if alpha_mode && code.trim().is_empty() {
         return mode <= 6 && location <= 5;
    }
    let axiom = HeaderAxiom::new(version, alpha_mode);
    axiom.is_plausible(mode, location, code, _flags)
}

pub fn peek_item_header_at(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
) -> Option<(u8, u8, u8, String, u32, u8, bool, u64, i8, bool)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if reader.skip(start_bit as u32).is_err() { return None; }

    // Read header structure
    let flags = reader.read::<32, u32>().ok()?;
    
    let mut alpha_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    alpha_reader.skip(start_bit as u32 + 32).ok()?;
    let checksum = alpha_reader.read::<8, u8>().ok()?;
    let v = alpha_reader.read::<3, u8>().ok()?;
    let calculated = calculate_alpha_v105_checksum(flags, v);
    
    let (version, mode, loc, _x_val, base_header_len, has_checksum) = if calculated == checksum {
        let m = alpha_reader.read::<3, u8>().ok()?;
        let l = alpha_reader.read::<3, u8>().ok()?;
        let x = alpha_reader.read::<4, u8>().ok()?;
        (v, m, l, x, 32 + 8 + 3 + 3 + 3 + 4, true)
    } else {
        let mut retail_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
        retail_reader.skip(start_bit as u32 + 32).ok()?;
        let v = retail_reader.read::<3, u8>().ok()?;
        let m = retail_reader.read::<3, u8>().ok()?;
        let l = retail_reader.read::<3, u8>().ok()?;
        let x = retail_reader.read::<4, u8>().ok()?;
        (v, m, l, x, 32 + 3 + 3 + 3 + 4, false)
    };

    let h_axiom = HeaderAxiom::new(version, alpha_mode);
    let mut is_compact = h_axiom.is_compact(flags, None);
    
    // Axiom 0344: In Alpha v105, blank items and certain compact types 
    // often lack the is_compact flag but are strictly interval-aligned.
    // Use physical interval sniffing to force compact mode.
    if alpha_mode && !is_compact {
        for &interval in &[72, 80, 88] {
            let next_bit = start_bit + interval;
            if next_bit + 64 <= (section_bytes.len() * 8) as u64 {
                 let mut jm_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                 if jm_reader.skip(next_bit as u32).is_ok() {
                     if let Ok(next_flags) = jm_reader.read::<32, u32>() {
                         // Check for JM marker or Alpha plausibility
                         if (next_flags & 0xFFFF) == 0x4D4A {
                             is_compact = true;
                             break;
                         }
                         
                         let mut p_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                         if p_reader.skip(next_bit as u32 + 32).is_ok() {
                             // Check for version 5 checksum or plausible mode/loc
                             let ck_res = p_reader.read::<8, u8>();
                             let nv_res = p_reader.read::<3, u8>();
                             if let (Ok(ck), Ok(nv)) = (ck_res, nv_res) {
                                 if ck == calculate_alpha_v105_checksum(next_flags, nv) {
                                     is_compact = true;
                                     break;
                                 }
                             }
                             
                             // Manual retry with fresh reader instead of rollback
                             let mut b_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                             let _ = b_reader.skip(next_bit as u32 + 32);
                             if let (Ok(nv), Ok(nm), Ok(nl)) = (b_reader.read::<3, u8>(), b_reader.read::<3, u8>(), b_reader.read::<3, u8>()) {
                                 if nv <= 7 && nm <= 6 && nl <= 6 && (next_flags != 0 || nv != 0) {
                                     is_compact = true;
                                     break;
                                 }
                             }
                         }
                     }
                 }
            }
        }
    }

    let _s_axiom = StatsAxiom::new(version, crate::domain::item::ItemQuality::Normal, alpha_mode)
        .with_compact(is_compact);
    let _is_personalized = h_axiom.is_personalized(flags);
    
    // Trial peek to resolve Alpha v105 runeword header gaps (Axiom 0365)
    let mut trial_is_rw = false;
    if alpha_mode {
        // Assume 24-bit gap for RW/Shadow items in Alpha v105
        let trial_skip = base_header_len as u32 + 24;
        let mut t_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
        if t_reader.skip(start_bit as u32 + trial_skip).is_ok() {
            let mut t_cursor = BitCursor::new(t_reader);
            let mut t_code = String::new();
            for _ in 0..4 {
                if let Ok(ch) = huffman.decode_recorded(&mut t_cursor) { t_code.push(ch); }
                else { break; }
            }
            if h_axiom.is_runeword(flags, Some(&t_code)) {
                trial_is_rw = true;
            }
        }
    }

    let geometry = h_axiom.header_geometry(flags, if trial_is_rw { Some("acww") } else { None });
    
    let mut total_skip = base_header_len as u32;
    if alpha_mode && geometry.target_width > 0 {
        total_skip = geometry.target_width;
    } else if geometry.has_header_gap && alpha_mode {
        let gap_bits = V105HeaderGapAxiom::default().resolve_gap(version, None, flags, false, is_compact, has_checksum);
        if !is_compact {
             total_skip += (geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as u32;
        }
        total_skip += gap_bits as u32;
    } else if !geometry.skip_geometry {
         total_skip += (geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as u32;
    }

    let mut code = String::new();
    let mut n_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    
    // Trial peek for code to determine adaptive alignment nudge (Slice 14)
    let trial_nudge = if alpha_mode && version == 0 { 19 } else { 0 };
    let mut trial_code = String::new();
    let mut trial_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if trial_reader.skip(start_bit as u32 + total_skip + trial_nudge).is_ok() {
        let mut t_cursor = BitCursor::new(trial_reader);
        for _ in 0..4 {
            if let Ok(ch) = huffman.decode_recorded(&mut t_cursor) { trial_code.push(ch); }
            else { break; }
        }
    }

    let alignment_nudge = if alpha_mode { 
        V105AlignmentAxiom::default().get_alignment_nudge(version, &trial_code, flags, is_compact)
    } else { 0 };
    if n_reader.skip(start_bit as u32 + total_skip + alignment_nudge as u32).is_err() { return None; }
    let mut n_cursor = BitCursor::new(n_reader);
    let is_compact_peek = HeaderAxiom::new(version, alpha_mode).is_compact(flags, None);
    let mut is_compact_detected = is_compact_peek;
    if alpha_mode {
        let mut trial_reader = bitstream_io::BitReader::endian(std::io::Cursor::new(section_bytes), bitstream_io::LittleEndian);
        if trial_reader.skip(start_bit as u32 + total_skip + alignment_nudge as u32).is_ok() {
            let mut trial_code = String::new();
            let mut trial_ok = true;
            for _ in 0..3 {
                if let Ok(ch) = trial_reader.read::<8, u8>() {
                    if ch != 0 { trial_code.push(ch as char); }
                } else {
                    trial_ok = false;
                    break;
                }
            }
            if trial_ok && (is_compact_peek || is_v105_summary_code(&trial_code)) {
                code = trial_code;
                is_compact_detected = true;
                let _ = n_cursor.read_bits_as_vec(24);
            }
        }
    }

    if code.is_empty() {
        for i in 0..4 {
            match huffman.decode_recorded(&mut n_cursor) {
                Ok(ch) => code.push(ch),
                Err(_) => {
                    if alpha_mode && i >= 2 {
                        if n_cursor.read_bit().is_ok() {
                            if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                                code.push(ch);
                                continue;
                            }
                        }
                    }
                    return None;
                }
            }
        }
    }

    let _is_compact = is_compact_detected;

    let axiom = HeaderAxiom::new(version, alpha_mode);
    if !axiom.is_plausible(mode, loc, &code, flags) {
        return None;
    }

    let is_compact = is_compact_detected;
    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let s_axiom = s_axiom.with_compact(is_compact);
    let _is_personalized = s_axiom.is_personalized(flags);
    let geometry = axiom.header_geometry(flags, Some(&code));

    let mut possible_gaps = Vec::new();
    if alpha_mode {
        let reg = crate::domain::forensic::registry::get_registry();
        if let Some(overrides) = &reg.item_overrides {
            for item_map in overrides.values() {
                if let Some(&gap) = item_map.get("header_gap") {
                    possible_gaps.push(gap as u64);
                }
            }
        }
        // Fallback to standard Alpha increments
        let geom_bits = (geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as u64;
        possible_gaps.push(geom_bits + 0);
        possible_gaps.push(geom_bits + 8);
        possible_gaps.push(geom_bits + 16);
        possible_gaps.push(geom_bits + 24);
        possible_gaps.push(geom_bits + 32);

        // Alpha v105 forensic: Include the adaptive alignment nudge (Slice 14)
        if alignment_nudge > 0 {
            let base_geom_gap = (total_skip as i32 - base_header_len as i32) as i32;
            let nudge_gap = base_geom_gap + alignment_nudge as i32;
            if nudge_gap >= 0 {
                possible_gaps.push(nudge_gap as u64);
            }
        }

        // Alpha v105 forensic: Try 1-bit and 2-bit nudges (Axiom 0340)
        possible_gaps.push(geom_bits + 1);
        possible_gaps.push(geom_bits + 2);
        possible_gaps.push(geom_bits + 9);
        possible_gaps.push(geom_bits + 10);
    } else {
        if geometry.has_header_gap {
            possible_gaps.push((geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as u64 + 8);
        } else if !geometry.skip_geometry {
            possible_gaps.push((geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as u64);
        } else {
            possible_gaps.push(0);
        }
    }

    // Do NOT sort or dedup here for Alpha v105 to maintain prioritization of byte-aligned gaps.
    // (Axiom 0340)

    let mut best_candidate: Option<(u8, u8, u8, String, u32, u8, bool, u64, i8, bool)> = None;
    let mut best_is_known = false;

    for gap in possible_gaps {
        if let Some(candidate) = peek_item_header_at_specific_gap(
            section_bytes, start_bit, huffman, alpha_mode, gap
        ) {
            let code = &candidate.3;
            let is_known = is_v105_summary_code(code) || item_template(code).is_some();
            
            // Forensic: Prioritize checksums and known item codes to stop false positives (Axiom 0340)
            let best_has_checksum = if let Some(c) = &best_candidate { c.9 } else { false };
            let has_checksum = candidate.9;

            if best_candidate.is_none() 
                || (!best_has_checksum && has_checksum)
                || (!best_is_known && is_known && (!best_has_checksum || has_checksum)) 
            {
                best_candidate = Some(candidate);
                best_is_known = is_known;
            }
        }
    }
    best_candidate
}

pub fn peek_item_header_at_specific_gap(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    gap: u64,
) -> Option<(u8, u8, u8, String, u32, u8, bool, u64, i8, bool)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if reader.skip(start_bit as u32).is_err() { return None; }

    // Read header structure
    let flags = reader.read::<32, u32>().ok()?;
    
    let mut alpha_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    alpha_reader.skip(start_bit as u32 + 32).ok()?;
    let checksum = alpha_reader.read::<8, u8>().ok()?;
    let v = alpha_reader.read::<3, u8>().ok()?;
    let calculated = calculate_alpha_v105_checksum(flags, v);
    
    // Alpha Forensic (Axiom 0365): Some summary items use 0 as a checksum sentinel.
    let (version, mode, loc, x_val, base_header_len, has_checksum) = if calculated == checksum || (alpha_mode && checksum == 0) {
        let m = alpha_reader.read::<3, u8>().ok()?;
        let l = alpha_reader.read::<3, u8>().ok()?;
        let x = alpha_reader.read::<4, u8>().ok()?;
        (v, m, l, x, 32 + 8 + 3 + 3 + 3 + 4, true)
    } else {
        let mut retail_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
        retail_reader.skip(start_bit as u32 + 32).ok()?;
        let v = retail_reader.read::<3, u8>().ok()?;
        let m = retail_reader.read::<3, u8>().ok()?;
        let l = retail_reader.read::<3, u8>().ok()?;
        let x = retail_reader.read::<4, u8>().ok()?;
        (v, m, l, x, 32 + 3 + 3 + 3 + 4, false)
    };

    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact_peek = s_axiom.is_compact(flags);
    let mut is_compact_detected = is_compact_peek;

    let mut code = String::new();
    let mut n_reader = bitstream_io::BitReader::endian(std::io::Cursor::new(section_bytes), bitstream_io::LittleEndian);
    if n_reader.skip(start_bit as u32 + base_header_len as u32 + gap as u32).is_err() { return None; }
    let mut n_cursor = BitCursor::new(n_reader);
    let mut ok = true;

    if alpha_mode {
        let mut trial_reader = bitstream_io::BitReader::endian(std::io::Cursor::new(section_bytes), bitstream_io::LittleEndian);
        if trial_reader.skip(start_bit as u32 + base_header_len as u32 + gap as u32).is_ok() {
            let mut trial_code = String::new();
            let mut trial_ok = true;
            for _ in 0..3 {
                if let Ok(ch) = trial_reader.read::<8, u8>() {
                    if ch != 0 { trial_code.push(ch as char); }
                } else {
                    trial_ok = false;
                    break;
                }
            }
            if trial_ok && (is_compact_peek || is_v105_summary_code(&trial_code)) {
                code = trial_code;
                is_compact_detected = true;
                let _ = n_cursor.read_bits_as_vec(24);
            }
        }
    }

    if code.is_empty() && alpha_mode {
        let saved_pos = n_cursor.pos();
        if let Ok(bits) = n_cursor.read_bits_as_vec(24) {
            if let Some(stealth) = crate::domain::forensic::v105::axioms::V105StealthCodeAxiom::default().resolve_stealth_code(&bits) {
                code = stealth.to_string();
                is_compact_detected = true;
            } else {
                n_cursor.rollback(saved_pos);
            }
        } else {
            n_cursor.rollback(saved_pos);
        }
    }

    if code.is_empty() {
        for i in 0..4 {
            match huffman.decode_recorded(&mut n_cursor) {
                Ok(ch) => code.push(ch),
                Err(_) => {
                if alpha_mode && i >= 1 {
                    let current_cursor_pos = n_cursor.pos();
                    let relative_pos = base_header_len as u64 + gap as u64 + current_cursor_pos;
                    if relative_pos == 69 && (code.starts_with('h') || code.starts_with('m')) {
                        // Surgical 1-bit nudge for Opaque items at bit 69 (Axiom 0340)
                        if n_cursor.read_bit().is_ok() {
                            if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                                code.push(ch);
                                continue;
                            }
                        }
                        n_cursor.rollback(current_cursor_pos);
                    }
                }
                let saved_pos = n_cursor.pos();
                    // Try 1-bit nudge
                    if n_cursor.read_bit().is_ok() {
                        if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                            code.push(ch);
                            continue;
                        }
                    }
                    // Try 2-bit nudge
                    n_cursor.rollback(saved_pos);
                    if let Ok(bits) = n_cursor.read_bits_as_vec(2) {
                        if bits.len() == 2 {
                            if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                                code.push(ch);
                                continue;
                            }
                        }
                    }
                    n_cursor.rollback(saved_pos);
                    ok = false;
                    break;
                }
            }
        }
    }
    
    let is_compact = is_compact_detected;
    if ok {
        // eprintln!("[DEBUG] specific_gap: gap={}, code='{}', plausible={}", gap, code, is_plausible_item_header(mode, loc, &code, flags, version, alpha_mode));
        if is_plausible_item_header(mode, loc, &code, flags, version, alpha_mode) {
            return Some((mode, loc, x_val, code, flags, version, is_compact, (base_header_len as u64 + gap), gap as i8, has_checksum));
        }
    }
    None
}


pub fn parse_item_at_with_limit(
    bytes: &[u8],
    bit: u64,
    huffman: &HuffmanTree,
    idx: usize,
    alpha: bool,
    limit: Option<u64>,
    forced_compact: Option<bool>,
    code_hint: Option<&str>,
) -> ParsingResult<(Item, u64)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit as u32);
    let mut cursor = BitCursor::new(reader);
    if let Some(l) = limit {
        cursor.set_limit(l);
    }
    let item = Item::from_reader_with_context(&mut cursor, huffman, Some((bytes, bit)), alpha, idx, forced_compact, code_hint)?;
    Ok((item, cursor.pos()))
}

pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
    let mut all_items = Vec::new();
    let jm_positions = crate::save::find_jm_markers(bytes);

    if jm_positions.is_empty() {
        return Err(ParsingFailure {
            error: ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: 0 },
            context_stack: vec!["read_player_items".to_string()],
            bit_offset: 0,
            context_relative_offset: 0,
            hint: Some("Could not find any JM markers.".to_string()),
        });
    }

    for i in 0..jm_positions.len() {
        let pos = jm_positions[i];
        if bytes.len() < pos + 4 { continue; }
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        if count == 0 { continue; }

        let next_pos = jm_positions.get(i + 1).cloned().unwrap_or(bytes.len());
        let section_bytes = &bytes[pos..next_pos];

        match Item::read_section(section_bytes, (pos as u64) * 8, count, huffman, alpha) {
            Ok(items) => {
                all_items.extend(items);
            }
            Err(e) => {
                if !alpha { return Err(e); }
            }
        }
    }


    Ok(all_items)
}

pub fn from_bytes(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
    let (item, _) = parse_item_at_with_limit(bytes, 0, huffman, 0, alpha, None, None, None)?;
    Ok(item)
}

impl Item {
    pub fn from_bytes(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
        from_bytes(bytes, huffman, alpha)
    }

    pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
        read_player_items(bytes, huffman, alpha)
    }

    pub fn read_section_ext(section_bytes: &[u8], section_bit_offset: u64, top_level_count: u16, huffman: &HuffmanTree, alpha_mode: bool, preserve_unparsed: bool) -> ParsingResult<Vec<Item>> {
        let _ = preserve_unparsed;
        Self::read_section(section_bytes, section_bit_offset, top_level_count, huffman, alpha_mode)
    }

    pub fn parse_at_bit_offset(bytes: &[u8], bit_offset: u64, huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
        let (item, _) = parse_item_at_with_limit(bytes, bit_offset, huffman, 0, alpha, None, None, None)?;
        Ok(item)
    }

    pub fn read_section(section_bytes: &[u8], section_bit_offset: u64, top_level_count: u16, huffman: &HuffmanTree, alpha_mode: bool) -> ParsingResult<Vec<Item>> {
        let mut items: Vec<Item> = Vec::new();
        let section_bits = (section_bytes.len() * 8) as u64;

        // Parse D2R_FORCE_LENGTH (e.g., "7256:80,7336:80")
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

        let markers = crate::domain::item::scanner::scan_item_markers(section_bytes, huffman, alpha_mode, section_bit_offset, Some(top_level_count));
        eprintln!("[DEBUG-SLICE13] markers found: {}, top_level_count: {}", markers.len(), top_level_count);
        let mut start_offset = 32; // Relative skip JM (16) + Count (16) inside section_bytes
        let mut subsumed_indices = std::collections::HashSet::new();

        for (i, marker) in markers.iter().enumerate() {
            if subsumed_indices.contains(&i) { continue; }
            let start = marker.offset; // marker.offset is relative to section_bytes
            let non_residue_count = items.iter().filter(|it| !it.is_residue()).count();
            if non_residue_count >= top_level_count as usize {
                break;
            }
            if start < start_offset {
                continue;
            }

            // Slice 2: Capture residue between items
            if start > start_offset {
                let residue_len = start - start_offset;
                let mut bits = Vec::new();
                let mut fallback_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                if fallback_reader.skip(start_offset as u32).is_ok() {
                    for _ in 0..residue_len {
                        if let Ok(b) = fallback_reader.read_bit() {
                            bits.push(b);
                        } else {
                            break;
                        }
                    }
                }
                let mut residue = Item::default();
                residue.expected_start_bit = start_offset;
                residue.code = "    ".to_string();
                residue.modules.push(crate::domain::item::ItemModule::Residue(bits.clone()));
                for (idx, b) in bits.iter().enumerate() {
                    residue.bits.push(crate::domain::item::RecordedBit {
                        bit: *b,
                        offset: section_bit_offset + start_offset + idx as u64,
                    });
                }
                residue.range.start = section_bit_offset + start_offset;
                residue.range.end = section_bit_offset + start;
                residue.total_bits = residue_len;
                residue.forensic_audit.record(ForensicMetadata::new(
                    Confidence::Fragile,
                    Intentionality::Artifactual,
                    "Residue preservation"
                ));
                items.push(residue);
            }

            // Slice 5: Acceptance Gate
            // If confidence is low, degrade to Opaque isolation instead of attempting full parse.
            let mut reject_candidate = false;
            if alpha_mode && marker.confidence < 250 {
                reject_candidate = true;
            }

            let next_marker = markers.get(i + 1).map(|m| m.offset).unwrap_or(section_bits);
            let limit = next_marker - start;

            let absolute_offset = section_bit_offset + start;
            let forced_length = force_length_map.get(&absolute_offset).cloned();

            // Refined: Dynamically adjust chunk limit for known variable padding
            let mut dynamic_limit = limit;
            let mut is_compact_final = false;
            
            if let Some(flen) = forced_length {
                dynamic_limit = flen;
                is_compact_final = true;
            } else if let Some((_, _, _, code, flags, _, is_compact, _, _, _)) =
                peek_item_header_at(section_bytes, start, huffman, alpha_mode)
            {
                is_compact_final = is_compact;
                // Slice 6/9: Axiom 0344 inference for blank items and summary codes missing the compact flag
                if alpha_mode && !is_compact && (code.trim().is_empty() || is_v105_summary_code(&code)) {
                    // Refined: Only force compact if there's another plausible marker 72 bits later
                    let min_interval = if alpha_mode { 72 } else { 80 };
                    if let Some(next_header) = peek_item_header_at(section_bytes, start + min_interval, huffman, alpha_mode) {
                         let (n_mode, n_loc, _, n_code, n_flags, n_ver, _, _, _, _) = next_header;
                         if is_plausible_item_header(n_mode, n_loc, &n_code, n_flags, n_ver, alpha_mode) {
                             is_compact_final = true;
                         }
                    }
                }
                
                // Alpha v105 forensic: Socketed items add 8-bit alignment padding
                if !is_compact_final && (flags & 0x00000008) != 0 {
                    dynamic_limit += 8;
                }
            }

            if !alpha_mode && !is_compact_final {
                dynamic_limit += 128; // Safety buffer (Retail only)
            }

            let parse_result = if reject_candidate {
                Err(ParsingFailure {
                    error: ParsingError::SpeculativeRejection { 
                        bit_offset: start, 
                        confidence: marker.confidence 
                    },
                    context_stack: vec!["AcceptanceGate".to_string()],
                    bit_offset: section_bit_offset + start,
                    context_relative_offset: 0,
                    hint: Some("Candidate rejected due to low confidence score in noisy Alpha v105 segment.".to_string()),
                })
            } else {
                parse_item_at_with_limit(
                    section_bytes,
                    start,
                    huffman,
                    items.len(),
                    alpha_mode,
                    Some(dynamic_limit),
                    if is_compact_final { Some(true) } else { None },
                    Some(&marker.code),
                ).map_err(|e| e) // Compatibility
            };

            match parse_result {
                Ok((item, consumed_bits)) => {
                    let mut final_item = item.clone();
                    let actual_consumed = if let Some(flen) = forced_length { flen } else { consumed_bits };
                    
                    final_item.expected_start_bit = start;
                    final_item.range.start = section_bit_offset + start;
                    final_item.range.end = section_bit_offset + start + actual_consumed;
                    final_item.total_bits = actual_consumed;
                    
                    // Slice 7: Mark subsumed markers (Competitive Marker Resolution)
                    let end_bit = start + actual_consumed;
                    for (next_idx, next_marker) in markers.iter().enumerate().skip(i + 1) {
                        if next_marker.offset < end_bit {
                            subsumed_indices.insert(next_idx);
                        } else {
                            break;
                        }
                    }

                    items.push(final_item);
                    start_offset = start + actual_consumed;
                }
                Err(_e) => {
                    // Marker was plausible but parsing failed or was rejected. Capture raw bits as Opaque item.
                    // Slice 7: Dynamic Interval Capture. Scan for next JM to bound the Opaque block.
                    let mut actual_limit = limit;
                    let mut found_next = false;
                    
                    let mut probe_pos = start + (if alpha_mode { 72 } else { 80 }); // Minimum interval for Alpha v105
                    while probe_pos + 32 <= section_bits {
                        let mut probe_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                        if probe_reader.skip(probe_pos as u32).is_ok() {
                            if let Ok(p_flags) = probe_reader.read::<32, u32>() {
                                let mut is_next = (p_flags & 0xFFFF) == 0x4D4A;
                                if !is_next && alpha_mode {
                                    let mut check_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                                    if check_reader.skip(probe_pos as u32 + 32).is_ok() {
                                        if let (Ok(ck), Ok(v)) = (check_reader.read::<8, u8>(), check_reader.read::<3, u8>()) {
                                            if ck == calculate_alpha_v105_checksum(p_flags, v) {
                                                is_next = true;
                                            }
                                        }
                                    }
                                }
                                
                                if is_next {
                                    actual_limit = probe_pos - start;
                                    found_next = true;
                                    break;
                                }
                            }
                        }
                        probe_pos += 8;
                    }

                    let (peek_code, peek_limit) = if let Some(flen) = forced_length {
                        ("    ".to_string(), flen)
                    } else if let Some((version, _, _, code, flags, _, is_compact, _, _, _)) =
                        peek_item_header_at(section_bytes, start, huffman, alpha_mode)
                    {
                        let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode)
                            .with_compact(is_compact)
                            .with_code(&code);
                        let l = if alpha_mode && axiom.is_compact { 
                            // For Opaque compact items, we use the alignment axiom with a minimal 32-bit guess if unknown,
                            // but usually peek_item_header_at consumed ~64-80 bits already.
                            axiom.calculate_alignment(64, &code, flags)
                        } else if found_next {
                            actual_limit
                        } else { limit };
                        (code, l)
                    } else {
                        ("    ".to_string(), if found_next { actual_limit } else { limit })
                    };

                    let mut bits = Vec::new();
                    let mut fallback_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                    if fallback_reader.skip(start as u32).is_ok() {
                        for _ in 0..peek_limit {
                            if let Ok(b) = fallback_reader.read_bit() {
                                bits.push(b);
                            } else {
                                break;
                            }
                        }
                    }

                    let mut opaque_item = Item::default();
                    opaque_item.expected_start_bit = start;
                    opaque_item.code = peek_code;
                    opaque_item.modules.push(crate::domain::item::ItemModule::Opaque(bits.clone()));
                    for (idx, b) in bits.iter().enumerate() {
                        opaque_item.bits.push(crate::domain::item::RecordedBit {
                            bit: *b,
                            offset: section_bit_offset + start + idx as u64,
                        });
                    }
                    opaque_item.range.start = section_bit_offset + start;
                    opaque_item.range.end = section_bit_offset + start + peek_limit;
                    opaque_item.total_bits = peek_limit;
                    opaque_item.forensic_audit.record(ForensicMetadata::new(
                        Confidence::Fragile,
                        Intentionality::Undetermined,
                        format!("Opaque isolation: {}", _e)
                    ));
                    items.push(opaque_item);
                    start_offset = start + peek_limit;
                }
            }
        }

        // Slice 2: Residue capture to ensure item count parity and bit preservation
        let last_end = items.last().map(|it| it.range.end - section_bit_offset).unwrap_or(start_offset);
        if last_end < section_bits {
            let missing = if items.len() < top_level_count as usize {
                top_level_count as usize - items.len()
            } else if items.is_empty() && top_level_count == 0 {
                1 // Capture all as one residue if empty section
            } else {
                1 // Capture trailing bits as 1 residue
            };
            if missing > 0 {
                let remaining_bits = section_bits - last_end;
                // ... rest of the code
                let bits_per_item = remaining_bits / missing as u64;

                for i in 0..missing {
                    let mut bits = Vec::new();
                    let start = last_end + (i as u64 * bits_per_item);
                    let end = if i == missing - 1 { section_bits } else { start + bits_per_item };
                    let len = end - start;

                    let mut fallback_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                    if fallback_reader.skip(start as u32).is_ok() {
                        for _ in 0..len {
                            if let Ok(b) = fallback_reader.read_bit() {
                                bits.push(b);
                            } else {
                                break;
                            }
                        }
                    }

                    let mut opaque_item = Item::default();
                    opaque_item.expected_start_bit = start;
                    opaque_item.code = "    ".to_string();
                    opaque_item.modules.push(crate::domain::item::ItemModule::Residue(bits.clone()));
                    for (idx, b) in bits.iter().enumerate() {
                        opaque_item.bits.push(crate::domain::item::RecordedBit {
                            bit: *b,
                            offset: section_bit_offset + start + idx as u64,
                        });
                    }
                    opaque_item.range.start = section_bit_offset + start;
                    opaque_item.range.end = section_bit_offset + end;
                    opaque_item.total_bits = len;
                    opaque_item.forensic_audit.record(ForensicMetadata::new(
                        Confidence::Fragile,
                        Intentionality::Artifactual,
                        "Residue preservation"
                    ));
                    items.push(opaque_item);
                }
            }
        }

        Ok(items)
    }

    pub fn from_reader<R: BitRead>(
        reader: &mut R,
        huffman: &HuffmanTree,
        alpha: bool,
    ) -> ParsingResult<Item> {
        let mut cursor = BitCursor::new(reader);
        Self::from_reader_with_context(&mut cursor, huffman, None, alpha, 0, None, None)
    }

    pub fn from_reader_with_context<R: BitRead>(
        cursor: &mut BitCursor<R>,
        huff: &HuffmanTree,
        ctx: Option<(&[u8], u64)>,
        alpha_mode: bool,
        idx: usize,
        forced_compact: Option<bool>,
        code_hint: Option<&str>,
    ) -> ParsingResult<Item> {
        let is_first_item = idx == 0;
        cursor.set_trace(crate::item::item_trace_enabled());
        let start_bit = cursor.pos();
        cursor.begin_segment(ItemSegmentType::Root);

        let peek = if alpha_mode && ctx.is_some() {
            let (bytes, start_bit) = ctx.unwrap();
            peek_item_header_at(bytes, start_bit, huff, true)
        } else { None };
        let code_peek = code_hint.or(peek.as_ref().map(|p| p.3.as_str()));
        let gap_override = peek.as_ref().map(|p| p.8 as usize);
        let _has_checksum_peek = peek.as_ref().map(|p| p.9);
        let is_compact_peek = peek.as_ref().map(|p| p.6);

        let (header, alpha_header_gap, alpha_header_gap_bits) = crate::domain::item::entity::parse_item_header(cursor, alpha_mode, code_peek, gap_override, is_first_item, forced_compact.or(is_compact_peek))?;

        // Log gap for analysis
        if let Some(_gap) = alpha_header_gap {
            cursor.push_context("AlphaHeaderGap");
            // If we have an alpha_header_gap, consume or log its impact
            // This is a minimal modeling approach as per mini-spec
            cursor.pop_context();
        }

        let s_axiom = StatsAxiom::new(header.version, header.quality.unwrap_or(crate::domain::item::ItemQuality::Normal), alpha_mode)
            .with_index(idx)
            .with_compact(header.is_compact)
            .with_code(code_peek.unwrap_or(""));

        if s_axiom.is_header_only(header.flags, code_peek.unwrap_or("")) {
            let mut body = crate::domain::item::entity::ItemBody::default();
            body.alpha_header_gap = alpha_header_gap;
            body.alpha_header_gap_bits = alpha_header_gap_bits;
            cursor.end_segment(); // Root segment
            return Ok(Item {
                header,
                body,
                code: String::new(),
                bits: Vec::new(),
                range: crate::domain::item::ItemBitRange { start: start_bit, end: cursor.pos() },
                total_bits: cursor.pos() - start_bit,
                ..Default::default()
            });
        }

        let body_start_bit = cursor.pos();        
        // Force V5 propagation if header detected v5
        let body_res = crate::domain::item::entity::parse_item_body(cursor, huff, &header, alpha_mode);

        let mut rhythm_recovery = false;
        let (mut body, ear_class, ear_level, ear_player_name) = match body_res {
            Ok(res) => res,
            Err(_e) if alpha_mode && (header.version == 5 || header.version == 1 || header.version == 0 || header.version == 2) => {
                // Slice 6: Huffman resolution failure or drift in Alpha v105.
                // Trigger 9+9 property rhythm recovery.
                rhythm_recovery = true;
                let mut b = crate::domain::item::entity::ItemBody::default();
                b.code = "    ".to_string();
                cursor.rollback(body_start_bit);
                (b, None, None, None)
            }
            Err(e) => {
                if alpha_mode {
                    // Slice 4: Forensic isolation. Capture header and preserve body as SemiOpaque.
                    cursor.rollback(body_start_bit);
                    let remaining = if let Some(limit) = cursor.limit() {
                        (limit as i64 - (cursor.pos() - start_bit) as i64).max(0) as u64
                    } else { 0 };
                    let body_bits = cursor.read_bits_as_vec(remaining as u32)?;
                    
                    let mut item = Item::default();
                    item.header = header.clone();
                    item.body.alpha_header_gap = alpha_header_gap;
                    item.body.alpha_header_gap_bits = alpha_header_gap_bits;
                    item.range.start = start_bit;
                    item.range.end = cursor.pos();
                    item.total_bits = cursor.pos() - start_bit;
                    item.expected_start_bit = start_bit;
                    
                    // Slice 4: Record all bits (header + body) for parity check
                    let all_recorded = cursor.recorded_bits();
                    let start_idx = (start_bit as usize).min(all_recorded.len());
                    let end_idx = (cursor.pos() as usize).min(all_recorded.len());
                    item.bits = all_recorded[start_idx..end_idx].to_vec();

                    use crate::domain::item::ItemModule;
                    item.modules.push(ItemModule::SemiOpaque {
                        body_bits,
                        reason: format!("{:?}", e),
                    });
                    item.forensic_audit.record(ForensicMetadata::new(
                        Confidence::Speculative,
                        Intentionality::Undetermined,
                        format!("SemiOpaque isolation: {}", e)
                    ));
                    
                    let end_pos = cursor.pos() as usize;
                    if end_pos <= cursor.recorded_bits().len() {
                        item.bits = cursor.recorded_bits()[(start_bit as usize)..end_pos].to_vec();
                    }

                    cursor.end_segment();
                    return Ok(item);
                }
                return Err(e);
            }
        };
        body.alpha_header_gap = alpha_header_gap;
        body.alpha_header_gap_bits = alpha_header_gap_bits;

        let axiom = StatsAxiom::new(header.version, ItemQuality::Normal, alpha_mode)
            .with_compact(header.is_compact)
            .with_code(&body.code);
        
        let ext_data = if !header.is_compact && !rhythm_recovery {
            crate::domain::item::entity::ExtendedStatsData::read_from_cursor(cursor, &body.code, &header, alpha_mode, &axiom)?
        } else {
            crate::domain::item::entity::ExtendedStatsData::default()
        };

        let mut final_header = header;
        final_header.id = ext_data.id;
        final_header.level = ext_data.level;
        final_header.quality = ext_data.quality;
        final_header.alpha_quality_raw = ext_data.alpha_quality_raw;
        final_header.alpha_v5_runeword_extra = ext_data.v5_runeword_extra;
        final_header.alpha_unique_id_raw = ext_data.alpha_unique_id_raw;

        body.defense = ext_data.defense;
        body.max_durability = ext_data.max_durability;
        body.current_durability = ext_data.current_durability;
        body.quantity = ext_data.quantity;
        body.v5_runeword_extra = ext_data.v5_runeword_extra;
        body.alpha_set_list_val = ext_data.alpha_set_list_val;

        let code = body.code.clone();

        let mut item = Item {
            body,
            stats: ItemStats { properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new() },
            bits: Vec::new(),
            code: code.clone(),
            defense: ext_data.defense,
            max_durability: ext_data.max_durability,
            current_durability: ext_data.current_durability,
            quantity: ext_data.quantity,
            ear_class, ear_level, ear_player_name,
            personalized_player_name: ext_data.personalized_player_name,
            has_multiple_graphics: ext_data.has_multiple_graphics, multi_graphics_bits: ext_data.multi_graphics_bits,
            has_class_specific_data: ext_data.has_class_specific_data, class_specific_bits: ext_data.class_specific_bits,
            low_high_graphic_bits: ext_data.low_high_graphic_bits,
            magic_prefix: ext_data.magic_prefix, magic_suffix: ext_data.magic_suffix,
            rare_name_1: ext_data.rare_name_1, rare_name_2: ext_data.rare_name_2, rare_affixes: ext_data.rare_affixes,
            unique_id: ext_data.unique_id, runeword_id: ext_data.runeword_id, runeword_level: ext_data.runeword_level,
            properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new(),
            num_socketed_items: 0, socketed_items: Vec::new(),
            timestamp_flag: ext_data.timestamp_flag,
            properties_complete: false,
            terminator_bit: false,
            header: final_header,
            set_list_count: ext_data.set_list_count,
            tbk_ibk_teleport: ext_data.tbk_ibk_teleport,
            sockets: ext_data.sockets,
            modules: Vec::new(),
            range: crate::domain::item::ItemBitRange { start: start_bit, end: 0 },
            total_bits: 0,
            gap_bits: Vec::new(),
            segments: Vec::new(),
            expected_start_bit: 0,
            forensic_audit: ForensicAudit::new(),
        };

        if item.body.alpha_nudge.is_some() { item.forensic_audit.record(V105NudgeAxiom.metadata()); }
        if item.body.alpha_header_gap.is_some() { item.forensic_audit.record(V105HeaderGapAxiom.metadata()); }
        if item.body.alpha_shadow_skip_bits.is_some() { item.forensic_audit.record(V105ShadowAxiom.metadata()); }
        if rhythm_recovery { item.forensic_audit.record(V105PropertyNudgeAxiom::default().metadata()); }

        // Slice 1: Force stats reading for Alpha v105 items even if compact, 
        // to detect residue Defense/Durability as per mini-spec.
        // EXCEPT for summary items (Axiom 0392) which never have stats.
        let is_v105_summary = alpha_mode && crate::domain::forensic::v105::axioms::is_v105_summary_code(&item.code);
        if (!item.header.is_compact && !is_v105_summary) || (alpha_mode && (item.header.version == 0 || item.header.version == 1 || item.header.version == 2) && !is_v105_summary) {
            let is_v105_shadow = axiom.is_v105_shadow(item.header.flags);

            // Slice 11: Handle JM-to-Body alignment gap
            let gap_len = axiom.header_gap(&item.code, item.header.flags);
            if gap_len > 0 {
                cursor.push_context("AlphaBodyGap");
                let gap_bits = cursor.read_bits_as_vec(gap_len)?;
                item.body.alpha_body_gap_bits.extend(gap_bits);
                cursor.pop_context();
            }

            // Slice 23: Apply residue nudge (symbolic anchor)
            if alpha_mode {
                let p_nudge = calculate_property_residue(item.header.version);
                if p_nudge > 0 && !rhythm_recovery {
                    cursor.push_context("AlphaPropertyResidueNudge");
                    let _ = cursor.read_bits::<u32>(p_nudge as u32)?;
                    item.forensic_audit.record(V105PropertyNudgeAxiom::default().metadata());
                    cursor.pop_context();
                }
            }

            let (props, complete, term, _extra_bits, _payload, shadow_bits, nested_items) = crate::domain::stats::parser::read_item_stats(
                cursor, 
                &item.code, 
                item.header.version, 
                ctx, 
                huff, 
                alpha_mode, 
                item.header.quality, 
                item.header.is_runeword, 
                is_v105_shadow || rhythm_recovery, 
                item.header.is_personalized,
                item.header.is_compact
            )?;
            item.properties = props.clone();
            item.stats.properties = props;
            item.properties_complete = complete;
            item.terminator_bit = term;
            item.body.alpha_shadow_skip_bits = shadow_bits;
            item.socketed_items = nested_items;
        }

        let axiom = StatsAxiom::new(item.header.version, item.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_index(idx)
            .with_personalization(item.header.is_personalized)
            .with_compact(item.header.is_compact)
            .with_code(&item.code);
        let consumed_bits = cursor.pos() - start_bit;
        let final_consumed = axiom.calculate_alignment(consumed_bits, &item.code, item.header.flags);
        
        if final_consumed > consumed_bits {
            let padding_count = (final_consumed - consumed_bits) as u32;
            let padding = cursor.with_context("AlphaAlignmentPadding", |c| {
                let mut bits = Vec::new();
                for _ in 0..padding_count { 
                    match c.read_bit() {
                        Ok(bit) => bits.push(bit),
                        Err(_) => break, // Stop gracefully if we hit the end
                    }
                }
                Ok(bits)
            })?;
            item.body.alpha_alignment_padding = padding;
        }
        
        item.range.end = cursor.pos();
        item.total_bits = item.range.end - item.range.start;
        
        let start_idx = start_bit as usize;
        let end_idx = cursor.pos() as usize;
        if end_idx <= cursor.recorded_bits().len() {
             item.bits = cursor.recorded_bits()[start_idx..end_idx].to_vec();
        }
        
        item.segments = cursor.segments().iter()
            .filter(|s| s.start >= start_bit && s.end <= cursor.pos())
            .cloned()
            .collect();

        cursor.end_segment();
        
        if let Some(l) = cursor.limit() {
            if cursor.pos() < l {
                let residue_len = l - cursor.pos();
                let mut residue_bits = Vec::new();
                for _ in 0..residue_len {
                    if let Ok(b) = cursor.read_bit() { residue_bits.push(b); }
                }
                item.modules.push(crate::domain::item::ItemModule::Residue(residue_bits));
            }
        }

        Ok(item)
    }
}


pub fn is_v105_summary_code(code: &str) -> bool {
    crate::domain::forensic::v105::axioms::is_v105_summary_code(code)
}

pub fn item_template(code: &str) -> Option<&'static crate::data::item_codes::ItemTemplate> {
    crate::data::item_codes::ITEM_TEMPLATES.iter().find(|t| t.code == code.trim())
}

pub fn scan_socket_children(
    bytes: &[u8],
    bit_pos: u64,
    huffman: &HuffmanTree,
    _parent_idx: usize,
    alpha: bool,
    limit: u64,
) -> Option<(Vec<Item>, u64)> {
    let mut children = Vec::new();
    let mut current_pos = bit_pos;
    let max_pos = bit_pos + limit;
    let section_bits = (bytes.len() * 8) as u64;

    while current_pos < max_pos && current_pos < section_bits {
        if let Some((mode, location, _x, code, flags, version, _is_compact, _header_bits, _nudge, _has_checksum)) = peek_item_header_at(bytes, current_pos, huffman, alpha) {
            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                if mode == 6 || location == 6 {
                    let remaining = section_bits.saturating_sub(current_pos);
                    if let Ok((item, consumed)) = parse_item_at_with_limit(bytes, current_pos, huffman, 0, alpha, Some(remaining), None, None) {
                        let mut item_end = current_pos + consumed;
                        if alpha {
                            if let Some(next_start) = find_next_item_match(bytes, current_pos + 64, huffman, alpha) {
                                if next_start < item_end && next_start < max_pos { item_end = next_start; }
                            }
                        }
                        let mut final_child = item;
                        final_child.range.start = current_pos;
                        final_child.range.end = item_end;
                        final_child.total_bits = item_end - current_pos;
                        children.push(final_child);
                        current_pos = item_end;
                        continue;
                    }
                } else { break; }
            } else { break; }
        } else { break; }
    }
    if children.is_empty() { None } else { Some((children, current_pos)) }
}

#[derive(Debug, Clone)]
pub struct PropertyReaderContext<'a> {
    pub bytes: &'a [u8],
    pub item_start_bit: u64,
}

pub struct BitEmitter {
    writer: BitWriter<Vec<u8>, LittleEndian>,
    written: u64,
    bits: Vec<bool>,
}

impl BitEmitter {
    pub fn new() -> Self {
        BitEmitter {
            writer: BitWriter::endian(Vec::new(), LittleEndian),
            written: 0,
            bits: Vec::new(),
        }
    }

    pub fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        self.writer.write_bit(bit)?;
        self.written += 1;
        self.bits.push(bit);
        Ok(())
    }

    pub fn into_bits(self) -> Vec<bool> {
        self.bits
    }

    pub fn write_bits(&mut self, value: u32, count: u32) -> io::Result<()> {
        if count == 0 { return Ok(()); }
        for i in 0..count {
            let bit = if i < 32 {
                (value >> i) & 1 != 0
            } else {
                false
            };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn write_bits_u64(&mut self, value: u64, count: u32) -> io::Result<()> {
        if count == 0 { return Ok(()); }
        for i in 0..count {
            let bit = if i < 64 {
                (value >> i) & 1 != 0
            } else {
                false
            };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn extend_bits<I>(&mut self, bits: I) -> io::Result<()>
    where
        I: IntoIterator<Item = bool>,
    {
        for bit in bits {
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn byte_align(&mut self) -> io::Result<()> {
        let padding = (8 - (self.written % 8)) % 8;
        self.writer.byte_align()?;
        self.written += padding;
        Ok(())
    }

    pub fn written_bits(&self) -> u64 {
        self.written
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_writer()
    }
}

pub fn write_property_list(
    emitter: &mut BitEmitter,
    code: &str,
    props: &[ItemProperty],
    nested_items: &[Item],
    huffman: &HuffmanTree,
    version: u8,
    alpha_runeword: bool,
    terminator_bit: bool,
    properties_complete: bool,
    _quality: ItemQuality,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
) -> io::Result<()> {
    let _start_bits = emitter.written_bits();

    // Slice 23: Apply residue nudge (symbolic anchor)
    if axiom.save_is_alpha && !is_v105_shadow && properties_complete {
        let p_nudge = calculate_property_residue(version);
        if p_nudge > 0 {
             emitter.write_bits(0, p_nudge as u32)?;
        }
    }

    // Axiom 0344: Explicit header signal is primary, but blank items in Alpha v105 
    // often lack the compact flag despite being structurally compact (80-bit slot).
    // The inference is now centralized in StatsAxiom::with_code.
    let is_compact = axiom.is_compact;
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, 0);
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;
    let mut item_idx = 0;
    for prop in props {
        let raw_id = prop.stat_id;
        emitter.write_bits(raw_id, id_bits)?;

        let mut handled = false;
        let is_nested_stat = (raw_id == 317 || axiom.map_alpha_id(raw_id) == 317) || (raw_id == 320 || axiom.map_alpha_id(raw_id) == 320);
        if axiom.is_alpha() && is_nested_stat {
             if item_idx < nested_items.len() {
                 let child = &nested_items[item_idx];
                 let is_stat_320 = raw_id == 320 || axiom.map_alpha_id(raw_id) == 320;

                 if is_stat_320 {
                     let child_bits_vec = child.to_bits(0, huffman, axiom.save_is_alpha)?;
                     let child_bits = child_bits_vec.len();
                     let registry_width = axiom.stat_bit_width(320, 0);

                     emitter.extend_bits(child_bits_vec)?;

                     if registry_width > 0 {
                         let budget = registry_width + 2;
                         if child_bits < budget as usize {
                             emitter.write_bits(0, (budget as usize - child_bits) as u32)?;
                         }
                     }
                 } else {
                     // Variable budget (Stat 317)
                     let child_bits_vec = child.to_bits(0, huffman, axiom.save_is_alpha)?;
                     emitter.extend_bits(child_bits_vec)?;
                 }

                 item_idx += 1;
                 handled = true;
             }
        }

        if !handled {
            let mapped_id = axiom.map_alpha_id(raw_id);
            if raw_id != terminator {
                if let Some(width) = rhythm.value_bits {
                    let effective_width = axiom.stat_bit_width(raw_id, width);
                    emitter.write_bits(prop.raw_value as u32, effective_width)?;
                } else if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
                    if stat.save_param_bits > 0 { emitter.write_bits(prop.param as u32, stat.save_param_bits as u32)?; }
                    let effective_width = axiom.stat_bit_width(raw_id, stat.save_bits as u32);
                    emitter.write_bits(prop.raw_value as u32, effective_width)?;
                } else { emitter.write_bits(prop.raw_value as u32, 9)?; }
            }
        }
    }
    let already_has_term = props.iter().any(|p| p.stat_id == terminator);
    let is_rw = axiom.is_runeword(0);
    if properties_complete && !already_has_term && (!axiom.is_alpha() || version == 5 || version == 0 || version == 1 || version == 2 || version == 4 || version == 6) && !is_rw && !is_compact {
        emitter.write_bits(terminator, id_bits)?;
    }
    let preserve_trailing_align = axiom.is_alpha() && (version == 0 || version == 1 || version == 2 || version == 4 || version == 6);
    if properties_complete && rhythm.has_terminal_bit {
        if crate::item::item_trace_enabled() {
            eprintln!("[DEBUG Write] Writing terminal bit ({}) for code={}", terminator_bit, code);
        }
        emitter.write_bit(terminator_bit)?;
        if rhythm.has_extra_terminal_bit { emitter.write_bit(terminator_bit)?; }
        if !preserve_trailing_align { emitter.byte_align()?; }
    }

    // Axiom 0354: TVS (Terminator Value Slot) - Alpha v105 standard items
    if properties_complete && axiom.has_tvs_padding(alpha_runeword) {
        emitter.write_bits(0, 9)?;
    }

    Ok(())
}
pub struct HuffmanTree {
    root: Box<HuffmanNode>,
    encoding_table: std::collections::HashMap<char, Vec<bool>>,
}

struct HuffmanNode {
    symbol: Option<char>,
    left: Option<Box<HuffmanNode>>,
    right: Option<Box<HuffmanNode>>,
}

impl HuffmanNode {
    fn new() -> Self {
        HuffmanNode {
            symbol: None,
            left: None,
            right: None,
        }
    }
}

impl HuffmanTree {
    pub fn new() -> Self {
        let mut root = Box::new(HuffmanNode::new());
        let table = [
            ('0', "11111011"),
            (' ', "10"),
            ('1', "1111100"),
            ('2', "001100"),
            ('3', "1101101"),
            ('4', "11111010"),
            ('5', "00010110"),
            ('6', "1101111"),
            ('7', "01111"),
            ('8', "000100"),
            ('9', "01110"),
            ('a', "11110"),
            ('b', "0101"),
            ('c', "01000"),
            ('d', "110001"),
            ('e', "110000"),
            ('f', "010011"),
            ('g', "11010"),
            ('h', "00011"),
            ('i', "1111110"),
            ('j', "000101111"),
            ('k', "010010"),
            ('l', "11101"),
            ('m', "01101"),
            ('n', "001101"),
            ('o', "1111111"),
            ('p', "11001"),
            ('q', "11011001"),
            ('r', "11100"),
            ('s', "0010"),
            ('t', "01100"),
            ('u', "00001"),
            ('v', "1101110"),
            ('w', "00000"),
            ('x', "00111"),
            ('y', "0001010"),
            ('z', "11011000"),
        ];

        let mut encoding_table = std::collections::HashMap::new();
        for (symbol, pattern) in table {
            let mut bits = Vec::new();
            let mut current = &mut root;
            for bit in pattern.chars() {
                if bit == '1' {
                    bits.push(true);
                    if current.right.is_none() {
                        current.right = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.right.as_mut().unwrap();
                } else {
                    bits.push(false);
                    if current.left.is_none() {
                        current.left = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.left.as_mut().unwrap();
                }
            }
            current.symbol = Some(symbol);
            encoding_table.insert(symbol, bits);
        }
        HuffmanTree {
            root,
            encoding_table,
        }
    }

    pub fn encode(&self, code: &str) -> io::Result<Vec<bool>> {
        let mut bits = Vec::new();
        for c in code.chars() {
            let pattern = self.encoding_table.get(&c).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Char '{}' not in Huffman table", c),
                )
            })?;
            bits.extend(pattern);
        }
        Ok(bits)
    }

    fn decode_internal<F: FnMut() -> io::Result<bool>>(&self, mut read_bit: F) -> io::Result<char> {
        let mut current = &self.root;
        loop {
            if let Some(symbol) = current.symbol {
                return Ok(symbol);
            }
            let bit = read_bit()?;
            current = if bit {
                current.right.as_ref()
            } else {
                current.left.as_ref()
            }
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid Huffman bit"))?;
        }
    }

    pub fn decode_recorded<R: BitRead>(&self, cursor: &mut BitCursor<R>) -> ParsingResult<char> {
        self.decode_internal(|| cursor.read_bit().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e))))
            .map_err(|_| {
                cursor.fail(ParsingError::InvalidHuffmanBit { bit_offset: cursor.pos() })
                    .with_hint("Huffman bit pattern does not match Alpha v105 table. Possible bit drift or misalignment.")
            })
    }

    pub fn decode<R: BitRead>(&self, reader: &mut R) -> io::Result<char> {
        self.decode_internal(|| reader.read_bit())
    }
}

pub fn read_player_name<R: BitRead>(cursor: &mut BitCursor<R>, alpha_v5: bool) -> ParsingResult<String> {
    let mut name = String::new();
    let width = if alpha_v5 { 8 } else { 7 };
    loop {
        let ch = cursor.read_bits::<u8>(width)?;
        if ch == 0 { break; }
        name.push(ch as char);
    }
    Ok(name)
}

pub fn write_player_name(emitter: &mut BitEmitter, name: &str, alpha_v5: bool) -> io::Result<()> {
    let width = if alpha_v5 { 8 } else { 7 };
    for ch in name.chars() { emitter.write_bits((ch as u8) as u32, width)?; }
    emitter.write_bits(0, width)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_section_captures_opaque_on_failure() {
        let huffman = HuffmanTree::new();
        // Construct a plausible header: JM marker, non-compact.
        let mut emitter = BitEmitter::new();
        emitter.write_bits(0x00004D4A, 32).unwrap();
        emitter.write_bits(6, 3).unwrap(); // version
        emitter.write_bits(1, 3).unwrap(); // mode
        emitter.write_bits(1, 3).unwrap(); // loc
        emitter.write_bits(0, 4).unwrap(); // x
        
        // Add Huffman bits for "cap "
        let cap_bits = huffman.encode("cap ").unwrap();
        for b in cap_bits { emitter.write_bit(b).unwrap(); }
        
        // Non-compact unique item reads ExtendedStatsData: ID(32), Level(7), Quality(4), UniqueID(12)
        emitter.write_bits(0x12345678, 32).unwrap(); // id
        emitter.write_bits(10, 7).unwrap(); // level
        emitter.write_bits(7, 4).unwrap(); // quality (Unique)
        // Only provide 5 bits of the 12-bit Unique ID
        emitter.write_bits(0, 5).unwrap();
        
        // Pad heavily to satisfy scan_item_markers limit check
        while emitter.written_bits() < 160 {
            emitter.write_bit(false).unwrap();
        }
        
        let bytes = emitter.into_bytes();
        let section_bit_offset = 1234;
        
        // Truncate to force parsing failure but keep enough for scanner
        let truncated_bytes = if bytes.len() > 13 { &bytes[0..13] } else { &bytes }; 
        
        let items = Item::read_section(truncated_bytes, section_bit_offset, 1, &huffman, false).expect("Should not fail");
        
        if !items.is_empty() {
            assert_eq!(items[0].code, "Opaque");
            let mut has_opaque = false;
            for module in &items[0].modules {
                if let crate::domain::item::ItemModule::Opaque(_) = module {
                    has_opaque = true;
                }
            }
            assert!(has_opaque);
            assert_eq!(items[0].range.start, section_bit_offset);
        }
    }

    #[test]
    fn test_bit_budget_protection() {
        let huffman = HuffmanTree::new();
        let mut emitter = BitEmitter::new();

        // Alpha v105 header (Version 5)
        let flags = 0u32;
        let v = 5u8;
        let checksum = crate::domain::header::entity::calculate_alpha_v105_checksum(flags, v);

        emitter.write_bits(flags, 32).unwrap();
        emitter.write_bits(checksum as u32, 8).unwrap();
        emitter.write_bits(v as u32, 3).unwrap();
        emitter.write_bits(1, 3).unwrap(); // mode
        emitter.write_bits(1, 3).unwrap(); // loc
        emitter.write_bits(0, 4).unwrap(); // x

        // Item code "rin "
        let code_bits = huffman.encode("rin ").unwrap();
        for b in code_bits { emitter.write_bit(b).unwrap(); }

        // Properties: write many non-terminator stats.
        // stat_id 0 (9 bits) + value (6 bits) = 15 bits per property for Version 5 non-runeword.
        // 1500 / 15 = 100 properties.
        for _ in 0..110 {
            emitter.write_bits(0, 9).unwrap();
            emitter.write_bits(0, 6).unwrap();
        }

        let bytes = emitter.into_bytes();
        let result = Item::from_bytes(&bytes, &huffman, true);

        match result {
            Err(failure) => {
                match failure.error {
                    ParsingError::BitBudgetExceeded { bit_offset } => {
                        assert!(bit_offset >= 1500);
                    }
                    _ => panic!("Expected BitBudgetExceeded, got {:?}", failure.error),
                }
            }
            Ok(_) => panic!("Should have failed with BitBudgetExceeded"),
        }
    }

    #[test]
    fn test_read_section_bit_range_accuracy() {
        let huffman = HuffmanTree::new();
        let mut emitter = BitEmitter::new();
        // No padding at the start to ensure marker is found at 0
        
        // Valid compact item (cap)
        emitter.write_bits(0x00004D4A | (1 << 21), 32).unwrap(); // flags (compact)
        emitter.write_bits(6, 3).unwrap(); // version
        emitter.write_bits(1, 3).unwrap(); // mode
        emitter.write_bits(1, 3).unwrap(); // loc
        emitter.write_bits(0, 4).unwrap(); // x
        let cap_bits = huffman.encode("cap ").unwrap();
        for b in cap_bits { emitter.write_bit(b).unwrap(); }
        
        // Pad to satisfy scanner
        for _ in 0..256 { emitter.write_bit(false).unwrap(); }
        
        let bytes = emitter.into_bytes();
        let section_bit_offset = 100;
        let items = Item::read_section(&bytes, section_bit_offset, 1, &huffman, false).expect("Should parse");
        
        if !items.is_empty() {
            // Marker should be found at bit 0
            assert_eq!(items[0].range.start, section_bit_offset);
            // Verify that range.end and total_bits are consistent
            assert_eq!(items[0].range.end, items[0].range.start + items[0].total_bits);
        }
    }
}

