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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FailureFamily {
    Geometry,
    RWSet,
    Stat,
    Nudge,
    Unknown,
}

impl FailureFamily {
    pub fn as_tag(&self) -> String {
        format!("[{}]", match self {
            Self::Geometry => "Geometry",
            Self::RWSet => "RW/Set",
            Self::Stat => "Stat",
            Self::Nudge => "Nudge",
            Self::Unknown => "Unknown",
        })
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "geometry" => Some(Self::Geometry),
            "rwset" | "rw" | "set" => Some(Self::RWSet),
            "stat" => Some(Self::Stat),
            "nudge" => Some(Self::Nudge),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ParsedResult<T> {
    Success(T),
    Partial {
        item: T,
        diagnosis: FailureFamily,
        unknown_bits: Vec<(u64, u64)>,
    },
    Unknown {
        range: (u64, u64),
        raw: Vec<u8>,
        inferred_type: Option<String>,
        diagnosis: Option<FailureFamily>,
    },
    Error(String),
}
