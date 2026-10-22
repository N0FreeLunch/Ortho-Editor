use crate::data::generated::layout_policy::LEGACY_ITEM_FLAG_POLICY as FLAG_POLICY;
use crate::domain::forensic::header_policy::HeaderAxiom;
use crate::domain::forensic::registry::get_registry;
use crate::domain::item::quality::ItemQuality;

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

    pub(crate) fn with_code(mut self, code: &'a str) -> Self {
        self.code = code;
        let trimmed = self.code.trim();
        if self.save_is_alpha {
            let reg = get_registry();
            let mut is_compact = self.is_compact
                || crate::domain::forensic::v105::axioms::is_v105_summary_code(code);

            if trimmed.is_empty()
                && FLAG_POLICY
                    .empty_code_compact_versions
                    .contains(&self.version)
            {
                is_compact = true;
            } else {
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_compact") {
                            is_compact = val != 0;
                            self.is_compact = is_compact;
                            return self;
                        }
                    }
                }

                if let Some(codes) = &reg.forced_compact_codes {
                    if codes.iter().any(|c| c == trimmed) {
                        is_compact = true;
                    }
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

    pub(crate) fn is_runeword(&self, flags: u32) -> bool {
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
            let reg = get_registry();
            if let Some(codes) = &reg.forced_runeword_codes {
                if codes.iter().any(|c| c == trimmed) {
                    return true;
                }
            }
            if let Some(overrides) = &reg.item_overrides {
                if let Some(map) = overrides.get(trimmed) {
                    if let Some(&val) = map.get("is_runeword") {
                        return val != 0;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn is_v105_shadow(&self, flags: u32, code_hint: Option<&str>) -> bool {
        if self.is_compact {
            return false;
        }
        HeaderAxiom::new(self.version, self.save_is_alpha).is_v105_shadow(flags, code_hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[d2r_macros::hypothesis_test]
    fn compatibility_runeword_is_distinct_from_header_admission() {
        // Preserve the observed compatibility distinction without promoting format truth.
        let context = LegacyItemFlagContext::new(0, ItemQuality::Normal, false, false, "c8xr");
        assert!(context.is_runeword(0));
        assert!(!HeaderAxiom::new(0, false).is_runeword(0, Some("c8xr"), false));
    }
}
