use serde::{Serialize, Deserialize};

use crate::data::mercenary::{
    mercenary_class_name, mercenary_identity_label, mercenary_subtype_name,
};

/// Alpha v105 Mercenary State (Hybrid priority decoding).
///
/// Forensic evidence (Axiom 0328, 0366) shows that mercenary data is dual-localized:
/// - Experience: Always at Header Offset 171 (4B LE).
/// - Hireling ID: Priority to 'w4' NPC section (Offset 782+4), fallback to Header Offset 169.
/// - Act 3 Divergence: w4[4] contains Class ID (9), Header[169] contains Subtype (15, 16, 17).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
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

    /// Mercenary equipped items (Slice 4).
    pub equipment: MercenaryEquipment,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MercenaryEquipment {
    pub items: Vec<MercenaryEquipmentItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MercenaryEquipmentItem {
    pub code: String,
    pub location: u8,
    pub mode: u8,
    pub x: u8,
    pub y: u8,
}

impl MercenaryEquipmentItem {
    pub fn slot_name(&self) -> String {
        match self.location {
            1 => match self.x {
                1 => "Head",
                3 => "Torso",
                4 => "Right Hand",
                5 => "Left Hand",
                _ => "Equipped (Other)",
            },
            _ => "Unknown",
        }.to_string()
    }
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

            // Axiom 0380: The Offset 6 Anchor (Physical Class ID)
            class_id = w4_bytes.get(c_off).copied().unwrap_or(0);
            
            // Name ID: Usually a single byte at Offset 5
            name_id = w4_bytes.get(n_id_off).copied().map(|v| v as u16).unwrap_or(0);
        }

        // Alpha v105 mercenary class 0 is the only ambiguous slot; use header subtype as the tie-breaker.
        let hireling_id = if class_id == 0 {
            subtype_id // Return Header ID (1=Rogue, 8=Desert etc)
        } else {
            class_id // Return w4 Class ID (1=Iron Wolf, 9=Barbarian)
        };

        Self {
            hireling_id,
            class_id,
            subtype_id,
            experience,
            name_id,
            raw_w4,
            equipment: MercenaryEquipment::default(),
        }
    }

    /// Returns the localized class name from the mercenary bridge.
    pub fn class_name(&self) -> String {
        mercenary_class_name(self.class_id, self.hireling_id).to_string()
    }

    /// Returns the subtype name (e.g. element for Act 3 Iron Wolves).
    pub fn subtype_name(&self) -> String {
        mercenary_subtype_name(self.class_id, self.subtype_id)
    }

    /// Records forensic evidence about the mercenary state to the audit.
    pub fn record_forensics(&self, audit: &mut crate::domain::item::axiom_meta::ForensicAudit) {
        use crate::domain::item::axiom_meta::{ForensicMetadata, Confidence, Intentionality};
        
        let name = self.class_name();
        let expected_lvl = self.expected_level();
        audit.record(ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            format!(
                "Alpha v105 Mercenary identified as {} ({})",
                name,
                mercenary_identity_label(self.class_id, self.hireling_id, expected_lvl)
            ),
        ));

        if !self.equipment.items.is_empty() {
            audit.record(ForensicMetadata::new(
                Confidence::VerifiedTruth,
                Intentionality::Structural,
                format!("Mercenary Equipment: {} items detected", self.equipment.items.len()),
            ));
            for it in &self.equipment.items {
                audit.record(ForensicMetadata::new(
                    Confidence::VerifiedTruth,
                    Intentionality::Structural,
                    format!("  - {} equipped in {}", it.code.trim(), it.slot_name()),
                ));
            }
        }
    }

    /// Calculates the expected level based on experience using the verified Amazon XP table.
    /// Alpha v105 Truth: Mercenaries use the Amazon XP table (Axiom 0484).
    pub fn expected_level(&self) -> u8 {
        const XP_TABLE: [u32; 99] = [
            500, 1500, 3750, 7875, 14175, 22680, 32886, 44396, 57715, 72144, 90180, 112725, 140906,
            176132, 220165, 275207, 344008, 430010, 537513, 671891, 839864, 1049830, 1312287,
            1640359, 2050449, 2563061, 3203826, 3902260, 4663553, 5493363, 6397855, 7383752,
            8458379, 9629723, 10906488, 12298162, 13815086, 15468534, 17270791, 19235252, 21376515,
            23710491, 26254525, 29027522, 32050088, 35344686, 38935798, 42850109, 47116709,
            51767302, 56836449, 62361819, 68384473, 74949165, 82104680, 89904191, 98405658,
            107672256, 117772849, 128782495, 140783010, 153863570, 168121381, 183662396, 200602101,
            219066380, 239192444, 261129853, 285041630, 311105466, 339515048, 370481492, 404234916,
            441026148, 481128591, 524840254, 572485967, 624419793, 681027665, 742730244, 809986056,
            883294891, 963201521, 1050299747, 1145236814, 1248718217, 1361512946, 1484459201,
            1618470619, 1764543065, 1923762030, 2097310703, 2286478756, 2492671933, 2717422497,
            2962400612, 3229426756, 3520485254, 3837739017,
        ];

        let mut level = 1;
        for &threshold in &XP_TABLE {
            if self.experience >= threshold {
                level += 1;
            } else {
                break;
            }
        }
        level.min(99)
    }

    /// Legacy decoder (w4-only). Prefer `from_hybrid`.
    pub fn from_w4(bytes: &[u8]) -> Self {
        let header = [0u8; 175]; // Dummy header for legacy compat if needed, but better use hybrid.
        Self::from_hybrid(&header, Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::MercenaryState;

    #[test]
    fn mercenary_class_name_matches_alpha_v105_mapping() {
        let rogue = MercenaryState {
            class_id: 0,
            hireling_id: 1,
            ..Default::default()
        };
        assert_eq!(rogue.class_name(), "Rogue (Act 1)");

        let desert = MercenaryState {
            class_id: 0,
            hireling_id: 8,
            ..Default::default()
        };
        assert_eq!(desert.class_name(), "Desert Warrior (Act 2)");

        let iron_wolf = MercenaryState {
            class_id: 1,
            hireling_id: 1,
            subtype_id: 16,
            ..Default::default()
        };
        assert_eq!(iron_wolf.class_name(), "Iron Wolf");
        assert_eq!(iron_wolf.subtype_name(), "Cold");

        let barbarian = MercenaryState {
            class_id: 9,
            hireling_id: 9,
            ..Default::default()
        };
        assert_eq!(barbarian.class_name(), "Barbarian");
        assert_eq!(barbarian.subtype_name(), "N/A");
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
