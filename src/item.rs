pub use crate::domain::item::{Item, ItemQuality, ItemBitRange, RecordedBit, ItemModule, BitSegment, ItemBody};
pub use crate::domain::header::entity::{ItemSegmentType, ItemHeader};
pub use crate::domain::item::serialization::{HuffmanTree, find_next_item_match, peek_item_header_at, is_plausible_item_header, PropertyReaderContext};
pub use crate::error::{ParsingError, ParsingFailure, ParsingResult};
pub use crate::domain::stats::{ItemProperty, ItemStats};

pub(crate) fn item_trace_enabled() -> bool {
    std::env::var_os("D2R_ITEM_TRACE").is_some()
}

#[macro_export]
macro_rules! item_trace {
    ($($arg:tt)*) => {
        if crate::item::item_trace_enabled() {
            println!($($arg)*);
        }
    };
}
