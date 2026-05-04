pub mod quest;
pub mod waypoint;
pub mod axiom;

pub use quest::{Quest, QuestSet, QuestSection};
pub use waypoint::{Waypoint, WaypointSet, WaypointSection};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicResult, ForensicAxiom};
use crate::domain::progression::axiom::AlphaDifficultyAxiom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progression {
    pub difficulty: u8,
    pub audit: ForensicAudit,
}

impl Progression {
    pub fn from_bytes(bytes: &[u8], alpha_mode: bool) -> ForensicResult<Self> {
        let mut audit = ForensicAudit::new();
        
        let difficulty = if alpha_mode {
            if bytes.len() > 21 {
                let axiom = AlphaDifficultyAxiom;
                audit.record(axiom.metadata());
                (bytes[21] & 0x18) >> 3
            } else { 
                0 
            }
        } else {
            if bytes.len() > 0x0257 { bytes[0x0257] } else { 0 }
        };
        
        ForensicResult { value: Ok(Progression { difficulty, audit: audit.clone() }), audit }
    }
}

