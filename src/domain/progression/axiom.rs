use crate::domain::item::axiom_meta::{Confidence, Intentionality, ForensicAxiom};
// Note: impl_forensic_axiom! is available via crate::impl_forensic_axiom!

/// Alpha v105 Fixed Header Offsets and Lengths
/// 
/// These constants define the physical layout of the progression data within 
/// the Alpha v105 save game header (the first 833 bytes).

pub const V105_HEADER_LEN: usize = 833;

/// Quest Section starts at 0x193 (403).
pub const V105_QUEST_OFFSET: usize = 0x193;
pub const V105_QUEST_LEN: usize = 298; // 0x2BD - 0x193

/// Waypoint Section (WS) starts at 0x2BD (701).
pub const V105_WAYPOINT_OFFSET: usize = 0x2BD;
pub const V105_WAYPOINT_LEN: usize = 81; // 0x30E - 0x2BD

/// NPC Section (Expansion) starts at 0x30E (782).
pub const V105_NPC_OFFSET: usize = 0x30E;
pub const V105_NPC_LEN: usize = 51; // 833 - 0x30E
