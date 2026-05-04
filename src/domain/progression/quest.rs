use crate::data::quests::{QuestEntry, V105_QUESTS};

/// Alpha v105 quest sections include a 12-byte header before the quest payload.
pub const V105_QUEST_PAYLOAD_START: usize = 12;

#[derive(Clone, Copy)]
pub struct Quest {
    entry: &'static QuestEntry,
    state: u16,
}

impl PartialEq for Quest {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.entry, other.entry) && self.state == other.state
    }
}

impl Eq for Quest {}

impl std::fmt::Debug for Quest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Quest")
            .field("difficulty", &self.difficulty())
            .field("act", &self.act())
            .field("index", &self.index())
            .field("name", &self.name())
            .field("state", &format_args!("0x{:04X}", self.state))
            .field("v105_offset", &self.v105_offset())
            .finish()
    }
}

impl Quest {
    pub fn new(entry: &'static QuestEntry, state: u16) -> Self {
        Self { entry, state }
    }

    pub fn difficulty(&self) -> u8 {
        self.entry.difficulty
    }

    pub fn act(&self) -> u8 {
        self.entry.act
    }

    pub fn index(&self) -> u8 {
        self.entry.index
    }

    pub fn name(&self) -> &'static str {
        self.entry.name
    }

    pub fn v105_offset(&self) -> usize {
        self.entry.v105_offset
    }

    pub fn state(&self) -> u16 {
        self.state
    }

    pub fn is_completed(&self) -> bool {
        (self.state & 0x01) != 0
    }

    pub fn set_completed(&mut self, completed: bool) {
        if completed {
            self.state |= 0x01;
            // Also set 0x10 in high byte (seen/checked) as per save.rs logic
            self.state |= 0x1000;
        } else {
            self.state &= !0x01;
            self.state &= !0x1000;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestSet {
    quests: Vec<Quest>,
}

impl QuestSet {
    pub fn new_v105_empty() -> Self {
        let quests = V105_QUESTS.iter().map(|e| Quest::new(e, 0)).collect();
        Self { quests }
    }

    pub fn from_v105_bytes(bytes: &[u8], normal_anchor: usize, act5_anchor: usize) -> Self {
        let quests = V105_QUESTS
            .iter()
            .map(|entry| {
                // Grounding Logic: metadata (V105_QUESTS) uses a legacy 415 base.
                // Discussion 0230 revealed that Act 5 starts at a drifted anchor (0x90).
                let legacy_base = 415;
                const STRIDE: usize = 2;

                let (anchor, base_index) = if entry.act == 5 {
                    (act5_anchor, 24 * STRIDE)
                } else {
                    (normal_anchor, 0)
                };
                
                let quest_index_bytes = if entry.v105_offset >= legacy_base {
                    entry.v105_offset - legacy_base
                } else {
                    0
                };

                // The offset relative to the specific act's anchor
                let relative_to_anchor = if quest_index_bytes >= base_index {
                    quest_index_bytes - base_index
                } else {
                    0
                };

                // The payload starts at the anchor + relative quest index.
                let absolute_payload_start = anchor + relative_to_anchor;
                let section_offset = if absolute_payload_start >= crate::domain::progression::axiom::V105_QUEST_OFFSET {
                    absolute_payload_start - crate::domain::progression::axiom::V105_QUEST_OFFSET
                } else {
                    V105_QUEST_PAYLOAD_START
                };

                let state = if section_offset + 1 < bytes.len() {
                    u16::from_le_bytes([bytes[section_offset], bytes[section_offset + 1]])
                } else {
                    0
                };
                Quest::new(entry, state)
            })
            .collect();
        Self { quests }
    }

    pub fn sync_to_v105_bytes(&self, bytes: &mut [u8], normal_anchor: usize, act5_anchor: usize) {
        for quest in &self.quests {
            let legacy_base = 415;
            const STRIDE: usize = 2;

            let (anchor, base_index) = if quest.act() == 5 {
                (act5_anchor, 24 * STRIDE)
            } else {
                (normal_anchor, 0)
            };

            let quest_index_bytes = if quest.v105_offset() >= legacy_base {
                quest.v105_offset() - legacy_base
            } else {
                0
            };

            let relative_to_anchor = if quest_index_bytes >= base_index {
                quest_index_bytes - base_index
            } else {
                0
            };

            let absolute_payload_start = anchor + relative_to_anchor;
            let section_offset = if absolute_payload_start >= crate::domain::progression::axiom::V105_QUEST_OFFSET {
                absolute_payload_start - crate::domain::progression::axiom::V105_QUEST_OFFSET
            } else {
                V105_QUEST_PAYLOAD_START
            };

            if section_offset + 1 < bytes.len() {
                let le_bytes = quest.state().to_le_bytes();
                bytes[section_offset] = le_bytes[0];
                bytes[section_offset + 1] = le_bytes[1];
            }
        }
    }

    pub fn quests(&self) -> &[Quest] {
        &self.quests
    }

    pub fn quests_mut(&mut self) -> &mut [Quest] {
        &mut self.quests
    }

    pub fn find_by_name(&self, name: &str) -> Option<Quest> {
        self.quests.iter().find(|q| q.name() == name).copied()
    }

    pub fn filter_by_difficulty(&self, difficulty: u8) -> Vec<Quest> {
        self.quests
            .iter()
            .filter(|q| q.difficulty() == difficulty)
            .copied()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct QuestSection {
    pub raw_bytes: Vec<u8>,
}

impl QuestSection {
    pub fn from_slice(slice: &[u8]) -> Self {
        QuestSection {
            raw_bytes: slice.to_vec(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub fn is_v105_completed_by_name(&self, name: &str, normal_anchor: usize, act5_anchor: usize) -> bool {
        let set = QuestSet::from_v105_bytes(&self.raw_bytes, normal_anchor, act5_anchor);
        set.find_by_name(name).map(|q| q.is_completed()).unwrap_or(false)
    }

    pub fn set_v105_completed_by_name(&mut self, name: &str, completed: bool, normal_anchor: usize, act5_anchor: usize) -> bool {
        let mut set = QuestSet::from_v105_bytes(&self.raw_bytes, normal_anchor, act5_anchor);
        if let Some(q) = set.quests_mut().iter_mut().find(|q| q.name() == name) {
            q.set_completed(completed);
            set.sync_to_v105_bytes(&mut self.raw_bytes, normal_anchor, act5_anchor);
            return true;
        }
        false
    }

    /// Unlocks the Durance of Hate gate (Act 3) by setting semantic bits discovered in forensics.
    pub fn unlock_durance_gate(&mut self, normal_anchor: usize, act5_anchor: usize) {
        // 1. Set "Khalim's Will" Quest Completed Bits
        self.set_v105_completed_by_name("Khalim's Will", true, normal_anchor, act5_anchor);

        // 2. Set "Sacred Authority" / Gate Flag in the Quest Section Header (approx byte 8)
        if self.raw_bytes.len() > 8 {
            self.raw_bytes[8] |= 0x01; // Gate Flag
        }

        // 3. Set Environment State (approx 12th byte / before first quest)
        if self.raw_bytes.len() > 11 {
            self.raw_bytes[11] |= 0x80; // Orb Destroyed / Environment Trigger
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quest_set_initialization() {
        let quest_set = QuestSet::new_v105_empty();
        assert!(!quest_set.quests().is_empty());

        let den = quest_set.find_by_name("Den of Evil").expect("Should find Den of Evil");
        assert_eq!(den.difficulty(), 0);
        assert_eq!(den.act(), 1);
        assert!(!den.is_completed());
    }
}