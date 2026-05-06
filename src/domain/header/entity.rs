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
    pub alpha_quality_raw: Option<u8>,
    pub alpha_v5_runeword_extra: Option<u8>,
    pub alpha_unique_id_raw: Option<u16>,
}

#[derive(Debug, Clone, Copy)]
pub struct HeaderGeometry {
    pub y_bits: u32,
    pub page_bits: u32,
    pub socket_hint_bits: u32,
    pub has_header_gap: bool,
    pub skip_geometry: bool,
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

    pub fn is_plausible(&self, mode: u8, location: u8, code: &str, flags: u32) -> bool {
        let trimmed = code.trim();
        if trimmed.is_empty() { return false; }
        
        if code.starts_with(' ') {
            return false;
        }

        if self.alpha_mode {
            // High leniency for other fields in Alpha forensics
            return mode <= 7 && location <= 15;
        } else {
            if mode > 6 || location > 15 { return false; }
            true
        }
    }

    pub fn is_compact(&self, flags: u32) -> bool {
        if self.is_runeword(flags) {
            return false;
        }
        if self.alpha_mode {
            let identified = (flags & 1) != 0;
            let runeword_bit = (flags & (1 << 26)) != 0;
            if self.version == 5 {
                (flags & (1 << 23)) != 0 && !runeword_bit && identified
            } else if self.version == 6 || self.version == 7 {
                (flags & (1 << 21)) != 0 && !runeword_bit && identified
            } else {
                false
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
            if self.version == 4 {
                (flags & (1 << 28)) != 0
            } else {
                (flags & (1 << 29)) != 0
            }
        } else {
            (flags & (1 << 28)) != 0
        }
    }

    pub fn is_runeword(&self, flags: u32) -> bool {
        if self.alpha_mode {
            if self.version == 5 || self.version == 1 {
                let is_frag = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                !is_frag && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0)
            } else if self.version == 0 || self.version == 4 || self.version == 6 || self.version == 7 {
                (flags & (1 << 26)) != 0
            } else {
                (flags & (1 << 11)) != 0
            }
        } else {
            (flags & (1 << 26)) != 0
        }
    }

    pub fn is_v105_shadow(&self, flags: u32) -> bool {
        self.alpha_mode && (self.version == 5 || self.version == 2) && ((flags & (1 << 27)) != 0 || (flags & (1 << 26)) != 0)
    }

    pub fn header_geometry(&self, flags: u32, is_compact: bool, is_personalized: bool) -> HeaderGeometry {
        if self.alpha_mode {
            if self.version == 5 || self.version == 1 || self.version == 2 || self.version == 0 || self.version == 7 || self.version == 4 || self.version == 6 {
                let is_rw = self.is_runeword(flags);
                let is_v105_shadow = self.is_v105_shadow(flags);

                if is_rw || is_v105_shadow || is_personalized {
                    HeaderGeometry {
                        y_bits: 0,
                        page_bits: 0,
                        socket_hint_bits: 0,
                        has_header_gap: true,
                        skip_geometry: true,
                    }
                } else {
                    HeaderGeometry {
                        y_bits: if is_compact { 0 } else { 4 },
                        page_bits: if is_compact { 0 } else { 3 },
                        socket_hint_bits: if is_compact { 0 } else if self.version == 7 { 1 } else { 4 },
                        has_header_gap: true,
                        skip_geometry: false,
                    }
                }
            } else if self.version == 4 || self.version == 5 {
                HeaderGeometry {
                    y_bits: if is_compact { 0 } else { 4 },
                    page_bits: if is_compact { 0 } else { 3 },
                    socket_hint_bits: if is_compact { 0 } else { 3 },
                    has_header_gap: self.version == 5,
                    skip_geometry: is_compact,
                }
            } else if self.version == 0 || self.version == 1 || self.version == 2 {
                HeaderGeometry {
                    y_bits: if is_compact { 0 } else { 4 },
                    page_bits: if is_compact { 0 } else { 3 },
                    socket_hint_bits: if is_compact { 0 } else { 3 },
                    has_header_gap: false,
                    skip_geometry: is_compact,
                }
            } else {
                HeaderGeometry {
                    y_bits: if is_compact { 0 } else { 4 },
                    page_bits: if is_compact { 0 } else { 3 },
                    socket_hint_bits: if is_compact { 0 } else { 3 },
                    has_header_gap: false,
                    skip_geometry: is_compact,
                }
            }
        } else {
            HeaderGeometry {
                y_bits: if is_compact { 0 } else { 4 },
                page_bits: if is_compact { 0 } else { 3 },
                socket_hint_bits: if is_compact { 0 } else { 3 },
                has_header_gap: false,
                skip_geometry: is_compact,
            }
        }
    }
}

impl ItemHeader {
    pub fn read_from_cursor<R: BitRead>(
        cursor: &mut BitCursor<R>,
        alpha_mode: bool,
    ) -> ParsingResult<(Self, Option<u32>)> {
        let start_bit = cursor.pos();
        cursor.begin_segment(ItemSegmentType::Header);

        let flags = cursor.read_bits::<u32>(32)?;
        if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
             return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
        }

        let version = cursor.read_bits::<u8>(3)? as u8;
        let mode = cursor.read_bits::<u8>(3)? as u8;
        let location = cursor.read_bits::<u8>(3)? as u8;
        let x = cursor.read_bits::<u8>(4)? as u8;
        
        let axiom = HeaderAxiom::new(version, alpha_mode);
        let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
        let is_compact = s_axiom.is_compact(flags);
        let is_personalized = s_axiom.is_personalized(flags);
        
        let mut y = 0;
        let mut page = 0;
        let mut socket_hint = 0;

        let geometry = axiom.header_geometry(flags, is_compact, is_personalized);

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
                    if !is_compact {
                        y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                        page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                        socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                    }
                    alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
                }
            } else {
                if !is_compact {
                    y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                    page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                    socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                }
                alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
            }
        } else if !geometry.skip_geometry {
            y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
            page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
            socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
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
            is_personalized,
            is_runeword: s_axiom.is_runeword(flags),
            is_ethereal: s_axiom.is_ethereal(flags),
            is_ear: (flags & (1 << 24)) != 0,
            alpha_quality_raw: None,
            alpha_v5_runeword_extra: None,
            alpha_unique_id_raw: None,
        }, alpha_header_gap))
    }
}

pub fn parse_item_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    alpha_mode: bool,
) -> ParsingResult<(ItemHeader, Option<u32>)> {
    ItemHeader::read_from_cursor(cursor, alpha_mode)
}
