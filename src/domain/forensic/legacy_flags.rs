use crate::domain::forensic::header_policy::HeaderAxiom;
use crate::domain::forensic::registry::{get_registry, AlphaForensics};
use crate::domain::header::legacy_flags::{
    LegacyFlagProvider, LegacyFlagRegistry, LegacyItemFlagContext,
};

struct ForensicLegacyFlagProvider;

impl LegacyFlagRegistry for AlphaForensics {
    fn compact_override(&self, code: &str) -> Option<bool> {
        self.item_overrides
            .as_ref()?
            .get(code)?
            .get("is_compact")
            .map(|&value| value != 0)
    }

    fn forced_compact(&self, code: &str) -> bool {
        self.forced_compact_codes
            .as_ref()
            .is_some_and(|codes| codes.iter().any(|entry| entry == code))
    }

    fn forced_runeword(&self, code: &str) -> bool {
        self.forced_runeword_codes
            .as_ref()
            .is_some_and(|codes| codes.iter().any(|entry| entry == code))
    }

    fn runeword_override(&self, code: &str) -> Option<bool> {
        self.item_overrides
            .as_ref()?
            .get(code)?
            .get("is_runeword")
            .map(|&value| value != 0)
    }
}

impl LegacyFlagProvider for ForensicLegacyFlagProvider {
    type Registry = AlphaForensics;

    fn registry(&self) -> &Self::Registry {
        get_registry()
    }

    fn is_summary_code(&self, code: &str) -> bool {
        crate::domain::forensic::v105::axioms::is_v105_summary_code(code)
    }

    fn is_shadow(&self, version: u8, alpha: bool, flags: u32, hint: Option<&str>) -> bool {
        HeaderAxiom::new(version, alpha).is_v105_shadow(flags, hint)
    }
}

impl<'a> LegacyItemFlagContext<'a> {
    pub(crate) fn with_code(self, code: &'a str) -> Self {
        self.with_code_using(code, &ForensicLegacyFlagProvider)
    }

    pub(crate) fn is_runeword(&self, flags: u32) -> bool {
        self.is_runeword_using(flags, &ForensicLegacyFlagProvider)
    }

    pub(crate) fn is_v105_shadow(&self, flags: u32, code_hint: Option<&str>) -> bool {
        self.is_v105_shadow_using(flags, code_hint, &ForensicLegacyFlagProvider)
    }
}
