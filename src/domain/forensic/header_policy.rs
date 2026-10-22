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
    // Axiom 0365: Explicit Alpha/Legacy white-listed codes (Forensic markers & Quest items)
    set.insert(b"wuw8");
    set.insert(b"\xCF\x4F"); // Legacy marker: `ÏO`.
    set.insert(b"\x48\x04"); // H\x04
    set.insert(b"acww"); // Shadow Item placeholder
    set.insert(b"bwcw"); // Tome
    set.insert(b"isc"); // Identify Scroll
    set.insert(b"tsc"); // Town Portal Scroll
    set.insert(b"us g"); // Javelin Alpha code
    set.insert(b"k g"); // Javelin Alpha code (alternate)
    set.insert(b"b7ts"); // Alpha v105 Battle Shield
    set.insert(b"ks d"); // Alpha v105 forensic code
    set.insert(b"    "); // Blank code (Slice 6)
    set.insert(b"   "); // Blank code (3-char variant)
    set.insert(b""); // Allow empty trim (Slice 6)
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
                        return mode <= 6 && location <= 5;
                    }
                }
                return false;
            }
        }

        if self.alpha_mode {
            // mode <= 6 (standard), location <= 5 (standard).
            // version <= 7 (3-bit field).
            mode <= 6 && location <= 5 && self.version <= 7
        } else {
            if mode > 6 || location > 15 {
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
            // Alpha v105 version 7 reuses the legacy compact bits for other header state.
            // Treat compactness as opt-in there so code-aware overrides can drive the final decision.
            let mut is_compact = if self.version == 7 {
                false
            } else {
                (flags & (1 << 21)) != 0
            };

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
            if self.version == 5 {
                let is_fragment = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
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
            (flags & (1 << 21)) != 0
        }
    }

    pub fn is_identified(&self, flags: u32) -> bool {
        (flags & (1 << 4)) != 0
    }

    pub fn is_ethereal(&self, flags: u32) -> bool {
        if self.alpha_mode {
            if self.version == 5 || self.version == 2 {
                (flags & (1 << 24)) != 0
            } else {
                (flags & (1 << 22)) != 0
            }
        } else {
            (flags & (1 << 22)) != 0
        }
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool, code: Option<&str>) -> bool {
        if self.alpha_mode {
            if self.is_runeword(flags, code, false) {
                return true;
            }
            if self.version == 5 {
                !is_compact && (flags & (1 << 11)) != 0
            } else if self.version == 1
                || self.version == 2
                || self.version == 0
                || self.version == 7
                || self.version == 4
                || self.version == 6
            {
                (flags & (1 << 11)) != 0
            } else {
                (flags & (1 << 27)) != 0
            }
        } else {
            (flags & (1 << 11)) != 0
        }
    }

    pub fn is_personalized(&self, flags: u32, is_compact: bool) -> bool {
        if is_compact {
            return false;
        }
        if self.alpha_mode {
            if self.version == 7 {
                return false;
            }
            // Forensic (Axiom 0337): Personalization bit is 24 across most Alpha v105 variants.
            (flags & (1 << 24)) != 0
        } else {
            (flags & (1 << 28)) != 0
        }
    }

    pub fn is_runeword(&self, flags: u32, code: Option<&str>, has_checksum: bool) -> bool {
        let trimmed = code.unwrap_or("").trim();

        let mut is_forced_rw = false;
        if self.alpha_mode {
            if trimmed == "xrs" || trimmed == "c8xr" || trimmed == "rhd" || trimmed == "wa2" {
                is_forced_rw = true;
            }
            let reg = crate::domain::forensic::registry::get_registry();
            if let Some(codes) = &reg.forced_runeword_codes {
                if codes.iter().any(|rc| rc == trimmed) {
                    is_forced_rw = true;
                }
            }
        }

        if !is_forced_rw && ((flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0) {
            return false;
        }

        let has_bit_26 = (flags & (1 << 26)) != 0;

        if self.alpha_mode {
            if is_forced_rw {
                return true;
            }
            if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed)
                || trimmed == "ucb8"
                || trimmed == "bwcw"
            {
                return false;
            }

            // Preserve normal alpha base templates from the broad v5/v1 fallback.
            if crate::data::repository::legacy_item_template(trimmed).is_some() {
                return false;
            }

            // Alpha Forensic (Slice 19): Only trust bit 26 if we have a valid checksum
            // or if it's one of the known complex versions (1 or 5).
            if has_bit_26 && (has_checksum || self.version == 5 || self.version == 1) {
                return true;
            }

            if self.version == 5 || self.version == 1 {
                let is_frag = (flags & (1 << 27)) != 0;
                return !is_frag
                    && ((flags & (1 << 11)) != 0
                        || (flags & (1 << 12)) != 0
                        || (flags & (1 << 13)) != 0
                        || (flags & 0x800) != 0);
            }

            return false;
        }
        has_bit_26
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
            && (self.version == 5 || self.version == 2 || self.version == 0 || self.version == 1)
        {
            if (flags & (1 << 27)) != 0 || (flags & (1 << 26)) != 0 {
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
            let is_equipped = mode == Some(1);

            let is_summary = if let Some(c) = code_hint {
                crate::domain::forensic::v105::axioms::is_v105_summary_code(c)
            } else {
                // Forensic Peek: In Alpha v105 (v0, v1, v2, v4, v5, v6), compact items are almost always
                // summary items (potions, scrolls, etc.) which use 3-bit Y fields.
                is_compact
                    && (self.version == 0
                        || self.version == 1
                        || self.version == 2
                        || self.version == 4
                        || self.version == 5
                        || self.version == 6)
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
                80
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
                    y_bits: 3,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: true,
                    skip_geometry: false,
                    target_width: 0,
                };
            }

            if is_compact {
                let has_header_gap = self.version == 5
                    || self.version == 0
                    || self.version == 1
                    || self.version == 2
                    || self.version == 4
                    || self.version == 6;
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
                y_bits: 4,
                page_bits: 3,
                socket_hint_bits: if self.version == 7 { 1 } else { 4 },
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
                y_bits: 4,
                page_bits: 3,
                socket_hint_bits: 0,
                has_header_gap: false,
                skip_geometry: false,
                target_width: 0,
            }
        }
    }
}
