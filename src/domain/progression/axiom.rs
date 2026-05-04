use crate::domain::item::axiom_meta::{Confidence, Intentionality};
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

/// Progression Section starts at 0x127 (295).
/// Rationale: Verification from Discussion 0230 showed quest anchors at 0x78 (120) and 0x90 (144).
/// When anchored at 295, these offsets perfectly align with the legacy 415 hypothesis (295+120=415).
pub const PROG_START_FILE: usize = 0x127;

pub struct AlphaDifficultyAxiom;

crate::impl_forensic_axiom!(
    AlphaDifficultyAxiom,
    Confidence::VerifiedTruth,
    Intentionality::Structural,
    "Alpha v105 uses bits 3-4 of active_act (offset 21) for difficulty. 0x00A1 hypothesis was rejected in Discussion 0230."
);

pub struct V105QuestAxiom;

impl V105QuestAxiom {
    pub fn normal_start() -> usize { 120 } // 0x78
    pub fn act5_start() -> usize { 144 }   // 0x90
}

crate::impl_forensic_axiom!(
    V105QuestAxiom,
    Confidence::VerifiedTruth,
    Intentionality::Structural,
    "Alpha v105 quest offsets 0x78 (Normal) and 0x90 (Act 5) are relative to progression section (0x127). Verification from Discussion 0230."
);
