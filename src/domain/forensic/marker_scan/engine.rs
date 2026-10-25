//! Marker exploration independent of the concrete Item parser.
use super::{ItemMarker, MarkerStatus, ScannerWorkCounters};
use crate::data::generated::layout_policy::LegacyScannerMarkerClassificationPolicy as MarkerPolicy;
use crate::domain::forensic::header_scan::{
    is_plausible_item_header, peek_item_header_at_specific_gap,
    peek_item_header_at_with_gap_profile, peek_item_header_at_with_gap_profile_and_trial_gap,
    verify_marker_lookahead,
};
use crate::domain::forensic::header_types::AlphaScannerGapProfile;
use crate::domain::payload::HuffmanTree;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    static IN_SCANNER_JUMP_PEEK: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

fn is_alpha_v105_shadow_marker(code: &str) -> bool {
    let trimmed = code.trim();
    // Shadow markers observed in Alpha v105 runewords/socketed items.
    MarkerPolicy::SHADOW_CODES.contains(&trimmed)
        || (trimmed.starts_with('r')
            && trimmed.len() <= MarkerPolicy::SHADOW_RUNE_MAX_CODE_BYTES
            && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
}

fn is_alpha_v105_authority_marker(code: &str) -> bool {
    MarkerPolicy::AUTHORITY_CODES.contains(&code.trim())
}

fn is_alpha_v105_socket_child_marker(code: &str) -> bool {
    let trimmed = code.trim();
    if MarkerPolicy::SOCKET_CHILD_CODES.contains(&trimmed) {
        return true;
    }

    let bytes = trimmed.as_bytes();
    bytes.len() >= MarkerPolicy::SOCKET_RUNE_MIN_CODE_BYTES
        && bytes.len() <= MarkerPolicy::SOCKET_RUNE_MAX_CODE_BYTES
        && bytes[0] == b'r'
        && bytes[1..].iter().all(|b| b.is_ascii_digit())
}

const SCAN_CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks for parallel scanning

#[derive(Default)]
pub(super) struct ScannerWorkAccumulator {
    probes: AtomicU64,
    base_header_peeks: AtomicU64,
    alternate_gap_peeks: AtomicU64,
    lookahead_checks: AtomicU64,
    base_header_peek_micros: AtomicU64,
    alternate_gap_peek_micros: AtomicU64,
    lookahead_check_micros: AtomicU64,
}

impl ScannerWorkAccumulator {
    pub(super) fn snapshot(&self) -> ScannerWorkCounters {
        ScannerWorkCounters {
            probes: self.probes.load(Ordering::Relaxed),
            base_header_peeks: self.base_header_peeks.load(Ordering::Relaxed),
            alternate_gap_peeks: self.alternate_gap_peeks.load(Ordering::Relaxed),
            lookahead_checks: self.lookahead_checks.load(Ordering::Relaxed),
            base_header_peek_micros: self.base_header_peek_micros.load(Ordering::Relaxed),
            alternate_gap_peek_micros: self.alternate_gap_peek_micros.load(Ordering::Relaxed),
            lookahead_check_micros: self.lookahead_check_micros.load(Ordering::Relaxed),
        }
    }
}

/// The borrowed view required by the scanner; the concrete parsed value stays owned.
pub(super) trait JumpPeekItem {
    fn recorded_bit_len(&self) -> u64;
}

pub(super) type JumpPeek<T, E> = fn(
    &[u8],
    u64,
    u64,
    &HuffmanTree,
    usize,
    bool,
    Option<u64>,
    Option<bool>,
    Option<&str>,
) -> Result<(T, u64), E>;

/// Immutable scan selection and the caller's existing jump parser.
pub(super) struct ScanContext<T, E> {
    pub(super) gap_profile: AlphaScannerGapProfile,
    pub(super) jump_peek: JumpPeek<T, E>,
}

pub(super) fn scan_item_markers_internal<T: JumpPeekItem, E: std::fmt::Debug>(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_bit_offset: u64,
    expected_count: Option<u16>,
    verbose: bool,
    work: Option<&ScannerWorkAccumulator>,
    deadline: Option<std::time::Instant>,
    timed_out: Option<&std::sync::atomic::AtomicBool>,
    context: ScanContext<T, E>,
) -> Vec<ItemMarker> {
    let ScanContext {
        gap_profile,
        jump_peek,
    } = context;
    if bytes.is_empty() {
        return Vec::new();
    }

    let limit_bits = (bytes.len() * 8) as u64;

    let mut force_length_map = std::collections::HashMap::new();
    if let Ok(env_val) = std::env::var("D2R_FORCE_LENGTH") {
        for pair in env_val.split(',') {
            let parts: Vec<&str> = pair.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(offset), Ok(length)) = (
                    parts[0].trim().parse::<u64>(),
                    parts[1].trim().parse::<u64>(),
                ) {
                    force_length_map.insert(offset, length);
                }
            }
        }
    }

    let mut has_alpha_item = false;

    let num_chunks = (bytes.len() + SCAN_CHUNK_SIZE - 1) / SCAN_CHUNK_SIZE;
    let markers: Vec<(u64, u32, String, i8, u64, bool)> = (0..num_chunks)
        .into_iter()
        .flat_map(|chunk_idx| {
            let start_byte = chunk_idx * SCAN_CHUNK_SIZE;
            let end_byte = ((chunk_idx + 1) * SCAN_CHUNK_SIZE).min(bytes.len());

            let start_bit = (start_byte * 8) as u64;
            let _end_bit = ((end_byte * 8) as u64 + 256).min(limit_bits);

            let mut local_markers: Vec<(u64, u32, String, i8, u64, bool)> = Vec::new();
            let section_header_bits = if alpha && chunk_idx == 0 {
                let mut p = 32;
                if let Some((_, _, _, _, _, version, _, _, _, _)) =
                    peek_item_header_at_with_gap_profile(bytes, 32, huffman, alpha, 0, gap_profile)
                {
                    p = crate::domain::forensic::v105::axioms::V105JmMarkerAxiom::default()
                        .header_bits(version) as u64;
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
                if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                    if let Some(timed_out) = timed_out {
                        timed_out.store(true, Ordering::Relaxed);
                    }
                    break;
                }
                if let Some(work) = work {
                    work.probes.fetch_add(1, Ordering::Relaxed);
                }
                let mut best_offset = 0;
                let mut max_confidence = 0;
                let mut best_code = String::new();
                let mut best_corrected_gap = 0;
                let mut best_trial_gap = 0;
                let mut best_trial_gap_is_rhythm = false;

                let nudge_range = 8;
                for offset in 0..nudge_range {
                    let scan_pos = probe + offset;
                    let safety_margin = 72;
                    if scan_pos + safety_margin > limit_bits {
                        continue;
                    }

                    if let Some(work) = work {
                        work.base_header_peeks.fetch_add(1, Ordering::Relaxed);
                    }
                    let base_peek_started = std::time::Instant::now();
                    let mut header_candidate = peek_item_header_at_with_gap_profile_and_trial_gap(
                        bytes, scan_pos, huffman, alpha, 0, gap_profile,
                    );
                    if let Some(work) = work {
                        work.base_header_peek_micros.fetch_add(
                            base_peek_started.elapsed().as_micros() as u64,
                            Ordering::Relaxed,
                        );
                    }
                    if alpha {
                        let reg = crate::domain::forensic::registry::get_registry();
                        for alt_gap in [6u64, 27, 35, 46] {
                            if let Some(work) = work {
                                work.alternate_gap_peeks.fetch_add(1, Ordering::Relaxed);
                            }
                            let alternate_peek_started = std::time::Instant::now();
                            if let Some((
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
                            )) = peek_item_header_at_specific_gap(
                                bytes, scan_pos, huffman, alpha, alt_gap,
                            ) {
                                let trimmed_alt = code.trim();
                                let mut is_auth = false;
                                if let Some(overrides) = &reg.item_overrides {
                                    if let Some(map) = overrides.get(trimmed_alt) {
                                        if let Some(&val) = map.get("is_authority_overlap") {
                                            is_auth = val != 0;
                                        }
                                    }
                                }
                                if is_auth {
                                    header_candidate = Some((
                                        (
                                            mode, location, _x, code, flags, version, is_compact,
                                            _header_len, _nudge, has_checksum,
                                        ),
                                        alt_gap,
                                        false,
                                    ));
                                    break;
                                }
                            }
                            if let Some(work) = work {
                                work.alternate_gap_peek_micros.fetch_add(
                                    alternate_peek_started.elapsed().as_micros() as u64,
                                    Ordering::Relaxed,
                                );
                            }
                        }
                    }

                    if let Some((
                        (
                        mode,
                        location,
                        _x,
                        code,
                        flags,
                        version,
                        is_compact,
                        _header_len,
                        corrected_gap,
                        has_checksum,
                        ),
                        trial_gap,
                        trial_gap_is_rhythm,
                    )) = header_candidate
                    {
                        let use_v105_alignment = alpha && version != 6;
                        if is_plausible_item_header(
                            mode,
                            location,
                            code.as_bytes(),
                            flags,
                            version,
                            alpha,
                        ) {
                            if alpha && version != 6 {
                                has_alpha_item = true;
                            }
                            let trimmed_code = code.trim();
                            let is_known =
                                crate::domain::forensic::v105::axioms::is_v105_summary_code(&code)
                                    || crate::data::repository::legacy_item_template(&code)
                                        .is_some();
                            let reg = crate::domain::forensic::registry::get_registry();
                            let override_noncompact = reg
                                .item_overrides
                                .as_ref()
                                .and_then(|overrides| overrides.get(trimmed_code))
                                .and_then(|map| map.get("is_compact"))
                                .map(|&val| val == 0)
                                .unwrap_or(false);

                            if alpha && version == 5 && !has_checksum && !is_known {
                                continue;
                            }
                            if alpha && chunk_idx == 0 && scan_pos < section_header_bits {
                                continue;
                            }

                            let mut is_forced = false;
                            let absolute_offset = section_bit_offset + scan_pos;
                            if force_length_map.contains_key(&absolute_offset) {
                                is_forced = true;
                            }

                            let is_v105_summary =
                                crate::domain::forensic::v105::axioms::is_v105_summary_code(&code);

                            let mut forced_80 = false;
                            if use_v105_alignment && !is_compact && !is_forced {
                                if is_v105_summary {
                                    if let Some(work) = work {
                                        work.base_header_peeks.fetch_add(1, Ordering::Relaxed);
                                    }
                                    let summary_peek_started = std::time::Instant::now();
                                    if let Some(next_header) = peek_item_header_at_with_gap_profile(
                                        bytes, scan_pos + 80, huffman, alpha, 0, gap_profile,
                                    )
                                    {
                                        let (n_mode, n_loc, _, n_code, n_flags, n_ver, _, _, _, _) =
                                            (next_header.0, next_header.1, next_header.2, next_header.3, next_header.4, next_header.5, next_header.6, next_header.7, next_header.8, next_header.9);
                                        if is_plausible_item_header(
                                            n_mode,
                                            n_loc,
                                            n_code.as_bytes(),
                                            n_flags,
                                            n_ver,
                                            alpha,
                                        ) {
                                            forced_80 = true;
                                        }
                                    }
                                    if let Some(work) = work {
                                        work.base_header_peek_micros.fetch_add(
                                            summary_peek_started.elapsed().as_micros() as u64,
                                            Ordering::Relaxed,
                                        );
                                    }
                                }
                            }

                            let mut confidence = if is_known { 500 } else { 50 };
                            let header_axiom =
                                crate::domain::forensic::header_policy::HeaderAxiom::new(version, alpha);
                            let is_alpha_runeword_candidate =
                                alpha && header_axiom.is_runeword(flags, Some(&code), has_checksum);
                            if override_noncompact {
                                confidence += 300;
                            }

                            if use_v105_alignment
                                && !is_compact
                                && !is_forced
                                && !forced_80
                                && !(is_alpha_runeword_candidate && has_checksum)
                                && !is_v105_summary
                                && !override_noncompact
                                && !is_known
                            {
                                if let Some(work) = work {
                                    work.lookahead_checks.fetch_add(1, Ordering::Relaxed);
                                }
                                let lookahead_started = std::time::Instant::now();
                                let lookahead_valid = verify_marker_lookahead(
                                    bytes,
                                    scan_pos + _header_len,
                                    huffman,
                                    alpha,
                                );
                                if let Some(work) = work {
                                    work.lookahead_check_micros.fetch_add(
                                        lookahead_started.elapsed().as_micros() as u64,
                                        Ordering::Relaxed,
                                    );
                                }
                                if !lookahead_valid {
                                    continue;
                                }
                            }

                            if alpha
                                && (trimmed_code == "hp1"
                                    || trimmed_code == "xrs"
                                    || trimmed_code == "wa2")
                            {
                                confidence += 200;
                            }
                            if alpha
                                && is_alpha_runeword_candidate
                                && (trimmed_code == "xrs" || trimmed_code == "wa2")
                            {
                                confidence += 300;
                            }
                            if alpha && version == 5 {
                                confidence += 100;
                            }
                            if alpha && has_checksum {
                                confidence += 100;
                            }

                            if use_v105_alignment {
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

                                if trimmed_code == "xrs"
                                    || trimmed_code == "c8xr"
                                    || trimmed_code == "rhd"
                                    || trimmed_code == "wa2"
                                {
                                    if rem == 2 {
                                        confidence += 1000;
                                    } else if (trimmed_code == "wa2" || trimmed_code == "rhd")
                                        && rem == 6
                                    {
                                        confidence += 1000;
                                    } else {
                                        continue;
                                    }
                                } else if trimmed_code == "msk" {
                                    if rem == 0 {
                                        confidence += 1000;
                                    } else {
                                        continue;
                                    }
                                }
                            }

                            if confidence > max_confidence
                                || (alpha
                                    && version != 6
                                    && confidence == max_confidence
                                    && (scan_pos % 8 == 2))
                            {
                                max_confidence = confidence;
                                best_offset = scan_pos;
                                best_code = code.clone();
                                best_corrected_gap = corrected_gap;
                                best_trial_gap = trial_gap;
                                best_trial_gap_is_rhythm = trial_gap_is_rhythm;
                            }
                        }
                    }
                }

                if max_confidence > 0 {
                    local_markers.push((
                        best_offset,
                        max_confidence,
                        best_code.clone(),
                        best_corrected_gap,
                        best_trial_gap,
                        best_trial_gap_is_rhythm,
                    ));

                    let jump_peek_started = alpha.then(std::time::Instant::now);
                    let jump = if alpha {
                        if let Some(work) = work {
                            work.base_header_peeks.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Some((_, _, _, _, f, v, _, _, _, _)) =
                            peek_item_header_at_with_gap_profile(
                                bytes, best_offset, huffman, alpha, 0, gap_profile,
                            )
                        {
                            if v != 6 {
                                let mut j =
                                    crate::domain::forensic::v105::axioms::get_v105_target_width(
                                        v,
                                        &best_code,
                                        f,
                                        Some(local_markers.len()),
                                    ) as u64;
                                if (best_code.trim() == "xrs" || best_code.trim() == "wa2")
                                    && j == 0
                                {
                                    j = 150;
                                } else if best_code.trim() == "msk" && j == 0 {
                                    j = 328;
                                } else if j == 0
                                    && !is_alpha_v105_compact_limit_item(&best_code)
                                    && !IN_SCANNER_JUMP_PEEK.with(|c| c.get())
                                {
                                    // Non-compact equipment: parse with limit up to section/chunk boundary
                                    // to jump past its variable properties and prevent phantom markers inside property stream.
                                    IN_SCANNER_JUMP_PEEK.with(|c| c.set(true));
                                    let parse_res =
                                        jump_peek(
                                            bytes,
                                            best_offset,
                                            0,
                                            huffman,
                                            local_markers.len(),
                                            alpha,
                                            None,
                                            None,
                                            Some(best_code.as_str()),
                                        );
                                    IN_SCANNER_JUMP_PEEK.with(|c| c.set(false));
                                    match &parse_res {
                                        Ok((parsed_it, consumed)) => {
                                            let item_len = parsed_it.recorded_bit_len();
                                            if item_len > 0 && item_len < *consumed {
                                                j = (item_len + 7) & !7;
                                            } else if *consumed > 0 {
                                                j = *consumed;
                                            }
                                        }
                                        _ => {}
                                    }
                                    if crate::item::item_trace_enabled() {
                                        eprintln!("[scanner-jump] code={} best_offset={} j={} res={:?}", best_code.trim(), best_offset, j, parse_res.as_ref().map(|(_, c)| *c));
                                    }
                                }
                                if j > 0 {
                                    j
                                } else {
                                    72
                                }
                            } else {
                                72
                            }
                        } else {
                            72
                        }
                    } else {
                        72
                    };
                    if let (Some(work), Some(jump_peek_started)) = (work, jump_peek_started) {
                        work.base_header_peek_micros.fetch_add(
                            jump_peek_started.elapsed().as_micros() as u64,
                            Ordering::Relaxed,
                        );
                    }
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
    let is_known_false_oesw_chain = |index: usize| {
        if index == 0 {
            return false;
        }
        let (offset, _, code, _, trial_gap, trial_gap_is_rhythm) = &final_markers[index];
        let (previous_offset, _, previous_code, _, previous_trial_gap, previous_trial_gap_is_rhythm) =
            &final_markers[index - 1];
        let prev_trimmed = previous_code.trim();
        code.trim() == "oesw"
            && ((prev_trimmed == "cap" && *offset - *previous_offset == 280)
                || (*trial_gap == 32
                    && !*trial_gap_is_rhythm
                    && ((prev_trimmed == "9cs"
                        && *offset - *previous_offset == 81
                        && *previous_trial_gap == 24
                        && !*previous_trial_gap_is_rhythm)
                        || prev_trimmed == "msk")))
    };

    while i < final_markers.len() {
        let (offset, confidence, code_str, _, _, _) = &final_markers[i];
        let mut best_idx = i;
        let mut max_score = *confidence as i32;
        let use_v105_alignment = alpha && has_alpha_item;

        if use_v105_alignment && accepted_count > 0 {
            let diff = offset - last_offset;
            if is_alpha_v105_slot_item(&last_code) {
                if diff == 80 {
                    max_score += 350;
                } else if diff == 72 || diff == 73 {
                    max_score += 250;
                } else if is_v105_aligned(diff) {
                    max_score += 150;
                }
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

        let lookahead_limit = if use_v105_alignment {
            if is_alpha_v105_compact_limit_item(code_str) {
                offset + 72
            } else {
                offset + 128
            }
        } else {
            offset + 120
        };
        let mut j = i + 1;
        while j < final_markers.len() && final_markers[j].0 < lookahead_limit {
            let (o_offset, o_conf, o_code, _, _, _) = &final_markers[j];
            let mut score = *o_conf as i32;
            let next_use_v105_alignment = alpha && has_alpha_item;

            if next_use_v105_alignment && accepted_count > 0 {
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

                if next_use_v105_alignment {
                    let next_window = if is_alpha_v105_compact_limit_item(o_code) {
                        o_offset + 72
                    } else {
                        o_offset + 128
                    };
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
                    let is_aligned = if accepted_count == 0 {
                        false
                    } else {
                        is_v105_aligned(o_offset - last_offset)
                    };
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

        let (best_offset, best_confidence, best_code_str, best_corrected_gap, best_trial_gap, best_trial_gap_is_rhythm) =
            &final_markers[best_idx];
        let mut status = MarkerStatus::Accepted;
        let use_v105_alignment = alpha && has_alpha_item;

        if use_v105_alignment && is_known_false_oesw_chain(best_idx) {
            status = MarkerStatus::Phantom;
        }

        if status == MarkerStatus::Accepted && use_v105_alignment && max_score < 150 {
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
                    selected_corrected_gap: Some(*best_corrected_gap),
                    selected_trial_gap: Some(*best_trial_gap),
                    selected_trial_gap_is_rhythm: Some(*best_trial_gap_is_rhythm),
                });
                last_offset = *best_offset;
                last_code = best_code_str.clone();

                if is_alpha_v105_authority_marker(&last_code) {
                    let limit = if last_code.trim() == MarkerPolicy::SHORT_AUTHORITY_WINDOW_CODE {
                        MarkerPolicy::SHORT_AUTHORITY_WINDOW_BITS
                    } else {
                        MarkerPolicy::DEFAULT_AUTHORITY_WINDOW_BITS
                    };
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
                    selected_corrected_gap: Some(*best_corrected_gap),
                    selected_trial_gap: Some(*best_trial_gap),
                    selected_trial_gap_is_rhythm: Some(*best_trial_gap_is_rhythm),
                });
            }
        } else if verbose {
            all_markers.push(ItemMarker {
                offset: *best_offset,
                confidence: *best_confidence,
                code: best_code_str.clone(),
                score: max_score,
                status: status,
                selected_corrected_gap: Some(*best_corrected_gap),
                selected_trial_gap: Some(*best_trial_gap),
                selected_trial_gap_is_rhythm: Some(*best_trial_gap_is_rhythm),
            });
        }

        let skip_until = best_offset + 72;
        i = best_idx + 1;
        while i < final_markers.len() && final_markers[i].0 < skip_until {
            if verbose && !filtered_indices.contains(&i) {
                let (o_offset, o_conf, o_code, o_corrected_gap, o_trial_gap, o_trial_gap_is_rhythm) = &final_markers[i];
                all_markers.push(ItemMarker {
                    offset: *o_offset,
                    confidence: *o_conf,
                    code: o_code.clone(),
                    score: *o_conf as i32,
                    status: MarkerStatus::Rejected,
                    selected_corrected_gap: Some(*o_corrected_gap),
                    selected_trial_gap: Some(*o_trial_gap),
                    selected_trial_gap_is_rhythm: Some(*o_trial_gap_is_rhythm),
                });
            }
            i += 1;
        }
    }

    if verbose {
        all_markers.sort_unstable_by_key(|m| m.offset);
        all_markers
    } else {
        all_markers
            .into_iter()
            .filter(|m| m.status == MarkerStatus::Accepted)
            .collect()
    }
}

fn is_alpha_v105_slot_item(code: &str) -> bool {
    let trimmed = code.trim();
    if matches!(
        trimmed,
        "hp1"
            | "hp2"
            | "hp3"
            | "hp4"
            | "hp5"
            | "mp1"
            | "mp2"
            | "mp3"
            | "mp4"
            | "mp5"
            | "whp1"
            | "whp2"
            | "whp3"
            | "whp4"
            | "whp5"
            | "wmp1"
            | "wmp2"
            | "wmp3"
            | "wmp4"
            | "wmp5"
            | "rvs"
            | "rvl"
            | "vps"
            | "tsc"
            | "isc"
            | "jav"
            | "yps"
            | "wps"
            | "w8cs"
            | "w88w"
            | "xrs"
            | "wyws"
            | "6cs"
            | "7mgw"
            | "fsh"
            | "7pus"
            | "ww7c"
            | "mxh"
            | "d ew"
            | "ghm"
            | "amu"
            | "rin"
            | "cm1"
            | "vbt"
            | "vgl"
            | "hbl"
            | "tri"
            | "dr1"
            | "key"
            | "mac"
            | "ulss"
            | "9tr"
            | "swsp"
    ) {
        return true;
    }
    crate::domain::forensic::v105::axioms::is_v105_summary_code(code)
}

fn is_alpha_v105_compact_limit_item(code: &str) -> bool {
    let trimmed = code.trim();
    if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed) {
        return true;
    }
    matches!(
        trimmed,
        "hp1"
            | "hp2"
            | "hp3"
            | "hp4"
            | "hp5"
            | "mp1"
            | "mp2"
            | "mp3"
            | "mp4"
            | "mp5"
            | "whp1"
            | "whp2"
            | "whp3"
            | "whp4"
            | "whp5"
            | "wmp1"
            | "wmp2"
            | "wmp3"
            | "wmp4"
            | "wmp5"
            | "rvs"
            | "rvl"
            | "vps"
            | "tsc"
            | "isc"
            | "yps"
            | "wps"
            | "w8cs"
            | "w88w"
            | "wyws"
            | "e w"
            | "key"
            | "mac"
            | "ulss"
            | "9tr"
    )
}

fn is_v105_aligned(diff: u64) -> bool {
    matches!(
        diff,
        72 | 73
            | 74
            | 80
            | 81
            | 82
            | 88
            | 89
            | 90
            | 144
            | 145
            | 146
            | 152
            | 153
            | 154
            | 160
            | 161
            | 162
            | 168
            | 176
            | 216
            | 224
            | 232
            | 240
    )
}


#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::hypothesis_test;
    use std::cell::Cell;

    thread_local! {
        static CALLS: Cell<usize> = const { Cell::new(0) };
        static LENGTH_READS: Cell<usize> = const { Cell::new(0) };
        static DROPS: Cell<usize> = const { Cell::new(0) };
        static EXPECTED_BYTES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
        static EXPECTED_HUFFMAN: Cell<usize> = const { Cell::new(0) };
    }

    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            IN_SCANNER_JUMP_PEEK.with(|flag| flag.set(false));
        }
    }

    struct Probe;
    impl JumpPeekItem for Probe {
        fn recorded_bit_len(&self) -> u64 {
            assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
            LENGTH_READS.with(|n| n.set(n.get() + 1));
            13
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
            DROPS.with(|n| n.set(n.get() + 1));
        }
    }

    fn sections() -> Vec<Vec<u8>> {
        let root = std::env::var_os("D2R_CORE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        ["amazon_v105_slice2_equipment.d2s", "amazon_10_scrolls.d2s"]
            .into_iter()
            .map(|name| {
                let bytes =
                    std::fs::read(root.join("tests/fixtures/savegames/original").join(name))
                        .unwrap();
                let jm: Vec<_> = bytes
                    .windows(2)
                    .enumerate()
                    .filter_map(|(i, b)| (b == b"JM").then_some(i))
                    .collect();
                assert!(!jm.is_empty());
                bytes[jm[0]..jm.get(1).copied().unwrap_or(bytes.len())].to_vec()
            })
            .collect()
    }

    fn reset() -> ResetGuard {
        assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
        CALLS.with(|n| n.set(0));
        LENGTH_READS.with(|n| n.set(0));
        DROPS.with(|n| n.set(0));
        ResetGuard
    }

    fn scan(bytes: &[u8], huffman: &HuffmanTree, callback: JumpPeek<Probe, &'static str>) {
        let _ = scan_item_markers_internal(
            bytes,
            huffman,
            true,
            0,
            None,
            false,
            None,
            None,
            None,
            ScanContext {
                gap_profile: AlphaScannerGapProfile::Extended,
                jump_peek: callback,
            },
        );
    }

    fn exercise(callback: JumpPeek<Probe, &'static str>) {
        let huffman = HuffmanTree::new();
        EXPECTED_HUFFMAN.with(|value| value.set(&huffman as *const _ as usize));
        for bytes in sections() {
            EXPECTED_BYTES.with(|value| value.set((bytes.as_ptr() as usize, bytes.len())));
            scan(&bytes, &huffman, callback);
        }
    }

    fn success(
        bytes: &[u8],
        bit: u64,
        base: u64,
        huffman: &HuffmanTree,
        _idx: usize,
        alpha: bool,
        limit: Option<u64>,
        compact: Option<bool>,
        code: Option<&str>,
    ) -> Result<(Probe, u64), &'static str> {
        assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
        assert_eq!(
            EXPECTED_BYTES.with(Cell::get),
            (bytes.as_ptr() as usize, bytes.len())
        );
        assert_eq!(
            EXPECTED_HUFFMAN.with(Cell::get),
            huffman as *const _ as usize
        );
        assert!(bit < bytes.len() as u64 * 8);
        assert_eq!(base, 0);
        assert!(alpha && limit.is_none() && compact.is_none());
        assert!(code.is_some_and(|code| !code.is_empty()));
        CALLS.with(|n| n.set(n.get() + 1));
        Ok((Probe, 24))
    }

    fn forbidden(
        _bytes: &[u8],
        _bit: u64,
        _base: u64,
        _huffman: &HuffmanTree,
        _idx: usize,
        _alpha: bool,
        _limit: Option<u64>,
        _compact: Option<bool>,
        _code: Option<&str>,
    ) -> Result<(Probe, u64), &'static str> {
        panic!("nested jump parser must remain suppressed");
    }

    fn error(
        bytes: &[u8],
        _bit: u64,
        _base: u64,
        huffman: &HuffmanTree,
        _idx: usize,
        _alpha: bool,
        _limit: Option<u64>,
        _compact: Option<bool>,
        _code: Option<&str>,
    ) -> Result<(Probe, u64), &'static str> {
        assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
        let first = CALLS.with(|n| {
            let first = n.get() == 0;
            n.set(n.get() + 1);
            first
        });
        if first {
            scan(bytes, huffman, forbidden);
            assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
        }
        Err("jump-error")
    }

    fn panicking(
        _bytes: &[u8],
        _bit: u64,
        _base: u64,
        _huffman: &HuffmanTree,
        _idx: usize,
        _alpha: bool,
        _limit: Option<u64>,
        _compact: Option<bool>,
        _code: Option<&str>,
    ) -> Result<(Probe, u64), &'static str> {
        assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
        CALLS.with(|n| n.set(n.get() + 1));
        panic!("legacy jump panic snapshot");
    }

    // Software preservation snapshots, not physical game invariants.
    #[hypothesis_test]
    fn jump_callback_preserves_guard_and_owned_result_lifetime() {
        let _reset = reset();
        exercise(success);
        let calls = CALLS.with(Cell::get);
        assert!(calls > 0);
        assert_eq!(LENGTH_READS.with(Cell::get), calls);
        assert_eq!(DROPS.with(Cell::get), calls);
        assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
    }

    #[hypothesis_test]
    fn jump_callback_preserves_nested_suppression_and_error_reset() {
        let _reset = reset();
        exercise(error);
        let calls = CALLS.with(Cell::get);
        assert!(calls > 0);
        assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
        exercise(error);
        assert!(CALLS.with(Cell::get) > calls);
        assert!(!IN_SCANNER_JUMP_PEEK.with(Cell::get));
    }

    #[hypothesis_test]
    fn jump_callback_preserves_legacy_sticky_panic_guard() {
        let _reset = reset();
        let panic = std::panic::catch_unwind(|| exercise(panicking));
        assert!(panic.is_err());
        assert_eq!(CALLS.with(Cell::get), 1);
        assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
        exercise(forbidden);
        assert!(IN_SCANNER_JUMP_PEEK.with(Cell::get));
    }
}
