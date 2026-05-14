pub mod quest;
pub mod waypoint;
pub mod axiom;

pub use quest::{Quest, QuestSet, QuestSection};
pub use waypoint::{Waypoint, WaypointSet, WaypointSection};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicResult, ForensicAxiom};
use crate::domain::progression::axiom::{AlphaDifficultyAxiom, V105QuestAxiom, V105WaypointAxiom, PROG_START_FILE, V105_QUEST_OFFSET, V105_WAYPOINT_OFFSET, V105_NPC_OFFSET};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progression {
    pub difficulty: u8,
    pub quests: QuestSet,
    pub waypoints: WaypointSet,
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

            let wp_axiom = V105WaypointAxiom;
            audit.record(wp_axiom.metadata());
            let wp_anchor = PROG_START_FILE + V105WaypointAxiom::start_offset();
            
            let wp_bytes = if bytes.len() > V105_WAYPOINT_OFFSET {
                &bytes[V105_WAYPOINT_OFFSET..]
            } else {
                &[]
            };
            let waypoints = WaypointSet::from_bytes(wp_bytes, difficulty, wp_anchor);
            
            // Slice 4: Mercenary Forensic Integration
            let header = &bytes[0..V105_WAYPOINT_OFFSET.min(bytes.len())]; // Rough header approximation
            let w4_bytes = if bytes.len() > V105_NPC_OFFSET {
                Some(&bytes[V105_NPC_OFFSET..])
            } else {
                None
            };
            let merc = crate::domain::forensic::v105::mercenary::MercenaryState::from_hybrid(header, w4_bytes);
            merc.record_forensics(&mut audit);

            ForensicResult { value: Ok(Progression { difficulty, quests, waypoints, audit: audit.clone() }), audit }
        } else {
            let difficulty = if bytes.len() > 0x0257 { bytes[0x0257] } else { 0 };
            let quests = QuestSet::new_v105_empty(); // Placeholder for retail
            let waypoints = WaypointSet::new_empty(difficulty);
            ForensicResult { value: Ok(Progression { difficulty, quests, waypoints, audit: audit.clone() }), audit }
        }
    }

    pub fn sync_to_bytes(&self, bytes: &mut [u8], alpha_mode: bool) {
        if alpha_mode {
            if bytes.len() > PROG_START_FILE + 21 {
                bytes[PROG_START_FILE + 21] = (bytes[PROG_START_FILE + 21] & !0x18) | ((self.difficulty & 0x03) << 3);
            }

            let normal_anchor = PROG_START_FILE + V105QuestAxiom::normal_start();
            let act5_anchor = PROG_START_FILE + V105QuestAxiom::act5_start();
            
            if bytes.len() > V105_QUEST_OFFSET {
                self.quests.sync_to_v105_bytes(&mut bytes[V105_QUEST_OFFSET..], normal_anchor, act5_anchor);
            }

            let wp_anchor = PROG_START_FILE + V105WaypointAxiom::start_offset();
            if bytes.len() > V105_WAYPOINT_OFFSET {
                self.waypoints.sync_to_bytes(&mut bytes[V105_WAYPOINT_OFFSET..], wp_anchor);
            }
        }
    }
}

