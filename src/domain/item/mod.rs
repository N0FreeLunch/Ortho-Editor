pub mod quality;
pub mod entity;
pub mod serialization;
pub mod axiom_meta;
pub mod scanner;
pub mod editor;

pub use quality::{ItemQuality, map_item_quality};
pub use crate::domain::header::entity::ItemHeader;
pub use entity::{
    ItemBody, Item, ItemModule, CharmBagData, CursedItemData,
    RecordedBit, ItemBitRange, BitSegment
};
// Removed redundant re-exports: ItemProperty, ItemStats moved to domain::stats
pub use serialization::{BitEmitter, HuffmanTree, verify_marker_lookahead, peek_item_header_at};
pub use scanner::scan_item_markers;
pub use axiom_meta::{Confidence, Intentionality, ForensicMetadata, ForensicAudit, ForensicResult};
pub use editor::{ItemEditor, ItemEditorExt};
