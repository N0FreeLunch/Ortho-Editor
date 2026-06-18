pub use crate::domain::item::{Item, ItemQuality, ItemBitRange, RecordedBit, ItemModule, BitSegment, ItemBody, ItemEditor, ItemEditorExt};
pub use crate::domain::header::entity::{ItemSegmentType, ItemHeader};
pub use crate::domain::item::serialization::{find_next_item_match, peek_item_header_at, peek_item_header_at_specific_gap, is_plausible_item_header, PropertyReaderContext, verify_marker_lookahead, HuffmanTree};

pub use crate::domain::item::scanner::scan_item_markers;
pub use crate::error::{ParsingError, ParsingFailure, ParsingResult};
pub use crate::domain::stats::{ItemProperty, ItemStats};

pub(crate) fn item_trace_enabled() -> bool {
    std::env::var_os("D2R_ITEM_TRACE").is_some()
}

pub fn normalize_alpha_code_hint(code: &str) -> &str {
    let trimmed = code.trim();
    if trimmed == "us g" || trimmed == "k g" { return "jav"; }
    if trimmed == "7 p" || trimmed == "80sc" { return "ks d"; }
    if trimmed == "lbl" { return "b7ts"; }
    
    // Stealth codes (Alpha v105)
    let bytes: Vec<u8> = trimmed.chars().map(|c| c as u32 as u8).collect();
    if bytes.len() >= 2 && bytes[0] == 0xCF && bytes[1] == 0x4F { return "hp1"; }
    if bytes.len() >= 2 && bytes[0] == 0xCF && bytes[1] == 0x4D { return "mp1"; }
    if trimmed == "mp1" { return "mp1"; }
    
    trimmed
}

#[macro_export]
macro_rules! item_trace {
    ($($arg:tt)*) => {
        if crate::item::item_trace_enabled() {
            eprintln!($($arg)*);
        }
    };
}
