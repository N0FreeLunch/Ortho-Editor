pub mod quality;
pub mod entity;
pub mod serialization;
pub mod axiom_meta;
pub mod huffman;

pub use quality::{ItemQuality, map_item_quality};
pub use entity::{
    ItemHeader, ItemBody, Item, ItemModule, CharmBagData, CursedItemData,
    RecordedBit, ItemBitRange, BitSegment
};
// Removed redundant re-exports: ItemProperty, ItemStats moved to domain::stats
pub use serialization::BitEmitter;
pub use huffman::HuffmanTree;
pub use axiom_meta::{Confidence, Intentionality, ForensicMetadata, ForensicAudit, ForensicResult};
