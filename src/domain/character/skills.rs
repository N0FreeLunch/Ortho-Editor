use std::io;

pub const SKILL_SECTION_LEN: usize = 30;

pub(crate) fn skill_payload_range(if_pos: usize, limit: Option<usize>) -> std::ops::Range<usize> {
    let start = if_pos + crate::domain::forensic::v105::V105SectionMarkerAxiom::MARKER_IF.len();
    let full_end = start + SKILL_SECTION_LEN;
    let end = limit.map_or(full_end, |limit| full_end.min(limit));
    start..end
}

#[derive(Clone, Debug)]
pub struct SkillSection(pub [u8; SKILL_SECTION_LEN]);

impl SkillSection {
    pub fn from_slice(slice: &[u8]) -> io::Result<Self> {
        if slice.len() != SKILL_SECTION_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "skill slice does not match expected length",
            ));
        }
        let mut data = [0u8; SKILL_SECTION_LEN];
        data.copy_from_slice(slice);
        Ok(SkillSection(data))
    }

    pub fn as_slice(&self) -> &[u8; SKILL_SECTION_LEN] {
        &self.0
    }

    /// Gets the skill level for a specific skill ID, using the class base ID.
    pub fn get_level(&self, base_id: u32, skill_id: u32) -> u8 {
        if skill_id < base_id {
            return 0;
        }
        let index = (skill_id - base_id) as usize;
        if index < SKILL_SECTION_LEN {
            self.0[index]
        } else {
            0
        }
    }

    /// Sets the skill level for a specific skill ID, using the class base ID.
    pub fn set_level(&mut self, base_id: u32, skill_id: u32, level: u8) {
        if skill_id < base_id {
            return;
        }
        let index = (skill_id - base_id) as usize;
        if index < SKILL_SECTION_LEN {
            self.0[index] = level;
        }
    }

    /// Returns an iterator over all skills in this section with their levels.
    pub fn iter_skills(&self, base_id: u32) -> impl Iterator<Item = SkillLevel> + '_ {
        self.0.iter().enumerate().map(move |(i, &level)| SkillLevel {
            skill_id: base_id + i as u32,
            level,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillLevel {
    pub skill_id: u32,
    pub level: u8,
}

pub fn parse_skill_section(bytes: &[u8], if_pos: usize, jm0: Option<usize>) -> io::Result<SkillSection> {
    let payload = skill_payload_range(if_pos, jm0);
    let start = payload.start;
    let end = payload.end;

    if end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "skill section truncated",
        ));
    }
    
    // If shorter than 30 bytes, pad with zeros for the internal model
    if end - start < SKILL_SECTION_LEN {
        let mut data = [0u8; SKILL_SECTION_LEN];
        let len = end - start;
        data[..len].copy_from_slice(&bytes[start..end]);
        Ok(SkillSection(data))
    } else {
        SkillSection::from_slice(&bytes[start..end])
    }
}

pub(crate) use crate::data::char_base_stats::class_code;

/// Finds the base skill ID for a given character class.
pub fn find_base_skill_id(class_code: &str) -> Option<u32> {
    crate::data::skills::SKILLS
        .iter()
        .find(|s| s.charclass == class_code)
        .map(|s| s.id)
}
