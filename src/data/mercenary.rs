/// Alpha v105 mercenary naming bridge.
///
/// This keeps the audit's mercenary labels local to `d2r-core` so the verifier does
/// not depend on the broader forensic registry for class names that have already
/// drifted from the current fixture truth.

pub fn mercenary_class_name(class_id: u8, hireling_id: u8) -> &'static str {
    match class_id {
        0 => {
            if hireling_id >= 8 {
                "Desert Warrior (Act 2)"
            } else {
                "Rogue (Act 1)"
            }
        }
        1 => "Iron Wolf",
        9 => "Barbarian",
        _ => "Unknown Mercenary",
    }
}

pub fn mercenary_subtype_name(class_id: u8, subtype_id: u8) -> String {
    match class_id {
        1 => match subtype_id {
            15 => "Fire".to_string(),
            16 => "Cold".to_string(),
            17 => "Lightning".to_string(),
            _ => format!("Unknown Element({})", subtype_id),
        },
        _ => "N/A".to_string(),
    }
}

pub fn mercenary_identity_label(class_id: u8, hireling_id: u8, expected_level: u8) -> String {
    format!(
        "ClassID={}, HirelingID={}, ExpectedLevel={}",
        class_id, hireling_id, expected_level
    )
}
