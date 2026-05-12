/// Alpha v105 Mercenary State (Hybrid priority decoding).
///
/// Forensic evidence (Axiom 0328, 0366) shows that mercenary data is dual-localized:
/// - Experience: Always at Header Offset 171 (4B LE).
/// - Hireling ID: Priority to 'w4' NPC section (Offset 782+4), fallback to Header Offset 169.
/// - Act 3 Divergence: w4[4] contains Class ID (9), Header[169] contains Subtype (15, 16, 17).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MercenaryState {
    /// Generic Hireling ID. 
    /// Legacy: Equal to subtype_id or w4_id.
    pub hireling_id: u8,

    /// Hireling Class ID from w4[4].
    /// Iron Wolf: 9.
    pub class_id: u8,

    /// Persistent Subtype/Element ID from Header[169].
    /// Fire: 15, Cold: 16, Lightning: 17.
    pub subtype_id: u8,
    
    /// Mercenary Experience (at Header Offset 171, 32-bit LE).
    pub experience: u32,
    
    /// Mercenary Name ID (tentative).
    /// Note: w4[27] often contains HP data (e.g. 248) in Alpha v105.
    pub name_id: u16,

    /// Raw w4 bytes for forensic preservation.
    pub raw_w4: Vec<u8>,
}

impl MercenaryState {
    /// Creates a new state using a hybrid priority localization logic (Axiom 0328, 0366).
    ///
    /// Mode A (Header): Experience is at [171..175]. Subtype is at [169].
    /// Mode B (w4): If w4 exists and Hireling ID (w4[4]) is non-zero, it defines the class.
    pub fn from_hybrid(header: &[u8], w4: Option<&[u8]>) -> Self {
        // 1. Experience: Always from fixed header Offset 171 (4B LE)
        let experience = if header.len() >= 175 {
            u32::from_le_bytes(header[171..175].try_into().unwrap_or([0; 4]))
        } else {
            0
        };

        // 2. Subtype: Always from fixed header Offset 169
        let subtype_id = if header.len() >= 170 {
            header[169]
        } else {
            0
        };

        // 3. Hireling IDs: Priority to w4 section
        let mut class_id = 0;
        let mut raw_w4 = Vec::new();
        let mut name_id = 0;

        if let Some(w4_bytes) = w4 {
            raw_w4 = w4_bytes.to_vec();
            
            // Detect if marker 'w4' is included to handle both raw sections and stripped payloads.
            let has_marker = w4_bytes.starts_with(b"w4");
            let c_off = if has_marker { 6 } else { 4 }; // Class ID is 4 bytes after marker
            let n_id_off = if has_marker { 5 } else { 3 }; // Name ID is 3 bytes after marker

            class_id = w4_bytes.get(c_off).copied().unwrap_or(0);
            
            // Name ID: Usually a single byte in this context? 
            // Or part of a larger field. For now, take the verified index 5.
            name_id = w4_bytes.get(n_id_off).copied().map(|v| v as u16).unwrap_or(0);
        }

        // Hireling ID logic (Axiom 0366): 
        // In Alpha v105, Header[169] is the persistent attribute/subtype ID.
        // For Act 3 (Class 9), this is the elemental variant (15=Fire, 16=Cold, 17=Lightning).
        let hireling_id = subtype_id;

        Self {
            hireling_id,
            class_id,
            subtype_id,
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
