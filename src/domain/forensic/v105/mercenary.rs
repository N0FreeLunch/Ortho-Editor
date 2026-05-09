/// Alpha v105 Mercenary State (Hybrid priority decoding).
///
/// Forensic evidence (Axiom 0328) shows that mercenary data is dual-localized:
/// - Experience: Always at Header Offset 171 (4B LE).
/// - Hireling ID: Priority to 'w4' NPC section (Offset 782+4), fallback to Header Offset 169.
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
    /// Creates a new state using a hybrid priority localization logic (Axiom 0328).
    ///
    /// Mode A (Header): Experience is always at header[171..175]. ID at header[169] if w4 missing.
    /// Mode B (w4): If w4 exists and Hireling ID (w4[4]) is non-zero, it takes priority for ID.
    pub fn from_hybrid(header: &[u8], w4: Option<&[u8]>) -> Self {
        // 1. Experience: Always from fixed header Offset 171 (4B LE)
        let experience = if header.len() >= 175 {
            u32::from_le_bytes(header[171..175].try_into().unwrap_or([0; 4]))
        } else {
            0
        };

        // 2. Hireling ID: Priority to w4[4] if non-zero, otherwise Header[169]
        let mut hireling_id = 0;
        let mut raw_w4 = Vec::new();
        let mut name_id = 0;

        if let Some(w4_bytes) = w4 {
            raw_w4 = w4_bytes.to_vec();
            let w4_id = w4_bytes.get(4).copied().unwrap_or(0);
            if w4_id != 0 {
                hireling_id = w4_id;
            }
            
            if w4_bytes.len() >= 29 {
                name_id = u16::from_le_bytes(w4_bytes[27..29].try_into().unwrap_or([0; 2]));
            }
        }

        // Fallback to Header ID if still 0
        if hireling_id == 0 && header.len() >= 170 {
            hireling_id = header[169];
        }

        Self {
            hireling_id,
            experience,
            name_id,
            raw_w4,
        }
    }

    /// Legacy decoder (w4-only). Prefer `from_hybrid`.
    pub fn from_w4(bytes: &[u8]) -> Self {
        let header = [0u8; 175]; // Dummy header for legacy compat if needed, but better use hybrid.
        Self::from_hybrid(&header, Some(bytes))
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
