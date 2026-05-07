use std::io;

pub const SKILL_SECTION_LEN: usize = 30;

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
}

pub fn parse_skill_section(bytes: &[u8], if_pos: usize) -> io::Result<SkillSection> {
    let start = if_pos + 2;
    let end = start + SKILL_SECTION_LEN;
    if end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "skill section truncated",
        ));
    }
    SkillSection::from_slice(&bytes[start..end])
}
