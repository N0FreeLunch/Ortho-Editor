//! Legacy item framing with caller state and lazy forensic queries kept separate.
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
