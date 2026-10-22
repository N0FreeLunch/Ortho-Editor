pub use crate::domain::item::{Item, ItemQuality, ItemBitRange, RecordedBit, ItemModule, BitSegment, ItemBody, ItemEditor, ItemEditorExt, ItemClassProjection, ItemClassRegistry, ConcurrentItemClassRegistry, LevelResult, ItemPlacement, OptionFidelityReport};
pub use crate::domain::header::entity::{ItemSegmentType, ItemHeader};
pub use crate::domain::header::quality::map_item_quality;
pub use crate::domain::item::serialization::{find_next_item_match, peek_item_header_at, peek_item_header_at_specific_gap, is_plausible_item_header, PropertyReaderContext, verify_marker_lookahead, HuffmanTree};

pub use crate::domain::item::scanner::scan_item_markers;
pub use crate::error::{ParsingError, ParsingFailure, ParsingResult};
pub use crate::domain::stats::{ItemProperty, ItemStats};

pub(crate) fn item_trace_enabled() -> bool {
    std::env::var_os("D2R_ITEM_TRACE").is_some()
}

pub fn normalize_alpha_code_hint(code: &str) -> &str {
    use crate::data::generated::layout_policy::{
        ALPHA_LEGACY_HINT_LOW_BYTE_PREFIXES, legacy_alpha_hint_alias_before_stealth,
        legacy_alpha_hint_identity_after_stealth,
    };

    let trimmed = code.trim();
    if let Some(alias) = legacy_alpha_hint_alias_before_stealth(trimmed) {
        return alias;
    }

    // Stealth codes (Alpha v105)
    let bytes: Vec<u8> = trimmed.chars().map(|c| c as u32 as u8).collect();
    for &(prefix, alias) in ALPHA_LEGACY_HINT_LOW_BYTE_PREFIXES {
        if bytes.starts_with(&prefix) {
            return alias;
        }
    }
    if let Some(identity) = legacy_alpha_hint_identity_after_stealth(trimmed) {
        return identity;
    }

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
