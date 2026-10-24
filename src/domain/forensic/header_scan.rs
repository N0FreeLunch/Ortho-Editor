//! Provisional forensic Header peeks, ranked gap trials and optional telemetry.
//! Candidate observations do not establish canonical item identity or ownership.
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    LegacyHeaderCodeTrialLayout as CodeTrialLayout, ALPHA_LEGACY_AUTHORITY_CODES,
};
use crate::data::repository::legacy_item_template as item_template;
use crate::domain::forensic::header_policy::HeaderAxiom;
use crate::domain::forensic::header_probe::ItemHeaderPeek;
use crate::domain::forensic::v105::axioms::is_v105_summary_code;
use crate::domain::forensic::v105::{V105HeaderGapAxiom, V105PropertyWidthAxiom};
use crate::domain::header::entity::calculate_alpha_v105_checksum;
use crate::domain::item::serialization::AlphaScannerGapProfile;
use crate::domain::payload::HuffmanTree;
use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::io::Cursor;

pub fn is_plausible_item_header(
    mode: u8,
    location: u8,
    code: &[u8],
    flags: u32,
    version: u8,
    alpha_mode: bool,
) -> bool {
    if alpha_mode {
        if let Ok(s) = std::str::from_utf8(code) {
            let trimmed = s.trim();
            if trimmed == "xrs" || trimmed == "mp1" {
                return mode <= 6 && location <= 5;
            }
        }
    }
    if alpha_mode && code.is_empty() {
        return mode <= 6 && location <= 5;
    }

    let decoded_code: std::borrow::Cow<[u8]> = if let Ok(s) = std::str::from_utf8(code) {
        if s.chars().any(|c| c as u32 > 127) {
            std::borrow::Cow::Owned(s.chars().map(|c| c as u32 as u8).collect())
        } else {
            std::borrow::Cow::Borrowed(code)
        }
    } else {
        std::borrow::Cow::Borrowed(code)
    };

    let axiom = HeaderAxiom::new(version, alpha_mode);
    axiom.is_plausible(mode, location, &decoded_code, flags)
}

#[derive(Clone, Copy)]
enum ScannerPeekTimingStage {
    HeaderSetup,
    TrialConfig,
    GapReader,
    CodeDecode,
    Plausibility,
    RegistryTemplate,
    Scoring,
    GeometryResolution,
    SummaryClassification,
    HeaderGeometry,
}

struct ScannerPeekTiming {
    enabled: bool,
    start_bit: u64,
    profile: AlphaScannerGapProfile,
    started_at: std::time::Instant,
    header_setup_us: u128,
    trial_config_us: u128,
    gap_reader_us: u128,
    code_decode_us: u128,
    plausibility_us: u128,
    registry_template_us: u128,
    scoring_us: u128,
    geometry_resolution_us: u128,
    summary_classification_us: u128,
    summary_classification_calls: u64,
    header_geometry_us: u128,
    header_geometry_calls: u64,
    header_geometry_cache_hits: u64,
    header_geometry_input_signatures: Option<std::collections::HashSet<(u8, u32, u8, String)>>,
    gap_attempts: u64,
}

impl ScannerPeekTiming {
    fn new(enabled: bool, start_bit: u64, profile: AlphaScannerGapProfile) -> Self {
        Self {
            enabled,
            start_bit,
            profile,
            started_at: std::time::Instant::now(),
            header_setup_us: 0,
            trial_config_us: 0,
            gap_reader_us: 0,
            code_decode_us: 0,
            plausibility_us: 0,
            registry_template_us: 0,
            scoring_us: 0,
            geometry_resolution_us: 0,
            summary_classification_us: 0,
            summary_classification_calls: 0,
            header_geometry_us: 0,
            header_geometry_calls: 0,
            header_geometry_cache_hits: 0,
            header_geometry_input_signatures: enabled.then(std::collections::HashSet::new),
            gap_attempts: 0,
        }
    }

    fn mark(&self) -> Option<std::time::Instant> {
        self.enabled.then(std::time::Instant::now)
    }

    fn record_header_geometry_input(&mut self, version: u8, flags: u32, mode: u8, code: &str) {
        if let Some(signatures) = &mut self.header_geometry_input_signatures {
            signatures.insert((version, flags, mode, code.to_owned()));
        }
    }

    fn record_header_geometry_cache_hit(&mut self) {
        if self.enabled {
            self.header_geometry_cache_hits += 1;
        }
    }

    fn record(&mut self, stage: ScannerPeekTimingStage, started: Option<std::time::Instant>) {
        let Some(started) = started else {
            return;
        };
        let elapsed = started.elapsed().as_micros();
        match stage {
            ScannerPeekTimingStage::HeaderSetup => self.header_setup_us += elapsed,
            ScannerPeekTimingStage::TrialConfig => self.trial_config_us += elapsed,
            ScannerPeekTimingStage::GapReader => self.gap_reader_us += elapsed,
            ScannerPeekTimingStage::CodeDecode => self.code_decode_us += elapsed,
            ScannerPeekTimingStage::Plausibility => self.plausibility_us += elapsed,
            ScannerPeekTimingStage::RegistryTemplate => self.registry_template_us += elapsed,
            ScannerPeekTimingStage::Scoring => self.scoring_us += elapsed,
            ScannerPeekTimingStage::GeometryResolution => self.geometry_resolution_us += elapsed,
            ScannerPeekTimingStage::SummaryClassification => {
                self.summary_classification_us += elapsed;
                self.summary_classification_calls += 1;
            }
            ScannerPeekTimingStage::HeaderGeometry => {
                self.header_geometry_us += elapsed;
                self.header_geometry_calls += 1;
            }
        }
    }
}

impl Drop for ScannerPeekTiming {
    fn drop(&mut self) {
        if self.enabled {
            let unique_geometry_inputs = self
                .header_geometry_input_signatures
                .as_ref()
                .map_or(0, |signatures| signatures.len() as u64);
            let geometry_requests = self
                .header_geometry_calls
                .saturating_add(self.header_geometry_cache_hits);
            eprintln!(
                "[scanner-peek-profile] start_bit={} gap_profile={} gap_attempts={} total_us={} header_setup_us={} trial_config_us={} gap_reader_us={} code_decode_us={} plausibility_us={} registry_template_us={} scoring_us={} geometry_resolution_us={} summary_classification_us={} summary_classification_calls={} header_geometry_us={} header_geometry_requests={} header_geometry_calls={} header_geometry_cache_misses={} header_geometry_cache_hits={} header_geometry_unique_inputs={} header_geometry_repeated_inputs={} non_geometry_scoring_us={}",
                self.start_bit,
                self.profile.label(),
                self.gap_attempts,
                self.started_at.elapsed().as_micros(),
                self.header_setup_us,
                self.trial_config_us,
                self.gap_reader_us,
                self.code_decode_us,
                self.plausibility_us,
                self.registry_template_us,
                self.scoring_us,
                self.geometry_resolution_us,
                self.summary_classification_us,
                self.summary_classification_calls,
                self.header_geometry_us,
                geometry_requests,
                self.header_geometry_calls,
                self.header_geometry_calls,
                self.header_geometry_cache_hits,
                unique_geometry_inputs,
                geometry_requests.saturating_sub(unique_geometry_inputs),
                self.scoring_us.saturating_sub(self.geometry_resolution_us),
            );
        }
    }
}

impl AlphaScannerGapProfile {
    pub fn label(self) -> String {
        match self {
            Self::Extended => "extended".to_string(),
            Self::RhythmOnly => "rhythm-only".to_string(),
            Self::WitnessSubset => "witness-subset".to_string(),
            Self::ExtendedExceptMask(mask) => format!("extended-except-mask-{mask}"),
        }
    }
}

pub fn peek_item_header_at(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    idx: usize,
) -> Option<ItemHeaderPeek> {
    peek_item_header_at_with_gap_profile(
        section_bytes,
        start_bit,
        huffman,
        alpha_mode,
        idx,
        AlphaScannerGapProfile::Extended,
    )
}

pub fn peek_item_header_at_with_gap_profile(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    idx: usize,
    gap_profile: AlphaScannerGapProfile,
) -> Option<ItemHeaderPeek> {
    peek_item_header_at_with_gap_profile_and_trial_gap(
        section_bytes,
        start_bit,
        huffman,
        alpha_mode,
        idx,
        gap_profile,
    )
    .map(|(header, _, _)| header)
}

pub fn peek_item_header_at_with_gap_profile_and_trial_gap(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    idx: usize,
    gap_profile: AlphaScannerGapProfile,
) -> Option<(ItemHeaderPeek, u64, bool)> {
    peek_item_header_at_with_base_and_gap_profile(
        section_bytes,
        start_bit,
        None,
        huffman,
        alpha_mode,
        idx,
        gap_profile,
    )
}

pub fn peek_item_header_at_with_base(
    section_bytes: &[u8],
    start_bit: u64,
    absolute_start_bit: Option<u64>,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    idx: usize,
) -> Option<ItemHeaderPeek> {
    peek_item_header_at_with_base_and_gap_profile(
        section_bytes,
        start_bit,
        absolute_start_bit,
        huffman,
        alpha_mode,
        idx,
        AlphaScannerGapProfile::Extended,
    )
    .map(|(header, _, _)| header)
}

fn peek_item_header_at_with_base_and_gap_profile(
    section_bytes: &[u8],
    start_bit: u64,
    absolute_start_bit: Option<u64>,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    idx: usize,
    gap_profile: AlphaScannerGapProfile,
) -> Option<(ItemHeaderPeek, u64, bool)> {
    static SCANNER_PROFILE_START_BIT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    static SCANNER_PROFILE_GAP_PROFILE: std::sync::OnceLock<Option<String>> =
        std::sync::OnceLock::new();
    let profile_start_matches = *SCANNER_PROFILE_START_BIT.get_or_init(|| {
        std::env::var("D2R_ITEM_SCANNER_PROFILE_START_BIT")
            .ok()
            .and_then(|value| value.parse().ok())
    }) == Some(start_bit);
    let profile_gap_matches = SCANNER_PROFILE_GAP_PROFILE
        .get_or_init(|| std::env::var("D2R_ITEM_SCANNER_PROFILE_GAP_PROFILE").ok())
        .as_deref()
        .map_or(true, |profile| profile == gap_profile.label());
    let profile_trace_enabled = alpha_mode && profile_start_matches && profile_gap_matches;
    let mut timing = ScannerPeekTiming::new(profile_trace_enabled, start_bit, gap_profile);
    let mut header_geometry_cache: Vec<(
        u8,
        u32,
        u8,
        String,
        crate::domain::forensic::header_policy::HeaderGeometry,
    )> = Vec::new();
    let header_setup_started = timing.mark();
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if reader.skip(start_bit as u32).is_err() {
        return None;
    }

    // Read header structure
    let flags = reader.read::<32, u32>().ok()?;

    let mut alpha_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    let _ = alpha_reader.skip(start_bit as u32 + 32);
    let _checksum = alpha_reader.read::<8, u8>().unwrap_or(0);
    let v = alpha_reader.read::<3, u8>().unwrap_or(0);
    let _calculated = calculate_alpha_v105_checksum(flags, v);
    let w_axiom = V105PropertyWidthAxiom::default();
    let version_bits = w_axiom.version_bits(alpha_mode);
    let mode_bits = w_axiom.mode_bits(alpha_mode);
    let location_bits = w_axiom.location_bits(alpha_mode, v);
    let x_bits = w_axiom.x_bits(alpha_mode, v);

    let mut retail_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    let mut v_retail = 0;
    let mut retail_skip_ok = false;
    if retail_reader.skip(start_bit as u32 + 32).is_ok() {
        v_retail = retail_reader.read::<3, u8>().unwrap_or(0);
        retail_skip_ok = true;
    }
    timing.record(ScannerPeekTimingStage::HeaderSetup, header_setup_started);

    let mut best_res: Option<(ItemHeaderPeek, u64, bool)> = None;
    let mut max_confidence = 0;

    let trial_config_started = timing.mark();
    let mut trial_configs = Vec::new();
    if alpha_mode && (v <= 7) {
        let _calculated = calculate_alpha_v105_checksum(flags, v);
        let matched = (_checksum == _calculated)
            || (alpha_mode
                && flags != 0
                && (v == 5 || v == 0 || v == 1 || v == 2 || v == 4 || v == 3));

        if matched {
            let m = alpha_reader.read::<3, u8>().ok();
            let l = match location_bits {
                4 => alpha_reader.read::<4, u8>().ok(),
                _ => alpha_reader.read::<3, u8>().ok(),
            };
            let x = match x_bits {
                3 => alpha_reader.read::<3, u8>().ok(),
                _ => alpha_reader.read::<4, u8>().ok(),
            };
            if let (Some(mode), Some(loc), Some(x_val)) = (m, l, x) {
                trial_configs.push((
                    v,
                    mode,
                    loc,
                    x_val,
                    32 + version_bits + 8 + mode_bits + location_bits + x_bits,
                    true,
                ));
            }
        }
    }

    if retail_skip_ok && !alpha_mode {
        let m = retail_reader.read::<3, u8>().ok();
        let l = retail_reader.read::<3, u8>().ok();
        let x = retail_reader.read::<4, u8>().ok();
        if let (Some(mode), Some(loc), Some(x_val)) = (m, l, x) {
            trial_configs.push((v_retail, mode, loc, x_val, 32 + 3 + 3 + 3 + 4 + 3, false));
            trial_configs.push((v_retail, mode, loc, x_val, 32 + 3 + 3 + 3 + 4, false));
        }
    }
    timing.record(ScannerPeekTimingStage::TrialConfig, trial_config_started);

    for (version, mode, loc, _x_val, base_header_len, has_checksum) in trial_configs {
        let h_axiom = HeaderAxiom::new(version, alpha_mode);
        let compact_options = if alpha_mode {
            vec![true, false]
        } else {
            vec![h_axiom.is_compact(flags, None)]
        };
        for is_compact in compact_options {
            let gap_config_started = timing.mark();
            let mut trial_possible_gaps = Vec::new();
            let rhythm_gap = V105HeaderGapAxiom::default().resolve_gap(
                version,
                None,
                flags,
                false,
                is_compact,
                has_checksum,
                Some(absolute_start_bit.unwrap_or(start_bit)),
            );
            trial_possible_gaps.push(rhythm_gap);
            if alpha_mode && gap_profile != AlphaScannerGapProfile::RhythmOnly {
                // Preserve the existing fallback sequence for the production scanner profile.
                let fallback_gaps: Vec<usize> = match gap_profile {
                    AlphaScannerGapProfile::Extended => {
                        vec![
                            0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 16, 24, 32, 40, 48, 50, 56,
                        ]
                    }
                    AlphaScannerGapProfile::WitnessSubset => {
                        vec![1, 6, 7, 8, 12, 13, 14, 16, 24, 32, 40, 48, 50, 56]
                    }
                    AlphaScannerGapProfile::ExtendedExceptMask(mask) => {
                        let removable = [0usize, 2, 3, 4, 5];
                        [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 16, 24, 32, 40, 48, 50, 56]
                            .into_iter()
                            .filter(|gap| {
                                removable
                                    .iter()
                                    .position(|candidate| candidate == gap)
                                    .is_none_or(|bit| mask & (1 << bit) == 0)
                            })
                            .collect()
                    }
                    AlphaScannerGapProfile::RhythmOnly => Vec::new(),
                };
                for g in fallback_gaps {
                    if !trial_possible_gaps.contains(&g) {
                        trial_possible_gaps.push(g);
                    }
                }
            }
            timing.record(ScannerPeekTimingStage::TrialConfig, gap_config_started);

            let geom_bits = if !alpha_mode { 4 } else { 0 };

            for gap in trial_possible_gaps {
                if timing.enabled {
                    timing.gap_attempts += 1;
                }
                let trial_total_skip = base_header_len as u32 + geom_bits + gap as u32;
                let gap_reader_started = timing.mark();
                let mut t_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                if t_reader.skip(start_bit as u32 + trial_total_skip).is_err() {
                    timing.record(ScannerPeekTimingStage::GapReader, gap_reader_started);
                    continue;
                }
                let mut t_cursor = BitCursor::new(t_reader);
                let absolute_trial_pos = start_bit + trial_total_skip as u64;
                t_cursor.set_pos(absolute_trial_pos);
                t_cursor.base_pos = absolute_trial_pos;
                timing.record(ScannerPeekTimingStage::GapReader, gap_reader_started);
                let mut t_code = String::new();

                // Trial 1: Huffman
                let code_decode_started = timing.mark();
                let huffman_pos = t_cursor.pos();
                for _ in 0..CodeTrialLayout::HUFFMAN_CODE_SYMBOLS {
                    if let Ok(ch) = huffman.decode_recorded(&mut t_cursor) {
                        t_code.push(ch);
                    } else {
                        break;
                    }
                }

                // Trial 2: ASCII (Alpha v105 specific)
                if alpha_mode {
                    t_cursor.rollback(huffman_pos);
                    let mut ascii_code = String::new();
                    let mut success = true;
                    for _ in 0..CodeTrialLayout::ASCII_CODE_BYTES {
                        match t_cursor.read_bits::<u8>(CodeTrialLayout::ASCII_BYTE_BITS) {
                            Ok(ch) => {
                                ascii_code.push(ch as char);
                            }
                            Err(_) => {
                                success = false;
                                break;
                            }
                        }
                    }
                    let trimmed_ascii = ascii_code.trim();
                    if success
                        && (CodeTrialLayout::ASCII_OVERRIDE_CODES.contains(&trimmed_ascii)
                            || is_v105_summary_code(&ascii_code))
                    {
                        t_code = ascii_code;
                    } else {
                        t_cursor.rollback(huffman_pos);
                    }
                }
                timing.record(ScannerPeekTimingStage::CodeDecode, code_decode_started);

                let plausibility_started = timing.mark();
                let trimmed_norm = crate::item::normalize_alpha_code_hint(t_code.trim());
                let trimmed = trimmed_norm.trim();
                if trimmed.len() >= 2 {
                    let mut confidence: i32 = 100;
                    let is_socketable_rune = (trimmed.starts_with('r') && trimmed.len() <= 3)
                        || (trimmed.starts_with('g') && trimmed.len() == 3)
                        || trimmed == "jew"
                        || trimmed == "ww"
                        || trimmed == "gcw";
                    let is_plausible = h_axiom.is_plausible(mode, loc, trimmed.as_bytes(), flags)
                        || (alpha_mode && is_socketable_rune && (mode == 6 || loc == 6));
                    timing.record(ScannerPeekTimingStage::Plausibility, plausibility_started);

                    if is_plausible {
                        let registry_template_started = timing.mark();
                        let reg = crate::domain::forensic::registry::get_registry();
                        let is_known = reg
                            .forced_compact_codes
                            .as_ref()
                            .map(|codes| codes.iter().any(|c| c == trimmed))
                            .unwrap_or(false)
                            || reg
                                .forced_runeword_codes
                                .as_ref()
                                .map(|codes| codes.iter().any(|c| c == trimmed))
                                .unwrap_or(false)
                            || item_template(trimmed).is_some()
                            || trimmed == "acww"
                            || trimmed == "bcww"
                            || trimmed == "xrs"
                            || trimmed == "mp1";

                        if is_known {
                            confidence += 400;
                        }
                        let mut matches_registry_gap = false;
                        if let Some(overrides) = &reg.item_overrides {
                            if let Some(item_map) = overrides.get(trimmed) {
                                if let Some(&g_val) = item_map.get("header_gap") {
                                    if g_val as usize == gap {
                                        matches_registry_gap = true;
                                    }
                                }
                            }
                        }

                        if matches_registry_gap {
                            confidence += 1000;
                        }

                        if alpha_mode && (trimmed == "hp1" || trimmed == "xrs") {
                            confidence += 500;
                            if trimmed == "xrs" && (flags & (1 << 26)) != 0 {
                                confidence += 1000;
                            }
                        }

                        let mut is_compact_override = None;
                        if let Some(overrides) = &reg.item_overrides {
                            if let Some(item_map) = overrides.get(trimmed) {
                                if let Some(&c_val) = item_map.get("is_compact") {
                                    is_compact_override = Some(c_val);
                                }
                            }
                        }

                        if let Some(c_val) = is_compact_override {
                            let expected_compact = c_val != 0;
                            if is_compact != expected_compact {
                                confidence -= 5000;
                            }
                        }
                        timing.record(
                            ScannerPeekTimingStage::RegistryTemplate,
                            registry_template_started,
                        );
                    }

                    let scoring_started = timing.mark();
                    if has_checksum {
                        // Slice 5: Increase checksum bonus to favor Alpha-path trials over Retail-path trials
                        confidence += 400;
                    }

                    // Slice 5: Tip the balance for authority shifts
                    if alpha_mode && start_bit % 8 == 2 {
                        confidence += 10;
                    }

                    // Rhythmic Bonus: Favor gaps that align body start with Bit 5 boundary (Slice 7)
                    if alpha_mode && (start_bit + trial_total_skip as u64) % 8 == 5 {
                        confidence += 150;
                    }

                    let is_compact_trial = is_compact;
                    let true_has_checksum = has_checksum
                        && (_checksum == _calculated
                            || (alpha_mode
                                && (_checksum == 0
                                    || (is_compact_trial
                                        && (trimmed.starts_with('r') && trimmed.len() <= 3
                                            || matches!(
                                                trimmed.trim(),
                                                "hp1"
                                                    | "mp1"
                                                    | "tsc"
                                                    | "isc"
                                                    | "jew"
                                                    | "ww"
                                                    | "gcw"
                                            )))
                                    || (ALPHA_LEGACY_AUTHORITY_CODES.contains(&trimmed.trim())
                                        && flags != 0
                                        && (version <= 2)))));

                    let geometry_resolution_started = timing.mark();
                    let true_geom_bits = if alpha_mode {
                        let summary_classification_started = timing.mark();
                        let is_summary = w_axiom.is_summary_item(version, trimmed);
                        timing.record(
                            ScannerPeekTimingStage::SummaryClassification,
                            summary_classification_started,
                        );
                        if is_summary {
                            3
                        } else {
                            timing.record_header_geometry_input(version, flags, mode, trimmed);
                            let cached_geometry = header_geometry_cache
                                .iter()
                                .find(
                                    |(
                                        cached_version,
                                        cached_flags,
                                        cached_mode,
                                        cached_code,
                                        _,
                                    )| {
                                        *cached_version == version
                                            && *cached_flags == flags
                                            && *cached_mode == mode
                                            && cached_code.as_str() == trimmed
                                    },
                                )
                                .map(|(_, _, _, _, geometry)| *geometry);
                            let geom = if let Some(geometry) = cached_geometry {
                                timing.record_header_geometry_cache_hit();
                                geometry
                            } else {
                                let header_geometry_started = timing.mark();
                                let geometry = h_axiom.header_geometry_with_mode(
                                    flags,
                                    Some(trimmed),
                                    Some(mode),
                                );
                                timing.record(
                                    ScannerPeekTimingStage::HeaderGeometry,
                                    header_geometry_started,
                                );
                                header_geometry_cache.push((
                                    version,
                                    flags,
                                    mode,
                                    trimmed.to_owned(),
                                    geometry,
                                ));
                                geometry
                            };
                            if geom.skip_geometry {
                                0
                            } else {
                                (geom.y_bits + geom.page_bits + geom.socket_hint_bits) as u32
                            }
                        }
                    } else {
                        4
                    };
                    timing.record(
                        ScannerPeekTimingStage::GeometryResolution,
                        geometry_resolution_started,
                    );
                    let true_base_header_len = 32
                        + version_bits
                        + (if true_has_checksum { 8 } else { 0 })
                        + mode_bits
                        + location_bits
                        + x_bits;
                    let corrected_gap = (trial_total_skip as i32
                        - true_base_header_len as i32
                        - true_geom_bits as i32)
                        .max(0) as i8;

                    if confidence > max_confidence
                        || (confidence == max_confidence && (is_compact_trial || true_has_checksum))
                    {
                        max_confidence = confidence;
                        best_res = Some((
                            (
                                mode,
                                loc,
                                _x_val,
                                trimmed.to_string(),
                                flags,
                                version,
                                is_compact_trial,
                                trial_total_skip as u64,
                                corrected_gap,
                                true_has_checksum,
                            ),
                            gap as u64,
                            gap == rhythm_gap,
                        ));
                    }
                    timing.record(ScannerPeekTimingStage::Scoring, scoring_started);
                }
            }
        }
    }
    best_res
}

pub fn peek_item_header_at_specific_gap(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    gap: u64,
) -> Option<(u8, u8, u8, String, u32, u8, bool, u64, i8, bool)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if reader.skip(start_bit as u32).is_err() {
        return None;
    }

    // Read header structure
    let flags = reader.read::<32, u32>().ok()?;

    let mut alpha_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    alpha_reader.skip(start_bit as u32 + 32).ok()?;
    let checksum = alpha_reader.read::<8, u8>().ok()?;
    let v = alpha_reader.read::<3, u8>().ok()?;
    let calculated = calculate_alpha_v105_checksum(flags, v);
    let w_axiom = V105PropertyWidthAxiom::default();
    let version_bits = w_axiom.version_bits(alpha_mode);
    let mode_bits = w_axiom.mode_bits(alpha_mode);
    let location_bits = w_axiom.location_bits(alpha_mode, v);
    let x_bits = w_axiom.x_bits(alpha_mode, v);

    let is_compact_flag = (flags & 0x00200000) != 0;

    // Alpha Forensic (Axiom 0365): Some summary items use 0 as a checksum sentinel, or have corrupted checksums when stacked.
    let (version, mode, loc, x_val, base_header_len, has_checksum) =
        if (v == 5 || v == 7 || v == 0 || v == 2)
            && (calculated == checksum
                || (alpha_mode && (checksum == 0 || is_compact_flag || v == 0 || v == 2)))
        {
            let m = alpha_reader.read::<3, u8>().ok()?;
            let l = match location_bits {
                4 => alpha_reader.read::<4, u8>().ok()?,
                _ => alpha_reader.read::<3, u8>().ok()?,
            };
            let x = match x_bits {
                3 => alpha_reader.read::<3, u8>().ok()?,
                _ => alpha_reader.read::<4, u8>().ok()?,
            };
            (
                v,
                m,
                l,
                x,
                32 + version_bits + 8 + mode_bits + location_bits + x_bits,
                true,
            )
        } else {
            let mut retail_reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
            retail_reader.skip(start_bit as u32 + 32).ok()?;
            let v = retail_reader.read::<3, u8>().ok()?;
            let m = retail_reader.read::<3, u8>().ok()?;
            let l = retail_reader.read::<3, u8>().ok()?;
            let x = retail_reader.read::<4, u8>().ok()?;
            (v, m, l, x, 32 + 3 + 3 + 3 + 4, false)
        };

    // Alpha saves can omit checksum bytes on some items; the peek path must still
    // use Alpha rules for compactness and code decoding so version 7 equipment is not
    // misclassified through the retail compact bit.
    let item_alpha_mode = alpha_mode;
    let is_compact_peek = HeaderAxiom::new(version, item_alpha_mode).is_compact(flags, None);
    let mut is_compact_detected = is_compact_peek;

    let mut n_reader = bitstream_io::BitReader::endian(
        std::io::Cursor::new(section_bytes),
        bitstream_io::LittleEndian,
    );
    if n_reader
        .skip(start_bit as u32 + base_header_len as u32 + gap as u32)
        .is_err()
    {
        return None;
    }
    let mut n_cursor = BitCursor::new(n_reader);
    let mut ok = true;
    let mut code_bytes = [0u8; 4];
    let mut code_len = 0;

    if item_alpha_mode {
        // Alpha Forensic (Slice 6): Try a 0-nudge trial first for potential summary items
        let mut summary_reader = bitstream_io::BitReader::endian(
            std::io::Cursor::new(section_bytes),
            bitstream_io::LittleEndian,
        );
        if summary_reader
            .skip(start_bit as u32 + base_header_len as u32)
            .is_ok()
        {
            let mut trial_bytes = [0u8; 3];
            let mut trial_ok = true;
            for i in 0..3 {
                if let Ok(ch) = summary_reader.read::<8, u8>() {
                    if ch != 0 {
                        trial_bytes[i] = ch;
                    }
                } else {
                    trial_ok = false;
                    break;
                }
            }
            let trial_code: String = trial_bytes.iter().map(|&b| b as char).collect();
            let trial_code =
                crate::item::normalize_alpha_code_hint(trial_code.trim_end_matches('\0'))
                    .to_string();
            if trial_ok && is_v105_summary_code(&trial_code) {
                code_bytes[..3].copy_from_slice(&trial_bytes);
                code_len = 3;
                is_compact_detected = true;
                let _ = n_cursor.read_bits_as_vec(24);
            }
        }

        if code_len == 0 {
            let mut trial_reader = bitstream_io::BitReader::endian(
                std::io::Cursor::new(section_bytes),
                bitstream_io::LittleEndian,
            );
            if trial_reader
                .skip(start_bit as u32 + base_header_len as u32 + gap as u32)
                .is_ok()
            {
                let mut trial_bytes = [0u8; 3];
                let mut trial_ok = true;
                for i in 0..3 {
                    if let Ok(ch) = trial_reader.read::<8, u8>() {
                        if ch != 0 {
                            trial_bytes[i] = ch;
                        }
                    } else {
                        trial_ok = false;
                        break;
                    }
                }
                let trial_code: String = trial_bytes.iter().map(|&b| b as char).collect();
                let trial_code =
                    crate::item::normalize_alpha_code_hint(trial_code.trim_end_matches('\0'))
                        .to_string();
                if trial_ok && is_v105_summary_code(&trial_code) {
                    code_bytes[..3].copy_from_slice(&trial_bytes);
                    code_len = 3;
                    is_compact_detected = true;
                    if gap > 0 {
                        let _ = n_cursor.read_bits_as_vec(gap as u32);
                    }
                    let _ = n_cursor.read_bits_as_vec(24);
                }
            }
        }
    }

    if code_len == 0 && item_alpha_mode {
        let mut stealth_reader = bitstream_io::BitReader::endian(
            std::io::Cursor::new(section_bytes),
            bitstream_io::LittleEndian,
        );
        if stealth_reader
            .skip(start_bit as u32 + base_header_len as u32 + gap as u32)
            .is_ok()
        {
            let mut bits = Vec::new();
            let mut ok = true;
            for _ in 0..24 {
                if let Ok(b) = stealth_reader.read_bit() {
                    bits.push(b);
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                if let Some(stealth) =
                    crate::domain::forensic::v105::axioms::V105StealthCodeAxiom::default()
                        .resolve_stealth_code(&bits)
                {
                    let stealth_bytes = stealth.as_bytes();
                    let len = stealth_bytes.len().min(4);
                    code_bytes[..len].copy_from_slice(&stealth_bytes[..len]);
                    code_len = len;
                    is_compact_detected = true;
                    let _ = n_cursor.read_bits_as_vec(24);
                }
            }
        }
    }

    if code_len == 0 {
        for i in 0..4 {
            match huffman.decode_recorded(&mut n_cursor) {
                Ok(ch) => {
                    code_bytes[i] = ch as u8;
                    code_len = i + 1;
                }
                Err(_) => {
                    if item_alpha_mode && i >= 1 {
                        let current_cursor_pos = n_cursor.pos();
                        let relative_pos = base_header_len as u64 + gap as u64 + current_cursor_pos;
                        if relative_pos == 69 && (code_bytes[0] == b'h' || code_bytes[0] == b'm') {
                            // Surgical 1-bit nudge for Opaque items at bit 69 (Axiom 0340)
                            if n_cursor.read_bit().is_ok() {
                                if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                                    code_bytes[i] = ch as u8;
                                    code_len = i + 1;
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
                            code_bytes[i] = ch as u8;
                            code_len = i + 1;
                            continue;
                        }
                    }
                    // Try 2-bit nudge
                    n_cursor.rollback(saved_pos);
                    if let Ok(bits) = n_cursor.read_bits_as_vec(2) {
                        if bits.len() == 2 {
                            if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) {
                                code_bytes[i] = ch as u8;
                                code_len = i + 1;
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

    let code: String = code_bytes[..code_len].iter().map(|&b| b as char).collect();
    let code = crate::item::normalize_alpha_code_hint(code.trim_end_matches('\0')).to_string();
    let code = match code.as_str() {
        "whp1" => "hp1".to_string(),
        _ => code,
    };
    let mut is_compact = HeaderAxiom::new(version, item_alpha_mode).is_compact(flags, Some(&code));
    if ok {
        if is_plausible_item_header(
            mode,
            loc,
            &code_bytes[..code_len],
            flags,
            version,
            item_alpha_mode,
        ) {
            let candidate = (
                mode,
                loc,
                x_val,
                code,
                flags,
                version,
                is_compact,
                (base_header_len as u64 + gap),
                gap as i8,
                has_checksum,
            );
            return Some(candidate);
        }
    }
    None
}

