use crate::data::generated::layout_policy::LEGACY_ITEM_FLAG_POLICY as FLAG_POLICY;
use crate::domain::item::quality::ItemQuality;

/// Queries exposed at the compatibility algorithm's original registry stages.
pub(crate) trait LegacyFlagRegistry {
    fn compact_override(&self, code: &str) -> Option<bool>;
    fn forced_compact(&self, code: &str) -> bool;
    fn forced_runeword(&self, code: &str) -> bool;
    fn runeword_override(&self, code: &str) -> Option<bool>;
}

/// Lazy dependencies; callers preserve their admission and evaluation order.
pub(crate) trait LegacyFlagProvider {
    type Registry: LegacyFlagRegistry;

    fn registry(&self) -> &Self::Registry;
    fn is_summary_code(&self, code: &str) -> bool;
    fn is_shadow(&self, version: u8, alpha: bool, flags: u32, hint: Option<&str>) -> bool;
}

/// Borrowed compatibility flag state formerly hosted by StatsAxiom.
/// Its runeword contract remains distinct from HeaderAxiom admission.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LegacyItemFlagContext<'a> {
    version: u8,
    quality: ItemQuality,
    save_is_alpha: bool,
    is_compact: bool,
    code: &'a str,
}

impl<'a> LegacyItemFlagContext<'a> {
    pub(crate) fn new(
        version: u8,
        quality: ItemQuality,
        save_is_alpha: bool,
        is_compact: bool,
        code: &'a str,
    ) -> Self {
        Self {
            version,
            quality,
            save_is_alpha,
            is_compact,
            code,
        }
    }

    pub(crate) fn is_compact(&self) -> bool {
        self.is_compact
    }

    pub(crate) fn with_code_using<P: LegacyFlagProvider>(
        mut self,
        code: &'a str,
        provider: &P,
    ) -> Self {
        self.code = code;
        let trimmed = self.code.trim();
        if self.save_is_alpha {
            let reg = provider.registry();
            let mut is_compact = self.is_compact || provider.is_summary_code(code);

            if trimmed.is_empty()
                && FLAG_POLICY
                    .empty_code_compact_versions
                    .contains(&self.version)
            {
                is_compact = true;
            } else {
                if let Some(value) = reg.compact_override(trimmed) {
                    is_compact = value;
                    self.is_compact = is_compact;
                    return self;
                }

                if reg.forced_compact(trimmed) {
                    is_compact = true;
                }

                if FLAG_POLICY.low_quality_compact_codes.contains(&trimmed)
                    && (self.quality <= crate::domain::item::ItemQuality::Normal
                        || self.quality == crate::domain::item::ItemQuality::Low)
                {
                    is_compact = true;
                }
            }

            self.is_compact = is_compact;
        }
        self
    }

    pub(crate) fn is_runeword_using<P: LegacyFlagProvider>(
        &self,
        flags: u32,
        provider: &P,
    ) -> bool {
        let trimmed = self.code.trim();
        if FLAG_POLICY.unconditional_runeword_codes.contains(&trimmed) {
            return true;
        }
        if (flags & FLAG_POLICY.runeword_mask) != 0 {
            return true;
        }
        if self.save_is_alpha {
            if FLAG_POLICY
                .auxiliary_runeword_versions
                .contains(&self.version)
            {
                let is_frag = (flags & FLAG_POLICY.runeword_fragment_mask) != 0;
                if !is_frag && (flags & FLAG_POLICY.runeword_auxiliary_mask) != 0 {
                    return true;
                }
            }
            let reg = provider.registry();
            if reg.forced_runeword(trimmed) {
                return true;
            }
            if let Some(value) = reg.runeword_override(trimmed) {
                return value;
            }
        }
        false
    }

    pub(crate) fn is_v105_shadow_using<P: LegacyFlagProvider>(
        &self,
        flags: u32,
        code_hint: Option<&str>,
        provider: &P,
    ) -> bool {
        if self.is_compact {
            return false;
        }
        provider.is_shadow(self.version, self.save_is_alpha, flags, code_hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::header::HeaderAxiom;

    #[d2r_macros::hypothesis_test]
    fn compatibility_runeword_is_distinct_from_header_admission() {
        // Preserve the observed compatibility distinction without promoting format truth.
        let context = LegacyItemFlagContext::new(0, ItemQuality::Normal, false, false, "c8xr");
        assert!(context.is_runeword(0));
        assert!(!HeaderAxiom::new(0, false).is_runeword(0, Some("c8xr"), false));
    }

    struct Probe {
        events: std::cell::RefCell<Vec<String>>,
        compact_override: Option<bool>,
        force_runeword: bool,
    }

    impl Probe {
        fn new(compact_override: Option<bool>, force_runeword: bool) -> Self {
            Self {
                events: Default::default(),
                compact_override,
                force_runeword,
            }
        }

        fn record(&self, event: String) {
            self.events.borrow_mut().push(event);
        }
        fn take(&self) -> Vec<String> {
            self.events.take()
        }
    }

    impl LegacyFlagRegistry for Probe {
        fn compact_override(&self, code: &str) -> Option<bool> {
            self.record(format!("compact:{code}"));
            self.compact_override
        }
        fn forced_compact(&self, code: &str) -> bool {
            self.record(format!("forced-compact:{code}"));
            false
        }
        fn forced_runeword(&self, code: &str) -> bool {
            self.record(format!("forced-runeword:{code}"));
            self.force_runeword
        }
        fn runeword_override(&self, code: &str) -> Option<bool> {
            self.record(format!("runeword:{code}"));
            Some(false)
        }
    }

    impl LegacyFlagProvider for Probe {
        type Registry = Self;
        fn registry(&self) -> &Self {
            self.record("registry".to_string());
            self
        }
        fn is_summary_code(&self, code: &str) -> bool {
            self.record(format!("summary:{code}"));
            false
        }
        fn is_shadow(&self, version: u8, alpha: bool, flags: u32, hint: Option<&str>) -> bool {
            self.record(format!("shadow:{version}:{alpha}:{flags}:{hint:?}"));
            true
        }
    }

    #[d2r_macros::hypothesis_test]
    fn provider_compact_effect_order_preserves_observed_guards() {
        let probe = Probe::new(None, false);
        let context = LegacyItemFlagContext::new(5, ItemQuality::Normal, false, false, "");
        assert!(!context.with_code_using("cm1", &probe).is_compact());
        assert!(probe.take().is_empty());
        let context = LegacyItemFlagContext::new(5, ItemQuality::Normal, true, false, "");
        assert!(context.with_code_using("  cm1  ", &probe).is_compact());
        assert_eq!(
            probe.take(),
            [
                "registry",
                "summary:  cm1  ",
                "compact:cm1",
                "forced-compact:cm1"
            ]
        );
        assert!(context.with_code_using(" ", &probe).is_compact());
        assert_eq!(probe.take(), ["registry", "summary: "]);
        let compact = LegacyItemFlagContext::new(5, ItemQuality::Normal, true, true, "");
        assert!(compact.with_code_using("other", &probe).is_compact());
        assert_eq!(
            probe.take(),
            ["registry", "compact:other", "forced-compact:other"]
        );
        let override_probe = Probe::new(Some(false), false);
        assert!(!context.with_code_using("cm1", &override_probe).is_compact());
        assert_eq!(
            override_probe.take(),
            ["registry", "summary:cm1", "compact:cm1"]
        );
    }

    #[d2r_macros::hypothesis_test]
    fn provider_runeword_and_shadow_effect_order_preserves_observed_guards() {
        let probe = Probe::new(None, false);
        for (alpha, code, flags, expected) in [
            (false, "other", 0, false),
            (true, " c8xr ", 0, true),
            (true, "other", 1 << 26, true),
            (true, "other", 1 << 11, true),
        ] {
            let context = LegacyItemFlagContext::new(5, ItemQuality::Normal, alpha, false, code);
            assert_eq!(context.is_runeword_using(flags, &probe), expected);
            assert!(probe.take().is_empty());
        }
        let context = LegacyItemFlagContext::new(5, ItemQuality::Normal, true, false, " other ");
        assert!(!context.is_runeword_using((1 << 11) | (1 << 27), &probe));
        assert_eq!(
            probe.take(),
            ["registry", "forced-runeword:other", "runeword:other"]
        );
        let forced = Probe::new(None, true);
        assert!(context.is_runeword_using(0, &forced));
        assert_eq!(forced.take(), ["registry", "forced-runeword:other"]);
        let compact = LegacyItemFlagContext::new(7, ItemQuality::Normal, true, true, "other");
        assert!(!compact.is_v105_shadow_using(9, Some(" hint "), &probe));
        assert!(probe.take().is_empty());
        assert!(context.is_v105_shadow_using(9, Some(" hint "), &probe));
        assert_eq!(probe.take(), ["shadow:5:true:9:Some(\" hint \")"]);
    }
}
