use crate::data::generated::layout_policy::{
    ALPHA_ITEM_HEADER_KNOWN_CODE_MAX_VERSION, ALPHA_ITEM_HEADER_NONFORCED_RUNEWORD_EXCLUSIONS,
    ALPHA_ITEM_HEADER_PLAUSIBILITY_LIMITS, ALPHA_LEGACY_AUTHORITY_CODES,
    ALPHA_LEGACY_HEADER_PLAUSIBLE_CODE_EXCEPTIONS, LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS,
    LEGACY_ITEM_HEADER_FLAG_POLICY, LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT,
    LEGACY_ITEM_HEADER_PLAUSIBILITY_LIMITS, LegacyItemHeaderFlagPolicy,
    alpha_item_header_flag_policy, alpha_item_header_interpretation_policy,
};
use std::collections::HashSet;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy)]
pub struct HeaderGeometry {
    pub y_bits: u32,
    pub page_bits: u32,
    pub socket_hint_bits: u32,
    pub has_header_gap: bool,
    pub skip_geometry: bool,
    pub target_width: u32,
}

static VALID_CODES: LazyLock<HashSet<&'static [u8]>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for t in crate::data::item_codes::ITEM_TEMPLATES {
        let bytes = t.code.as_bytes();
        set.insert(bytes);
        let trimmed = t.code.trim().as_bytes();
        if trimmed != bytes {
            set.insert(trimmed);
        }
    }
    for &code in ALPHA_LEGACY_HEADER_PLAUSIBLE_CODE_EXCEPTIONS {
        set.insert(code);
    }
    set
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAxiom {
    pub version: u8,
    pub alpha_mode: bool,
}

impl HeaderAxiom {
    pub fn new(version: u8, alpha_mode: bool) -> Self {
        Self {
            version,
            alpha_mode,
        }
    }

    pub fn is_alpha(&self) -> bool {
        self.alpha_mode
            && crate::data::generated::layout_policy::ALPHA_ITEM_HEADER_ADMISSION_POLICY
                .header_versions
                .contains(&self.version)
    }

    pub fn is_plausible(&self, mode: u8, location: u8, code: &[u8], _flags: u32) -> bool {
        if code.is_empty() {
            return false;
        }

        // Zero-allocation plausible check: Registry lookup (O(1))
        if !VALID_CODES.contains(code) {
            // Forensic: Some Alpha v105 items use 3-char codes with trailing null/space.
            // Try trimmed version only if it's not a direct match.
            let trimmed =
                if code.len() > 0 && (code[code.len() - 1] == 0 || code[code.len() - 1] == b' ') {
                    let mut end = code.len();
                    while end > 0 && (code[end - 1] == 0 || code[end - 1] == b' ') {
                        end -= 1;
                    }
                    &code[..end]
                } else {
                    code
                };

            if !VALID_CODES.contains(trimmed) {
                // Slice 5: Support Alpha v105 stealth codes (potions/scrolls/etc.)
                if self.alpha_mode {
                    let s_str = String::from_utf8_lossy(trimmed);
                    if crate::domain::forensic::v105::axioms::is_v105_summary_code(&s_str) {
                        return mode <= ALPHA_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_mode
                            && location <= ALPHA_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_location;
                    }
                }
                return false;
            }
        }

        if self.alpha_mode {
            // The known-code path also applies its separate Alpha version cap.
            mode <= ALPHA_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_mode
                && location <= ALPHA_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_location
                && self.version <= ALPHA_ITEM_HEADER_KNOWN_CODE_MAX_VERSION
        } else {
            if mode > LEGACY_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_mode
                || location > LEGACY_ITEM_HEADER_PLAUSIBILITY_LIMITS.max_location
            {
                return false;
            }
            true
        }
    }
    pub fn is_compact(&self, flags: u32, code: Option<&str>) -> bool {
        if self.is_runeword(flags, code, false) {
            return false;
        }
        if self.alpha_mode {
            // The policy selects the initial flag; code-aware overrides remain authoritative.
            let policy = alpha_item_header_interpretation_policy(self.version);
            let mut is_compact = policy.compact_flag_enabled
                && (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.compact_mask) != 0;

            if let Some(c) = code {
                let trimmed = c.trim();

                let reg = crate::domain::forensic::registry::get_registry();
                if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed) {
                    is_compact = true;
                }
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_compact") {
                            is_compact = val != 0;
                        }
                        if let Some(&val) = map.get("is_authority_overlap") {
                            if val != 0 {
                                is_compact = false;
                            }
                        }
                    }
                }

                // Default to true for empty strings if no override was found
                if trimmed.is_empty() {
                    if let Some(overrides) = &reg.item_overrides {
                        if overrides.contains_key(trimmed) {
                            // Already handled by override above
                        } else {
                            return true;
                        }
                    } else {
                        return true;
                    }
                }
            }
            if policy.compact_fragment_filter {
                let is_fragment =
                    (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.fragment_state_mask) != 0;
                let mut fragment_override = false;
                if let Some(c) = code {
                    let trimmed = c.trim();
                    let reg = crate::domain::forensic::registry::get_registry();
                    if let Some(overrides) = &reg.item_overrides {
                        if let Some(map) = overrides.get(trimmed) {
                            if let Some(&val) = map.get("is_shadow") {
                                if val == 0 {
                                    fragment_override = true;
                                }
                            }
                        }
                    }
                }
                is_compact && (!is_fragment || fragment_override)
            } else {
                is_compact
            }
        } else {
            (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.compact_mask) != 0
        }
    }

    fn base_flag_policy(&self) -> &'static LegacyItemHeaderFlagPolicy {
        if self.alpha_mode {
            alpha_item_header_flag_policy(self.version)
        } else {
            &LEGACY_ITEM_HEADER_FLAG_POLICY
        }
    }

    pub fn is_identified(&self, flags: u32) -> bool {
        (flags & self.base_flag_policy().identified_mask) != 0
    }

    pub fn is_ethereal(&self, flags: u32) -> bool {
        (flags & self.base_flag_policy().ethereal_mask) != 0
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool, code: Option<&str>) -> bool {
        if self.alpha_mode && self.is_runeword(flags, code, false) {
            return true;
        }
        let policy = self.base_flag_policy();
        (!policy.socketed_requires_noncompact || !is_compact) && (flags & policy.socketed_mask) != 0
    }

    pub fn is_personalized(&self, flags: u32, is_compact: bool) -> bool {
        if is_compact {
            return false;
        }
        self.base_flag_policy()
            .personalized_mask
            .is_some_and(|mask| (flags & mask) != 0)
    }

    pub fn is_runeword(&self, flags: u32, code: Option<&str>, has_checksum: bool) -> bool {
        let trimmed = code.unwrap_or("").trim();

        let mut is_forced_rw = false;
        if self.alpha_mode {
            if ALPHA_LEGACY_AUTHORITY_CODES.contains(&trimmed) {
                is_forced_rw = true;
            }
            let reg = crate::domain::forensic::registry::get_registry();
            if let Some(codes) = &reg.forced_runeword_codes {
                if codes.iter().any(|rc| rc == trimmed) {
                    is_forced_rw = true;
                }
            }
        }

        if !is_forced_rw
            && (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.nonforced_runeword_blocking_mask)
                != 0
        {
            return false;
        }

        let has_runeword_flag =
            (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.runeword_mask) != 0;

        if self.alpha_mode {
            if is_forced_rw {
                return true;
            }
            if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed)
                || ALPHA_ITEM_HEADER_NONFORCED_RUNEWORD_EXCLUSIONS.contains(&trimmed)
            {
                return false;
            }

            // Preserve normal alpha base templates from the broad v5/v1 fallback.
            if crate::data::repository::legacy_item_template(trimmed).is_some() {
                return false;
            }

            // The checksum requirement and auxiliary fallback have separate policy roles.
            let policy = alpha_item_header_interpretation_policy(self.version);
            if has_runeword_flag && (has_checksum || policy.runeword_without_checksum) {
                return true;
            }

            if policy.runeword_auxiliary_fallback {
                let is_frag =
                    (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.runeword_fragment_mask) != 0;
                return !is_frag
                    && (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.runeword_auxiliary_mask)
                        != 0;
            }

            return false;
        }
        has_runeword_flag
    }

    pub fn is_v105_shadow(&self, flags: u32, code_hint: Option<&str>) -> bool {
        if self.is_compact(flags, code_hint) {
            return false;
        }
        let trimmed = code_hint.unwrap_or("").trim();
        if self.alpha_mode {
            let reg = crate::domain::forensic::registry::get_registry();
            if let Some(overrides) = &reg.item_overrides {
                if let Some(map) = overrides.get(trimmed) {
                    if let Some(&val) = map.get("is_shadow") {
                        return val != 0;
                    }
                }
            }
        }
        if self.is_runeword(flags, code_hint, false) {
            return false;
        }
        if self.alpha_mode
            && alpha_item_header_interpretation_policy(self.version).shadow_from_fragment
        {
            if (flags & LEGACY_ITEM_HEADER_CLASSIFICATION_MASKS.fragment_state_mask) != 0 {
                return true;
            }
        }
        false
    }

    pub fn header_geometry(&self, flags: u32, code_hint: Option<&str>) -> HeaderGeometry {
        self.header_geometry_with_mode(flags, code_hint, None)
    }

    pub fn header_geometry_with_mode(
        &self,
        flags: u32,
        code_hint: Option<&str>,
        mode: Option<u8>,
    ) -> HeaderGeometry {
        let is_compact = self.is_compact(flags, code_hint);
        let is_personalized = self.is_personalized(flags, is_compact);

        if self.alpha_mode {
            let is_rw = self.is_runeword(flags, code_hint, false);
            let is_v105_shadow = self.is_v105_shadow(flags, code_hint);
            let policy = alpha_item_header_interpretation_policy(self.version);
            let is_equipped = mode == Some(LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.equipped_mode);

            let is_summary = if let Some(c) = code_hint {
                crate::domain::forensic::v105::axioms::is_v105_summary_code(c)
            } else {
                // Forensic Peek: In Alpha v105 (v0, v1, v2, v4, v5, v6), compact items are almost always
                // summary items (potions, scrolls, etc.) which use 3-bit Y fields.
                is_compact && policy.summary_without_code_for_compact
            };

            let target_width = if is_summary {
                0
            } else if self.is_alpha() {
                if let Some(c) = code_hint {
                    crate::domain::forensic::v105::axioms::get_v105_target_width(
                        self.version,
                        c,
                        flags,
                        None,
                    )
                } else {
                    0
                }
            } else {
                LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.unadmitted_alpha_target_width_bits
            };

            if is_rw || is_v105_shadow || is_personalized || is_equipped {
                return HeaderGeometry {
                    y_bits: 0,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: true,
                    skip_geometry: true,
                    target_width: if is_equipped { 0 } else { target_width },
                };
            }

            if is_summary {
                return HeaderGeometry {
                    y_bits: LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.alpha_summary_y_bits,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: true,
                    skip_geometry: false,
                    target_width: 0,
                };
            }

            if is_compact {
                let has_header_gap = policy.compact_header_gap;
                return HeaderGeometry {
                    y_bits: 0,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap,
                    skip_geometry: true,
                    target_width,
                };
            }

            return HeaderGeometry {
                y_bits: LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.regular_y_bits,
                page_bits: LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.regular_page_bits,
                socket_hint_bits: policy.socket_hint_bits,
                has_header_gap: true,
                skip_geometry: false,
                target_width,
            };
        }

        // Retail / Fallback
        if is_compact {
            HeaderGeometry {
                y_bits: 0,
                page_bits: 0,
                socket_hint_bits: 0,
                has_header_gap: false,
                skip_geometry: true,
                target_width: 0,
            }
        } else {
            HeaderGeometry {
                y_bits: LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.regular_y_bits,
                page_bits: LEGACY_ITEM_HEADER_GEOMETRY_LAYOUT.regular_page_bits,
                socket_hint_bits: 0,
                has_header_gap: false,
                skip_geometry: false,
                target_width: 0,
            }
        }
    }
}
