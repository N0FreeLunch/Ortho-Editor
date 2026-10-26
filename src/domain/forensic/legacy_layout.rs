//! Legacy item framing with caller state and lazy forensic queries kept separate.
use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::v105::axioms::V105PropertyNudgeAxiom;
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};
use crate::item::ParsingResult;
use crate::error::ParsingError;
use crate::domain::item::subdomains::gap::{AlphaHeaderGap, GapCombinator};
use bitstream_io::BitRead;
use crate::data::generated::layout_policy::{
    ALPHA_ITEM_HEADER_ADMISSION_POLICY, LEGACY_PROPERTY_BODY_GAP_READ_POLICY,
    LEGACY_STATS_HEADER_GAP_POLICY,
};

trait LegacyItemLayoutProvider {
    fn header_gap(&self, code: &str) -> Option<u32>;
    fn target_width(&self, version: u8, code: &str, flags: u32, idx: Option<usize>) -> u32;
}

struct RegistryLayoutProvider;

impl LegacyItemLayoutProvider for RegistryLayoutProvider {
    fn header_gap(&self, code: &str) -> Option<u32> {
        crate::domain::forensic::registry::get_registry()
            .item_overrides
            .as_ref()
            .and_then(|overrides| overrides.get(code))
            .and_then(|values| values.get("header_gap"))
            .copied()
    }

    fn target_width(&self, version: u8, code: &str, flags: u32, idx: Option<usize>) -> u32 {
        crate::domain::forensic::v105::axioms::get_v105_target_width(version, code, flags, idx)
    }
}

pub(crate) struct LegacyItemLayoutContext {
    version: u8,
    save_is_alpha: bool,
    is_compact: bool,
    idx: usize,
}

impl LegacyItemLayoutContext {
    pub(crate) fn new(version: u8, save_is_alpha: bool, is_compact: bool, idx: usize) -> Self {
        Self {
            version,
            save_is_alpha,
            is_compact,
            idx,
        }
    }

    pub(crate) fn header_gap(&self, code: &str) -> u32 {
        self.header_gap_using(code, &RegistryLayoutProvider)
    }

    fn header_gap_using<P: LegacyItemLayoutProvider>(&self, code: &str, provider: &P) -> u32 {
        let policy = &LEGACY_STATS_HEADER_GAP_POLICY;
        if !self.save_is_alpha
            || !ALPHA_ITEM_HEADER_ADMISSION_POLICY
                .stats_versions
                .contains(&self.version)
        {
            return policy.default_gap_bits;
        }
        let trimmed = code.trim();
        if trimmed == policy.forced_code {
            return policy.forced_gap_bits;
        }
        provider
            .header_gap(trimmed)
            .unwrap_or(policy.default_gap_bits)
    }

    fn body_gap_length(&self, header_version: u8, code: &str) -> u32 {
        self.body_gap_length_using(header_version, code, &RegistryLayoutProvider)
    }

    fn body_gap_length_using<P: LegacyItemLayoutProvider>(
        &self,
        header_version: u8,
        code: &str,
        provider: &P,
    ) -> u32 {
        let policy = &LEGACY_PROPERTY_BODY_GAP_READ_POLICY;
        if policy.suppressed_codes.contains(&code.trim())
            || policy.suppressed_header_versions.contains(&header_version)
        {
            return policy.suppressed_gap_bits;
        }
        self.header_gap_using(code, provider)
    }

    pub(crate) fn calculate_alignment(&self, current_len: u64, code: &str, flags: u32) -> u64 {
        self.calculate_alignment_using(current_len, code, flags, &RegistryLayoutProvider)
    }

    fn calculate_alignment_using<P: LegacyItemLayoutProvider>(
        &self,
        current_len: u64,
        code: &str,
        flags: u32,
        provider: &P,
    ) -> u64 {
        if self.save_is_alpha {
            let target_width =
                provider.target_width(self.version, code, flags, Some(self.idx)) as u64;
            if target_width > 0 {
                return target_width;
            }
        }
        if !self.is_compact {
            let aligned = (current_len + 7) / 8 * 8;
            return aligned;
        }
        current_len
    }
}

/// Read the legacy pre-property gap with independent header and Stats versions.
/// Keep the original partial-read, context, storage and unwind ordering.
pub(crate) fn read_legacy_property_body_gap<R: BitRead>(
    cursor: &mut BitCursor<R>,
    header_version: u8,
    code: &str,
    layout: &LegacyItemLayoutContext,
    stored_bits: &mut Vec<bool>,
) -> ParsingResult<()> {
    let gap_len = layout.body_gap_length(header_version, code);
    if gap_len > 0 {
        cursor.push_context("AlphaBodyGap");
        let gap_seg = AlphaHeaderGap::parse(cursor, gap_len as usize).map_err(|e| {
            cursor.fail(ParsingError::UnexpectedValue {
                field: "AlphaBodyGap".to_string(),
                value: "".to_string(),
                reason: format!("Failed to parse body gap: {}", e),
            })
        })?;
        stored_bits.extend(gap_seg.bits);
        cursor.pop_context();
    }
    Ok(())
}

pub(crate) fn apply_property_residue_nudge<R: BitRead>(
    cursor: &mut BitCursor<R>,
    version: u8,
    rhythm_recovery: bool,
    is_compact: bool,
    is_runeword: bool,
    audit: &mut ForensicAudit,
) -> ParsingResult<()> {
    let p_nudge = V105PropertyNudgeAxiom::default().get_nudge(version) as usize;

    // forensic-1363: In Alpha v105, version 1 runewords (xrs) do NOT use the 2-bit property residue nudge
    // if they are correctly aligned via resolve_gap. Standard runewords also skip this.
    if p_nudge > 0 && !is_compact && !rhythm_recovery && !is_runeword {
        let saved_pos = cursor.pos();
        cursor.push_context("AlphaPropertyResidueNudge");
        if cursor.read_bits::<u32>(p_nudge as u32).is_ok() {
            audit.record(V105PropertyNudgeAxiom::default().metadata());
        } else {
            cursor.rollback(saved_pos);
        }
        cursor.pop_context();
    }
    Ok(())
}

pub(crate) fn apply_alignment_padding<R: BitRead>(
    cursor: &mut BitCursor<R>,
    start_bit: u64,
    code: &str,
    flags: u32,
    layout: &LegacyItemLayoutContext,
) -> ParsingResult<Vec<bool>> {
    let consumed_bits = cursor.pos() - start_bit;
    let final_consumed = layout.calculate_alignment(consumed_bits, code, flags);

    if final_consumed > consumed_bits {
        let padding_count = (final_consumed - consumed_bits) as u32;
        let saved_base_pos = cursor.base_pos;
        cursor.base_pos = start_bit;
        let padding_result = cursor.with_context("AlphaAlignmentPadding", |c| {
            let mut bits = Vec::new();
            for _ in 0..padding_count {
                match c.read_bit() {
                    Ok(bit) => bits.push(bit),
                    Err(_) => break,
                }
            }
            Ok(bits)
        });
        cursor.base_pos = saved_base_pos;
        let padding = padding_result?;
        return Ok(padding);
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::hypothesis_test;
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    enum Query {
        Gap(String),
        Target(u8, String, u32, Option<usize>),
    }

    struct Provider {
        gap: Option<u32>,
        target: u32,
        queries: RefCell<Vec<Query>>,
    }

    impl Provider {
        fn new(gap: Option<u32>, target: u32) -> Self {
            Self {
                gap,
                target,
                queries: RefCell::new(Vec::new()),
            }
        }
    }

    impl LegacyItemLayoutProvider for Provider {
        fn header_gap(&self, code: &str) -> Option<u32> {
            self.queries.borrow_mut().push(Query::Gap(code.to_string()));
            self.gap
        }
        fn target_width(&self, version: u8, code: &str, flags: u32, idx: Option<usize>) -> u32 {
            self.queries
                .borrow_mut()
                .push(Query::Target(version, code.to_string(), flags, idx));
            self.target
        }
    }

    #[hypothesis_test]
    fn gap_admission_precedence_normalization_and_query_effects_remain_distinct() {
        for (version, alpha, code, override_gap, expected, query) in [
            (0, false, "probe_gap", Some(13), 0, None),
            (3, true, "ks d", Some(99), 0, None),
            (7, true, "ks d", Some(99), 10, None),
            (255, true, "probe_gap", Some(13), 0, None),
            (5, true, "\u{2003}ks d\u{2003}", Some(99), 10, None),
            (5, true, "ks d\0", Some(99), 99, Some("ks d\0")),
            (5, true, " probe_zero ", Some(0), 0, Some("probe_zero")),
            (5, true, " unknown ", None, 0, Some("unknown")),
            (5, true, "probe_gap", Some(13), 13, Some("probe_gap")),
        ] {
            let provider = Provider::new(override_gap, 80);
            let context = LegacyItemLayoutContext::new(version, alpha, true, 9);
            assert!(provider.queries.borrow().is_empty());
            assert_eq!(context.header_gap_using(code, &provider), expected);
            let expected_queries = query
                .map(|q| vec![Query::Gap(q.to_string())])
                .unwrap_or_default();
            assert_eq!(*provider.queries.borrow(), expected_queries);
        }
    }

    #[hypothesis_test]
    fn alignment_uses_raw_alpha_and_original_target_arguments_before_fallback() {
        for (version, alpha, compact, idx, current, target, expected) in [
            (3, true, false, 4, 0, 80, 80),
            (7, true, true, 1, u64::MAX, 88, 88),
            (96, true, false, 8, 71, 0, 72),
            (5, false, false, 9, 71, 80, 72),
            (5, false, true, 9, 71, 80, 71),
            (3, true, true, 3, u64::MAX, 0, u64::MAX),
        ] {
            let provider = Provider::new(Some(999), target);
            let context = LegacyItemLayoutContext::new(version, alpha, compact, idx);
            assert_eq!(
                context.calculate_alignment_using(current, " \0explicit ", 0x80000000, &provider),
                expected
            );
            let queries = if alpha {
                vec![Query::Target(
                    version,
                    " \0explicit ".into(),
                    0x80000000,
                    Some(idx),
                )]
            } else {
                vec![]
            };
            assert_eq!(*provider.queries.borrow(), queries);
        }
    }

    #[hypothesis_test]
    fn ordinary_alignment_retains_original_overflow_mode() {
        let provider = Provider::new(None, 80);
        let context = LegacyItemLayoutContext::new(3, false, false, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.calculate_alignment_using(u64::MAX, "unknown", 0, &provider)
        }));
        if cfg!(debug_assertions) {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), 0);
        }
        assert!(provider.queries.borrow().is_empty());
    }

    #[hypothesis_test]
    fn body_gap_selector_preserves_header_stats_domains_and_lazy_queries() {
        for (headers, stats) in [
            ((0..=255).collect::<Vec<u8>>(), vec![1, 3, 5, 7, 96]),
            (vec![0, 1, 5], (0..=255).collect::<Vec<u8>>()),
        ] {
            for header in &headers {
                for version in &stats {
                    for raw in [false, true] {
                        for code in [
                            "buc",
                            "\u{2003}buc\u{2003}",
                            "buc\0",
                            "BUC",
                            "ks d",
                            "probe_gap",
                            "unknown",
                            "",
                        ] {
                            let actual_provider = Provider::new(Some(13), 80);
                            let expected_provider = Provider::new(Some(13), 80);
                            let context = LegacyItemLayoutContext::new(*version, raw, true, 9);
                            let expected = if code.trim() == "buc" || *header == 1 {
                                0
                            } else {
                                context.header_gap_using(code, &expected_provider)
                            };
                            assert_eq!(
                                context.body_gap_length_using(*header, code, &actual_provider),
                                expected
                            );
                            assert_eq!(
                                *actual_provider.queries.borrow(),
                                *expected_provider.queries.borrow()
                            );
                        }
                    }
                }
            }
        }
    }

    struct GapFaultReader {
        bytes: std::io::Cursor<Vec<u8>>,
        calls: std::rc::Rc<std::cell::Cell<usize>>,
        armed: std::rc::Rc<std::cell::Cell<usize>>,
        fault: u8,
    }
    impl std::io::Read for GapFaultReader {
        fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            if call == self.armed.get() {
                if self.fault == 1 {
                    return Err(std::io::Error::other("controlled body gap error"));
                }
                if self.fault == 2 {
                    panic!("controlled body gap panic");
                }
            }
            std::io::Read::read(&mut self.bytes, target)
        }
    }
    type GapCursor = BitCursor<bitstream_io::BitReader<GapFaultReader, bitstream_io::LittleEndian>>;
    fn gap_cursor(
        bytes: &[u8],
        prefix: u32,
        limit: Option<u64>,
        fault: u8,
    ) -> (GapCursor, std::rc::Rc<std::cell::Cell<usize>>) {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let armed = std::rc::Rc::new(std::cell::Cell::new(usize::MAX));
        let reader = GapFaultReader {
            bytes: std::io::Cursor::new(bytes.to_vec()),
            calls: calls.clone(),
            armed: armed.clone(),
            fault,
        };
        let mut cursor = BitCursor::new(bitstream_io::BitReader::endian(
            reader,
            bitstream_io::LittleEndian,
        ));
        cursor.set_trace(true);
        cursor.push_context("outer body gap owner");
        cursor.read_bits_as_vec_up_to(prefix);
        cursor.set_pos(u64::from(prefix));
        cursor.base_pos = u64::from(prefix.min(2));
        if let Some(limit) = limit {
            cursor.set_limit(limit);
        }
        armed.set(calls.get() + 1);
        (cursor, calls)
    }
    fn gap_state(cursor: &GapCursor) -> String {
        format!(
            "{:?}",
            (
                cursor.pos(),
                cursor.limit(),
                cursor.remaining(),
                cursor.base_pos,
                cursor.recorded_bits(),
                cursor.segments(),
                cursor.active_segments(),
                cursor.context_stack(),
                cursor.operation_events(),
                cursor.alpha_quality
            )
        )
    }
    fn gap_outcome<T: std::fmt::Debug>(f: impl FnOnce() -> T) -> String {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(value) => format!("result:{value:?}"),
            Err(payload) => format!(
                "panic:{:?}",
                payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
            ),
        }
    }
    // Frozen pre-extraction stage; the generic public delegate is source-equivalent
    // to the original private layout construction and remains unchanged by this slice.
    fn original_body_gap(
        cursor: &mut GapCursor,
        header: u8,
        code: &str,
        version: u8,
        raw: bool,
        stored: &mut Vec<bool>,
    ) -> ParsingResult<()> {
        let mut axiom = crate::domain::stats::StatsAxiom::new(
            version,
            crate::domain::item::ItemQuality::Normal,
            raw,
        );
        axiom.is_compact = true;
        axiom.idx = 9;
        let gap_len = if code.trim() == "buc" || header == 1 {
            0
        } else {
            axiom.header_gap(code, 0)
        };
        if gap_len > 0 {
            cursor.push_context("AlphaBodyGap");
            let gap_seg = AlphaHeaderGap::parse(cursor, gap_len as usize).map_err(|e| {
                cursor.fail(ParsingError::UnexpectedValue {
                    field: "AlphaBodyGap".to_string(),
                    value: "".to_string(),
                    reason: format!("Failed to parse body gap: {}", e),
                })
            })?;
            stored.extend(gap_seg.bits);
            cursor.pop_context();
        }
        Ok(())
    }
    #[hypothesis_test]
    fn body_gap_execution_matches_original_stage_with_io_faults() {
        use std::io::Write;
        let mut output = std::env::var_os("D2R_BODY_GAP_OBSERVATIONS").map(|path| {
            std::io::BufWriter::new(
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(path)
                    .expect("new body gap output"),
            )
        });
        let mut cases = 0;
        let mut panics = 0;
        let mut appended = 0;
        let mut operation_errors = 0;
        for header in [0, 1, 5, 96] {
            for version in [3, 5, 7] {
                for raw in [false, true] {
                    for code in [
                        "buc",
                        "\u{2003}buc\u{2003}",
                        "buc\0",
                        "ks d",
                        "probe_gap",
                        "unknown",
                    ] {
                        for prefix in [0u32, 6] {
                            for limit in
                                [None, Some(u64::from(prefix)), Some(u64::from(prefix + 1))]
                            {
                                for bytes in [vec![], vec![0x96, 0x69, 0xac]] {
                                    for initial in [vec![], vec![true, false, true]] {
                                        for fault in [0, 1, 2] {
                                            let (mut old, old_calls) =
                                                gap_cursor(&bytes, prefix, limit, fault);
                                            let (mut actual, actual_calls) =
                                                gap_cursor(&bytes, prefix, limit, fault);
                                            let mut expected_bits = initial.clone();
                                            let mut actual_bits = initial.clone();
                                            let context =
                                                LegacyItemLayoutContext::new(version, raw, true, 9);
                                            let before = gap_state(&old);
                                            let old_initial_calls = old_calls.get();
                                            let expected = gap_outcome(|| {
                                                original_body_gap(
                                                    &mut old,
                                                    header,
                                                    code,
                                                    version,
                                                    raw,
                                                    &mut expected_bits,
                                                )
                                            });
                                            let result = gap_outcome(|| {
                                                read_legacy_property_body_gap(
                                                    &mut actual,
                                                    header,
                                                    code,
                                                    &context,
                                                    &mut actual_bits,
                                                )
                                            });
                                            assert_eq!(result, expected);
                                            assert_eq!(actual_bits, expected_bits);
                                            assert_eq!(gap_state(&actual), gap_state(&old));
                                            assert_eq!(actual_calls.get(), old_calls.get());
                                            if result.starts_with("panic:") {
                                                panics += 1;
                                            }
                                            if actual_bits.len() > initial.len() {
                                                appended += 1;
                                            }
                                            if fault == 1 && old_calls.get() > old_initial_calls {
                                                operation_errors += 1;
                                            }
                                            let after = gap_state(&actual);
                                            let calls_after = actual_calls.get();
                                            let old_follow = gap_outcome(|| old.read_bits::<u8>(3));
                                            let follow = gap_outcome(|| actual.read_bits::<u8>(3));
                                            assert_eq!(follow, old_follow);
                                            assert_eq!(gap_state(&actual), gap_state(&old));
                                            assert_eq!(actual_calls.get(), old_calls.get());
                                            if let Some(file) = output.as_mut() {
                                                serde_json::to_writer(&mut *file,&serde_json::json!({"kind":"case","header":header,"stats_version":version,"raw":raw,"code":code,"prefix":prefix,"limit":limit,"bytes":bytes,"initial":initial,"fault":fault,"before":before,"result":result,"stored":actual_bits,"after":after,"calls_after":calls_after,"follow":follow,"follow_state":gap_state(&actual),"final_calls":actual_calls.get()})).unwrap();
                                                writeln!(file).unwrap();
                                            }
                                            cases += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 10368);
        assert!(panics > 0 && appended > 0 && operation_errors > 0);
        if let Some(file) = output.as_mut() {
            serde_json::to_writer(&mut *file,&serde_json::json!({"kind":"summary","schema":1,"cases":cases,"operation_panics":panics,"appended":appended,"operation_io_errors":operation_errors})).unwrap();
            writeln!(file).unwrap();
            file.flush().unwrap();
        }
    }
}
