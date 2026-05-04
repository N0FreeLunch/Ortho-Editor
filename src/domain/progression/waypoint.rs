use crate::data::waypoints::{WaypointEntry, WAYPOINTS};

/// Alpha v105 waypoint sections include a 10-byte header before the waypoint payload.
pub const V105_WAYPOINT_PAYLOAD_START: usize = 10;
const DIFFICULTY_STRIDE_BITS: usize = 24 * 8;

#[derive(Clone, Copy)]
pub struct Waypoint {
    entry: &'static WaypointEntry,
    active: bool,
}

impl PartialEq for Waypoint {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.entry, other.entry) && self.active == other.active
    }
}

impl Eq for Waypoint {}

impl std::fmt::Debug for Waypoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waypoint")
            .field("act", &self.act())
            .field("index", &self.index())
            .field("name", &self.name())
            .field("active", &self.active)
            .field("ws_bit", &self.ws_bit())
            .finish()
    }
}

impl Waypoint {
    pub fn new(entry: &'static WaypointEntry, active: bool) -> Self {
        Self { entry, active }
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

    pub fn ws_bit(&self) -> u8 {
        self.entry.ws_bit
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaypointSet {
    difficulty: u8,
    waypoints: Vec<Waypoint>,
}

impl WaypointSet {
    pub fn new_empty(difficulty: u8) -> Self {
        let waypoints = WAYPOINTS.iter().map(|e| Waypoint::new(e, false)).collect();
        Self {
            difficulty,
            waypoints,
        }
    }

    pub fn from_bytes(bytes: &[u8], difficulty: u8, wp_anchor: usize) -> Self {
        let waypoints = WAYPOINTS
            .iter()
            .map(|entry| {
                // Ground absolute file offset to the payload start
                // Discussion 0230 verified waypoint anchor at 0x126 (294) from progression base.
                // 295 + 294 = 589.
                // The waypoint section has a 10-byte header.
                let absolute_payload_start = wp_anchor + V105_WAYPOINT_PAYLOAD_START;
                
                // Waypoint bits are packed by difficulty.
                let difficulty_bits = difficulty as usize * DIFFICULTY_STRIDE_BITS;
                let global_bit_idx = (absolute_payload_start * 8) + difficulty_bits + entry.ws_bit as usize;

                // from_bytes is called with a slice starting at V105_WAYPOINT_OFFSET (701).
                let section_start_bits = crate::domain::progression::axiom::V105_WAYPOINT_OFFSET * 8;
                let relative_bit_idx = if global_bit_idx >= section_start_bits {
                    global_bit_idx - section_start_bits
                } else {
                    0
                };

                let byte_idx = relative_bit_idx / 8;
                let bit_in_byte = relative_bit_idx % 8;
                
                let active = if byte_idx < bytes.len() {
                    (bytes[byte_idx] & (1 << bit_in_byte)) != 0
                } else {
                    false
                };
                Waypoint::new(entry, active)
            })
            .collect();
        Self {
            difficulty,
            waypoints,
        }
    }

    pub fn sync_to_bytes(&self, bytes: &mut [u8], wp_anchor: usize) {
        for wp in &self.waypoints {
            let absolute_payload_start = wp_anchor + V105_WAYPOINT_PAYLOAD_START;
            let difficulty_bits = self.difficulty as usize * DIFFICULTY_STRIDE_BITS;
            let global_bit_idx = (absolute_payload_start * 8) + difficulty_bits + wp.ws_bit() as usize;

            let section_start_bits = crate::domain::progression::axiom::V105_WAYPOINT_OFFSET * 8;
            let relative_bit_idx = if global_bit_idx >= section_start_bits {
                global_bit_idx - section_start_bits
            } else {
                0
            };

            let byte_idx = relative_bit_idx / 8;
            let bit_in_byte = relative_bit_idx % 8;
            
            if byte_idx < bytes.len() {
                if wp.is_active() {
                    bytes[byte_idx] |= 1 << bit_in_byte;
                } else {
                    bytes[byte_idx] &= !(1 << bit_in_byte);
                }
            }
        }
    }

    pub fn difficulty(&self) -> u8 {
        self.difficulty
    }

    pub fn waypoints(&self) -> &[Waypoint] {
        &self.waypoints
    }

    pub fn waypoints_mut(&mut self) -> &mut [Waypoint] {
        &mut self.waypoints
    }

    pub fn find_by_name(&self, name: &str) -> Option<Waypoint> {
        self.waypoints.iter().find(|w| w.name() == name).copied()
    }

    pub fn filter_by_act(&self, act: u8) -> Vec<Waypoint> {
        self.waypoints
            .iter()
            .filter(|w| w.act() == act)
            .copied()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct WaypointSection {
    pub raw_bytes: Vec<u8>,
}

impl WaypointSection {
    pub fn from_slice(slice: &[u8]) -> Self {
        WaypointSection {
            raw_bytes: slice.to_vec(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub fn set_activated(&mut self, byte_idx: usize, bit_idx: usize, active: bool) {
        if byte_idx < self.raw_bytes.len() {
            if active {
                self.raw_bytes[byte_idx] |= 1 << bit_idx;
            } else {
                self.raw_bytes[byte_idx] &= !(1 << bit_idx);
            }
        }
    }

    pub fn is_activated_by_name(&self, name: &str, difficulty: u8, wp_anchor: usize) -> bool {
        let set = WaypointSet::from_bytes(&self.raw_bytes, difficulty, wp_anchor);
        set.find_by_name(name).map(|w| w.is_active()).unwrap_or(false)
    }

    pub fn set_activated_by_name(&mut self, name: &str, difficulty: u8, active: bool, wp_anchor: usize) -> bool {
        let mut set = WaypointSet::from_bytes(&self.raw_bytes, difficulty, wp_anchor);
        if let Some(wp) = set.waypoints_mut().iter_mut().find(|w: &&mut Waypoint| w.name() == name) {
            wp.set_active(active);
            set.sync_to_bytes(&mut self.raw_bytes, wp_anchor);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waypoint_set_initialization() {
        let waypoint_set = WaypointSet::new_empty(0);
        assert!(!waypoint_set.waypoints().is_empty());
        
        let act1_town = waypoint_set.find_by_name("Act 1 - Town").expect("Should find Act 1 - Town");
        assert_eq!(act1_town.act(), 1);
        assert_eq!(act1_town.ws_bit(), 0);
        assert!(!act1_town.is_active());
    }

    #[test]
    fn test_waypoint_parsing() {
        let bytes = vec![0u8; 100];
        // Test with a dummy anchor (e.g., 295)
        let _ = WaypointSet::from_bytes(&bytes, 0, 295);
    }
}
