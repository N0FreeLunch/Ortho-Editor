/// Alpha v105 Mercenary State (decoded from 'w4' NPC data section).
///
/// Forensic evidence shows that mercenary attributes (Level, XP, Type) are stored
/// within the 'w4' section (Offset 782) rather than the 'kf/lf' footer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MercenaryState {
    /// Hireling ID from Hireling.txt (at w4 + 4).
    /// Rogue: 0, Desert: 140, Barbarian: 174.
    pub hireling_id: u8,
    
    /// Mercenary Experience (at w4 + 6, 32-bit LE).
    pub experience: u32,
    
    /// Mercenary Name ID (at w4 + 27, tentative).
    pub name_id: u16,

    /// Raw w4 bytes for forensic preservation.
    pub raw_w4: Vec<u8>,
}

impl MercenaryState {
    /// Creates a new state from the raw 'w4' section bytes.
    pub fn from_w4(bytes: &[u8]) -> Self {
        let hireling_id = bytes.get(4).copied().unwrap_or(0);
        let experience = if bytes.len() >= 10 {
            u32::from_le_bytes(bytes[6..10].try_into().unwrap_or([0; 4]))
        } else {
            0
        };
        let name_id = if bytes.len() >= 29 {
            u16::from_le_bytes(bytes[27..29].try_into().unwrap_or([0; 2]))
        } else {
            0
        };

        Self {
            hireling_id,
            experience,
            name_id,
            raw_w4: bytes.to_vec(),
        }
    }
}

/// Alpha v105 Mercenary Footer (kf/lf envelope).
///
/// This 9-byte sequence is a static structural anchor found at the end of JM #2.
/// Value: [b'k', b'f', 0x00, 0x01, 0x00, b'l', b'f', 0x00, 0x00]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MercenaryFooter {
    pub raw: [u8; 9],
}

impl MercenaryFooter {
    pub const STATIC_PAYLOAD: [u8; 9] = [b'k', b'f', 0x00, 0x01, 0x00, b'l', b'f', 0x00, 0x00];

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut raw = [0u8; 9];
        let len = bytes.len().min(9);
        raw[..len].copy_from_slice(&bytes[..len]);
        Self { raw }
    }

    pub fn is_standard(&self) -> bool {
        self.raw == Self::STATIC_PAYLOAD
    }
}
