use crate::domain::item::quality::ItemQuality;
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError};
use bitstream_io::BitRead;
use serde::Serialize;
use crate::domain::stats::axiom::StatsAxiom;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ItemSegmentType {
    Root,
    Header,
    Code,
    Stats,
    ExtendedStats,
    ItemIndex,
    Unknown,
}

impl Default for ItemSegmentType {
    fn default() -> Self {
        ItemSegmentType::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemHeader {
    pub flags: u32,
    pub version: u8,
    pub mode: u8,
    pub location: u8,
    pub x: u8,
    pub y: u8,
    pub page: u8,
    pub socket_hint: u8,
    
    pub id: Option<u32>,
    pub level: Option<u8>,
    pub quality: Option<ItemQuality>,
    pub is_compact: bool,
    pub is_identified: bool,
    pub is_socketed: bool,
    pub is_personalized: bool,
    pub is_runeword: bool,
    pub is_ethereal: bool,
    pub is_ear: bool,

    // Alpha Forensic Preservation Fields
    pub has_checksum: bool,
    pub alpha_quality_raw: Option<u8>,
    pub alpha_v5_runeword_extra: Option<u8>,
    pub alpha_unique_id_raw: Option<u16>,
    pub save_is_alpha: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HeaderGeometry {
    pub y_bits: u32,
    pub page_bits: u32,
    pub socket_hint_bits: u32,
    pub has_header_gap: bool,
    pub skip_geometry: bool,
    pub target_width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAxiom {
    pub version: u8,
    pub alpha_mode: bool,
}

impl HeaderAxiom {
    pub fn new(version: u8, alpha_mode: bool) -> Self {
        Self { version, alpha_mode }
    }

    pub fn is_alpha(&self) -> bool {
        self.alpha_mode && (self.version == 5 || self.version == 1 || self.version == 2 || self.version == 0 || self.version == 7 || self.version == 4 || self.version == 6)
    }

    pub fn is_plausible(&self, mode: u8, location: u8, code: &str, _flags: u32) -> bool {
        let trimmed = code.trim();
        if trimmed.is_empty() { return false; }
        
        if code.starts_with(' ') {
            return false;
        }

        // Strictly reject non-alphanumeric codes to avoid bit-shifted ghost items
        if trimmed.chars().any(|c| !c.is_alphanumeric() && c != ' ') {
            return false;
        }

        if self.alpha_mode {
            // Forensic: Alpha v105 follows standard mode/location boundaries
            return mode <= 6 && location <= 5;
        } else {
            if mode > 6 || location > 15 { return false; }
            true
        }
    }

    pub fn is_compact(&self, flags: u32, code: Option<&str>) -> bool {
        if self.is_runeword(flags, code) {
            return false;
        }
        if self.alpha_mode {
            let mut is_compact = (flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0;
            if let Some(c) = code {
                let trimmed = c.trim();
                if trimmed.is_empty() { return true; }
                
                let reg = crate::domain::forensic::registry::get_registry();
                if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed) {
                    is_compact = true;
                }
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_compact") { is_compact = val != 0; }
                    }
                }
            }
            
            if self.version == 5 {
                let is_fragment = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                is_compact && !is_fragment
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

    pub fn is_socketed(&self, flags: u32, is_compact: bool) -> bool {
        if self.alpha_mode {
            if self.version == 5 {
                !is_compact && (flags & (1 << 11)) != 0
            } else if self.version == 1 || self.version == 2 || self.version == 0 || self.version == 7 || self.version == 4 || self.version == 6 {
                (flags & (1 << 11)) != 0
            } else {
                (flags & (1 << 27)) != 0
            }
        } else {
            (flags & (1 << 11)) != 0
        }
    }

    pub fn is_personalized(&self, flags: u32) -> bool {
        if self.alpha_mode {
            // Forensic (Axiom 0337): Personalization bit is 28 across most Alpha v105 variants.
            (flags & (1 << 28)) != 0
        } else {
            (flags & (1 << 28)) != 0
        }
    }

    pub fn is_runeword(&self, flags: u32, code: Option<&str>) -> bool {
        if (flags & (1 << 26)) != 0 { return true; }
        if self.alpha_mode {
            if let Some(c) = code {
                let trimmed = c.trim();
                let reg = crate::domain::forensic::registry::get_registry();
                
                // 1. Check registry root list
                if let Some(codes) = &reg.forced_runeword_codes {
                    if codes.iter().any(|rc| rc == trimmed) { return true; }
                }
                
                // 2. Check item overrides
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_runeword") { return val != 0; }
                    }
                }
                
                if self.version == 5 || self.version == 1 {
                    let is_frag = (flags & (1 << 27)) != 0;
                    return !is_frag && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0);
                }
            } else if self.version == 5 || self.version == 1 {
                return false; // Conservatively false if no code hint
            }
            
            if self.version == 0 || self.version == 4 || self.version == 6 || self.version == 7 {
                return (flags & (1 << 26)) != 0;
            }
        }
        (flags & (1 << 26)) != 0
    }

    pub fn is_v105_shadow(&self, flags: u32) -> bool {
        self.alpha_mode && (self.version == 5 || self.version == 2) && ((flags & (1 << 27)) != 0 || (flags & (1 << 26)) != 0)
    }

    pub fn header_geometry(&self, flags: u32, code_hint: Option<&str>) -> HeaderGeometry {
        let is_compact = self.is_compact(flags, code_hint);
        let is_personalized = self.is_personalized(flags);

        if self.alpha_mode {
            let is_rw = self.is_runeword(flags, code_hint);
            let is_v105_shadow = self.is_v105_shadow(flags);

            if is_rw || is_v105_shadow || is_personalized {
                return HeaderGeometry {
                    y_bits: 0,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: true,
                    skip_geometry: true,
                    target_width: 80,
                };
            }

            let mut target_width = if self.is_alpha() {
                let code_str = code_hint.unwrap_or("");
                crate::domain::forensic::v105::axioms::get_v105_target_width(self.version, code_str, flags)
            } else { 80 };

            let is_summary = crate::domain::forensic::v105::axioms::is_v105_summary_code(code_hint.unwrap_or(""));

            if is_compact && self.alpha_mode {
                // For compact items, target_width from axioms is the TOTAL width.
                // Subtract 24 bits for the fixed-width code to get the header target.
                target_width = target_width.saturating_sub(24);
            }

            if is_summary {
                return HeaderGeometry {
                    y_bits: 3,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: true,
                    skip_geometry: false,
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
                y_bits: 0, page_bits: 0, socket_hint_bits: 0, has_header_gap: false, skip_geometry: true, target_width: 0,
            }
        } else {
            HeaderGeometry {
                y_bits: 4, page_bits: 3, socket_hint_bits: 0, has_header_gap: false, skip_geometry: false, target_width: 0,
            }
        }
    }
}

impl ItemHeader {
    pub fn read_from_cursor<R: BitRead>(
        cursor: &mut BitCursor<R>,
        alpha_mode: bool,
        code: Option<&str>,
    ) -> ParsingResult<(Self, Option<u32>)> {
        let start_bit = cursor.pos();
        cursor.begin_segment(ItemSegmentType::Header);

        let flags = cursor.read_bits::<u32>(32)?;
        if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
             return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
        }

        let (version, has_checksum) = if alpha_mode {
            let saved_pos = cursor.checkpoint();
            let checksum = cursor.read_bits::<u8>(8)?;
            let v = cursor.read_bits::<u8>(3)? as u8;
            let expected = calculate_alpha_v105_checksum(flags, v);
            
            if checksum == expected {
                (v, true)
            } else {
                cursor.rollback(saved_pos);
                (cursor.read_bits::<u8>(3)? as u8, false)
            }
        } else {
            (cursor.read_bits::<u8>(3)? as u8, false)
        };
        let mode = cursor.read_bits::<u8>(3)? as u8;
        let location = cursor.read_bits::<u8>(3)? as u8;
        let x = cursor.read_bits::<u8>(4)? as u8;
        
        let axiom = HeaderAxiom::new(version, alpha_mode);
        let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
        
        let mut y = 0;
        let mut page = 0;
        let mut socket_hint = 0;

        let geometry = axiom.header_geometry(flags, code);
        let is_compact = axiom.is_compact(flags, code);

        let mut alpha_header_gap = None;
        if geometry.has_header_gap {
            if axiom.is_alpha() {
                let is_v105_shadow = s_axiom.is_v105_shadow(flags);
                let is_rw = s_axiom.is_runeword(flags);
                if is_rw || is_v105_shadow {
                    let is_v105_shadow_local = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                    let gap_bits = if is_v105_shadow_local { 8 } else { 24 }; 
                    let gap = cursor.read_bits::<u32>(gap_bits)?;
                    alpha_header_gap = Some(gap);

                    if !is_compact {
                        y = (gap & 0x0F) as u8;
                        page = ((gap >> 4) & 0x07) as u8;
                        socket_hint = ((gap >> 7) & 0x01) as u8;
                    }
                } else {
                    y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                    page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                    socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                    
                    if geometry.has_header_gap || !has_checksum {
                        alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
                    }
                }
            } else {
                y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                
                if geometry.has_header_gap || !has_checksum {
                    alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
                }
            }
        } else if !geometry.skip_geometry {
            y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
            page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
            socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
        }

        if alpha_mode && geometry.target_width > 0 {
            let current_bits = (cursor.pos() - start_bit) as u32;
            if current_bits < geometry.target_width {
                let to_read = geometry.target_width - current_bits;
                alpha_header_gap = Some(cursor.read_bits::<u32>(to_read)?);
            }
        }
        cursor.end_segment();

        Ok((ItemHeader {
            flags,
            version,
            mode,
            location,
            x,
            y,
            page,
            socket_hint,
            id: None,
            level: None,
            quality: None,
            is_compact,
            is_identified: s_axiom.is_identified(flags),
            is_socketed: s_axiom.is_socketed(flags, is_compact),
            is_personalized: axiom.is_personalized(flags),
            is_runeword: s_axiom.is_runeword(flags),
            is_ethereal: s_axiom.is_ethereal(flags),
            is_ear: (flags & (1 << 24)) != 0,
            has_checksum,
            alpha_quality_raw: None,
            alpha_v5_runeword_extra: None,
            alpha_unique_id_raw: None,
            save_is_alpha: alpha_mode,
        }, alpha_header_gap))
    }
}

pub fn parse_item_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    alpha_mode: bool,
    code: Option<&str>,
) -> ParsingResult<(ItemHeader, Option<u32>)> {
    ItemHeader::read_from_cursor(cursor, alpha_mode, code)
}

pub fn calculate_alpha_v105_checksum(flags: u32, version: u8) -> u8 {
    let b1 = (flags >> 24) & 0xFF;
    let b2 = (flags >> 16) & 0xFF;
    let b3 = (flags >> 8) & 0xFF;
    let b4 = flags & 0xFF;
    let v = (version & 0x07) as u32;
    (b1 ^ b2 ^ b3 ^ b4 ^ v ^ 0x87) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_v105_checksum_known_vector() {
        assert_eq!(calculate_alpha_v105_checksum(0, 0), 0x87);
        assert_eq!(calculate_alpha_v105_checksum(0x01020304, 5), 0x86);
        assert_eq!(calculate_alpha_v105_checksum(0xFFFFFFFF, 7), 0x80);
    }
}
