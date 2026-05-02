use crate::domain::item::quality::ItemQuality;
use serde::Serialize;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAxiom {
    pub version: u8,
    pub alpha_mode: bool,
}

impl HeaderAxiom {
    pub fn is_alpha(&self) -> bool {
        self.alpha_mode && (self.version == 5 || self.version == 1 || self.version == 2 || self.version == 0)
    }

    pub fn is_plausible(&self, mode: u8, location: u8, code: &str, flags: u32) -> bool {
        let trimmed = code.trim();
        if trimmed.is_empty() { return false; }
        
        if code.starts_with(' ') {
            return false;
        }

        if self.alpha_mode {
            if matches!(trimmed, "ww l" | "xlp" | "buc") {
                return mode <= 7 && location <= 15 && (flags & 0xF8000000) == 0;
            }
            if !(self.version == 5 || self.version == 1) {
                return false; 
            }
            if mode > 7 || location > 15 { 
                return false; 
            }
            if (flags & 0xF8000000) != 0 {
                return false;
            }
            true
        } else {
            if mode > 6 || location > 15 { return false; }
            true
        }
    }
}
