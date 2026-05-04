pub mod quest;
pub mod waypoint;
pub mod axiom;

pub use quest::{Quest, QuestSet, QuestSection};
pub use waypoint::{Waypoint, WaypointSet, WaypointSection};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicResult, ForensicAxiom};
use crate::domain::progression::axiom::{AlphaDifficultyAxiom, V105QuestAxiom, PROG_START_FILE, V105_QUEST_OFFSET};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progression {
    pub difficulty: u8,
    pub quests: QuestSet,
    pub audit: ForensicAudit,
}

impl Progression {
    pub fn from_bytes(bytes: &[u8], alpha_mode: bool) -> ForensicResult<Self> {
        let mut audit = ForensicAudit::new();
        
        if alpha_mode {
            let difficulty = if bytes.len() > PROG_START_FILE + 21 {
                let diff_axiom = AlphaDifficultyAxiom;
                audit.record(diff_axiom.metadata());
                (bytes[PROG_START_FILE + 21] & 0x18) >> 3
            } else { 
                0 
            };

            let quest_axiom = V105QuestAxiom;
            audit.record(quest_axiom.metadata());
            
            let normal_anchor = PROG_START_FILE + V105QuestAxiom::normal_start();
            let act5_anchor = PROG_START_FILE + V105QuestAxiom::act5_start();
            
            let quest_bytes = if bytes.len() > V105_QUEST_OFFSET {
                &bytes[V105_QUEST_OFFSET..]
            } else {
                &[]
            };

            let quests = QuestSet::from_v105_bytes(quest_bytes, normal_anchor, act5_anchor);
            
            ForensicResult { value: Ok(Progression { difficulty, quests, audit: audit.clone() }), audit }
        } else {
            let difficulty = if bytes.len() > 0x0257 { bytes[0x0257] } else { 0 };
            let quests = QuestSet::new_v105_empty(); // Placeholder for retail
            ForensicResult { value: Ok(Progression { difficulty, quests, audit: audit.clone() }), audit }
        }
    }
}

