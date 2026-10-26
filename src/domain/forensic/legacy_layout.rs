//! Legacy item framing with caller state and lazy forensic queries kept separate.
use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::v105::axioms::V105PropertyNudgeAxiom;
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};
use crate::item::ParsingResult;
use bitstream_io::BitRead;
use crate::data::generated::layout_policy::{
    ALPHA_ITEM_HEADER_ADMISSION_POLICY, LEGACY_STATS_HEADER_GAP_POLICY,
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
}
