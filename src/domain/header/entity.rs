use crate::domain::item::quality::ItemQuality;
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError};
use bitstream_io::BitRead;
use serde::Serialize;
use crate::domain::stats::axiom::StatsAxiom;

thread_local! {
    pub static IN_NESTED_RECOVERY: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

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
    pub alpha_checksum: Option<u8>,
    pub alpha_quality_raw: Option<u8>,
    pub alpha_v5_runeword_extra: Option<u8>,
    pub alpha_unique_id_raw: Option<u16>,
    pub save_is_alpha: bool,
}

pub use crate::domain::forensic::header_policy::{HeaderAxiom, HeaderGeometry};

impl ItemHeader {
    pub fn read_from_cursor<R: BitRead>(
        cursor: &mut BitCursor<R>,
        alpha_mode: bool,
        code: Option<&str>,
    ) -> ParsingResult<(Self, Option<u32>)> {
        let start_bit = cursor.pos();
        cursor.begin_segment(ItemSegmentType::Header);

        let flags = cursor.read_bits::<u32>(32)?;
        let is_nested = cursor.context_stack().iter().any(|s| s == "nested") || IN_NESTED_RECOVERY.with(|v| v.get());
        crate::item_trace!(
            "[TEST-DEBUG] entity side IN_NESTED_RECOVERY: {}, is_nested: {}, alpha_mode: {}, flags: {:08x}",
            IN_NESTED_RECOVERY.with(|v| v.get()),
            is_nested,
            alpha_mode,
            flags
        );
        if !alpha_mode && !is_nested && (flags & 0xFFFF) != 0x4D4A {
             return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
        }

        let mut alpha_checksum = None;
        let (version, has_checksum) = if alpha_mode {
            let saved_pos = cursor.checkpoint();
            let checksum = cursor.read_bits::<u8>(8)?;
            let v = cursor.read_bits::<u8>(3)? as u8;
            let expected = calculate_alpha_v105_checksum(flags, v);
            
            if checksum == expected && (v == 5 || v == 0 || v == 1 || v == 2 || v == 4 || v == 3) {
                alpha_checksum = Some(checksum);
                (v, true)
            } else {
                cursor.rollback(saved_pos);
                (cursor.read_bits::<u8>(3)? as u8, false)
            }
        } else {
            (cursor.read_bits::<u8>(3)? as u8, false)
        };
        let mode = cursor.read_bits::<u8>(3)? as u8;
        let (x, location) = if alpha_mode {
            (cursor.read_bits::<u8>(3)? as u8, cursor.read_bits::<u8>(4)? as u8)
        } else {
            let loc = cursor.read_bits::<u8>(3)? as u8;
            let x_val = cursor.read_bits::<u8>(4)? as u8;
            (x_val, loc)
        };
        
        let axiom = HeaderAxiom::new(version, alpha_mode);
        let mut s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
        if let Some(c) = code {
            s_axiom = s_axiom.with_code(c);
        }
        
        let mut y = 0;
        let mut page = 0;
        let mut socket_hint = 0;

        let geometry = axiom.header_geometry(flags, code);
        let is_compact = axiom.is_compact(flags, code);

        let mut alpha_header_gap = None;
        if geometry.has_header_gap {
            if axiom.is_alpha() {
                let is_v105_shadow = s_axiom.is_v105_shadow(flags, code);
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
                        let gap = cursor.read_bits::<u32>(8)?;
                        alpha_header_gap = Some(gap);
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
            is_socketed: s_axiom.is_socketed(flags, is_compact, code),
            is_personalized: axiom.is_personalized(flags, is_compact),
            is_runeword: s_axiom.is_runeword(flags),
            is_ethereal: s_axiom.is_ethereal(flags),
            is_ear: (flags & (1 << 24)) != 0,
            has_checksum,
            alpha_checksum,
            alpha_quality_raw: None,
            alpha_v5_runeword_extra: None,
            alpha_unique_id_raw: None,
            save_is_alpha: alpha_mode,
        }, alpha_header_gap))
    }
}

pub fn calculate_alpha_v105_checksum(flags: u32, version: u8) -> u8 {
    let mut bits_set = 0u8;
    for i in 0..32 {
        if (flags & (1u32 << i)) != 0 { bits_set += 1; }
    }

    let policy = crate::data::generated::layout_policy::alpha_item_header_checksum_policy(version);
    bits_set
        .wrapping_mul(policy.multiplier)
        .wrapping_add(policy.addend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_v105_checksum_known_vector() {
        assert_eq!(calculate_alpha_v105_checksum(0x00A20010, 0), 21);
        assert_eq!(calculate_alpha_v105_checksum(0x87E10001, 1), 66);
    }
}
