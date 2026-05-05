use d2r_core::domain::progression::quest::QuestSet;
use d2r_core::domain::progression::waypoint::WaypointSet;
use d2r_core::item::HuffmanTree;
use d2r_core::save::{
    ExpansionSection, Save, WaypointSection, rebuild_status_and_player_items,
};
use std::fs;
use std::path::PathBuf;

fn repo_path(relative: &str) -> PathBuf {
    let _ = dotenvy::dotenv();
    let base = std::env::var("D2R_CORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    base.join(relative)
}

fn assert_symmetry_integrity(
    section_name: &str,
    actual: &[u8],
    expected: &[u8],
    allowed_bit_ranges: &[(usize, usize)],
) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{} section length mismatch",
        section_name
    );

    for (i, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e {
            let diff = a ^ e;
            for bit in 0..8 {
                if (diff >> bit) & 1 == 1 {
                    let bit_offset = i * 8 + bit;
                    let is_allowed = allowed_bit_ranges
                        .iter()
                        .any(|&(start, end)| bit_offset >= start && bit_offset <= end);

                    if !is_allowed {
                        panic!(
                            "Unintended drift in {} section at bit offset {} (byte {}, bit {}). Expected bit {}, got {}",
                            section_name, bit_offset, i, bit, (e >> bit) & 1, (a >> bit) & 1
                        );
                    }
                }
            }
        }
    }
}

/// Verification Rule: Only Known bits should change.
/// This matches the 'Semantic Axis' of the Dual-Axis Verification Paradigm.
fn assert_no_unknown_drift(actual: &[u8], expected: &[u8], known_bits: &[usize]) {
    for (i, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e {
            let diff = a ^ e;
            for bit in 0..8 {
                if (diff >> bit) & 1 == 1 {
                    let abs_bit = i * 8 + bit;
                    if !known_bits.contains(&abs_bit) {
                        panic!(
                            "ENVIRONMENT FLAG DRIFT: Bit {} (byte {}, bit {}) changed from {} to {} but is not in the Known whitelist.",
                            abs_bit, i, bit, (e >> bit) & 1, (a >> bit) & 1
                        );
                    }
                }
            }
        }
    }
}

/// Verification Rule: A mutated file must be bit-perfectly identical after a roundtrip rebuild.
/// This ensures serialization symmetry and stability.
fn assert_rebuild_symmetry(original_bytes: &[u8], huffman: &HuffmanTree, version: u32) -> std::io::Result<()> {
    let save = Save::from_bytes(original_bytes)?;
    
    // Extract items to ensure full rebuild surface
    let items = d2r_core::item::Item::read_player_items(original_bytes, huffman, version == 105)?;
    
    let rebuilt = rebuild_status_and_player_items(
        original_bytes,
        None, // attributes
        None, // skills
        save.header.quests.as_ref(),
        save.header.waypoints.as_ref(),
        save.header.expansion.as_ref(),
        &items,
        huffman,
    )?;

    if rebuilt != original_bytes {
        // Find first diff for better error message
        for i in 0..rebuilt.len().min(original_bytes.len()) {
            if rebuilt[i] != original_bytes[i] {
                panic!("REBUILD ASYMMETRY: First difference at byte {} (0x{:02X}). Expected 0x{:02X}, got 0x{:02X}", 
                    i, i, original_bytes[i], rebuilt[i]);
            }
        }
        if rebuilt.len() != original_bytes.len() {
            panic!("REBUILD ASYMMETRY: Length mismatch. Expected {}, got {}", original_bytes.len(), rebuilt.len());
        }
    }
    
    Ok(())
}

#[test]
fn test_alpha_v105_progression_mutation_verification() -> std::io::Result<()> {
    let path = repo_path("tests/fixtures/savegames/original/amazon_empty.d2s");
    let bytes = fs::read(path).expect("fixture should be readable");
    let huffman = HuffmanTree::new();

    // 1. Initial State Check
    let save = Save::from_bytes(&bytes)?;
    assert_eq!(save.header.version, 105);

    let mut wps_section = save
        .header
        .waypoints
        .clone()
        .expect("v105 should have waypoints");
    let mut quests_section = save.header.quests.clone().expect("v105 should have quests");
    let mut expansion = save
        .header
        .expansion
        .clone()
        .expect("v105 should have expansion");

    // Capture "before" state for no-collateral verification
    let wps_before = wps_section.raw_bytes.clone();
    let quests_before = quests_section.raw_bytes.clone();

    // 2. Perform Mutations via Domain Models
    
    // 2a. Waypoint Mutation
    let wp_anchor = 589; // 295 + 294
    let mut wp_set = WaypointSet::from_bytes(&wps_section.raw_bytes, 0, wp_anchor);
    {
        let name = "Act 1 - Wilderness 2";
        let mut wp = wp_set.find_by_name(name).expect("Should find Cold Plains (Wilderness 2)");
        assert!(!wp.is_active());
        wp.set_active(true);
        // Note: find_by_name returns a copy, so we need to update the set if it's not a reference
        // Actually, WaypointSet::waypoints_mut gives access
        let wp_mut = wp_set.waypoints_mut().iter_mut().find(|w| w.name() == name).unwrap();
        wp_mut.set_active(true);
    }
    wp_set.sync_to_bytes(&mut wps_section.raw_bytes, wp_anchor);

    // 2b. Quest Mutation
    let normal_anchor = 415; // 295 + 120
    let act5_anchor = 439;  // 295 + 144
    let mut q_set = QuestSet::from_v105_bytes(&quests_section.raw_bytes, normal_anchor, act5_anchor);
    {
        let den = q_set.find_by_name("Den of Evil").expect("Should find Den of Evil");
        assert!(!den.is_completed());
        let q_mut = q_set.quests_mut().iter_mut().find(|q| q.name() == "Den of Evil").unwrap();
        q_mut.set_completed(true);
    }
    q_set.sync_to_v105_bytes(&mut quests_section.raw_bytes, normal_anchor, act5_anchor);

    // 2c. Expansion (kept as raw for now as no domain model exists)
    expansion.raw_bytes[5] = 0xFF;

    // 3. No-Collateral Verification (Inside Sections)
    
    // Waypoint: "Act 1 - Wilderness 2" is ws_bit 1 (Act 1). 
    // Normal starts at bit 80 (byte 10). bit 81 is byte 10, bit 1.
    // 0x01 (Act 1 Town) | 0x02 (Cold Plains) = 0x03
    assert_eq!(wps_section.raw_bytes[10], 0x03);
    assert_symmetry_integrity(
        "Waypoint",
        &wps_section.raw_bytes,
        &wps_before,
        &[(81, 81)], // Only bit 81 should change (Normal Cold Plains)
    );

    // Quest: "Den of Evil" is at offset 12 in raw_bytes (415 - 403 = 12)
    // set_completed(true) sets bit 0 (byte 12) and bit 12 (0x10 in byte 13)
    assert_eq!(quests_section.raw_bytes[12], 0x01);
    assert_eq!(quests_section.raw_bytes[13], 0x10);
    
    // Quest Section Marker: "Woo!" (0x193 = 403)
    assert_eq!(quests_section.raw_bytes[0], 0x57); // 'W'
    assert_symmetry_integrity(
        "Quest",
        &quests_section.raw_bytes,
        &quests_before,
        &[(96, 96), (108, 108)], // Byte 12 bit 0, Byte 13 bit 4
    );

    // Expansion Mutation Verification
    assert_symmetry_integrity(
        "Expansion",
        &expansion.raw_bytes,
        &save.header.expansion.as_ref().unwrap().raw_bytes,
        &[(40, 47)], // Byte 5 (8 * 5 = 40)
    );

    // 4. Rebuild Save
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items = d2r_core::item::Item::read_player_items(&bytes, &huffman, version == 105)?;
    let rebuilt = rebuild_status_and_player_items(
        &bytes,
        None, // attributes
        None, // skills
        Some(&quests_section),
        Some(&wps_section),
        Some(&expansion),
        &items,
        &huffman,
    )?;

    // 5. Verify rebuilt save matches expectations
    // Quests: Den of Evil is at 415 (0x19F)
    assert_eq!(rebuilt[0x19F], 0x01);
    assert_eq!(rebuilt[0x1A0], 0x10);
    // Waypoints: 0x2BD + 10 = 0x2C7
    assert_eq!(rebuilt[0x2C7], 0x03);

    // 6. Verify roundtrip parsing
    let save_back = Save::from_bytes(&rebuilt)?;
    assert_eq!(save_back.header.version, 105);
    assert_eq!(save_back.header.quests.as_ref().unwrap().raw_bytes[12], 0x01);
    assert_eq!(save_back.header.quests.as_ref().unwrap().raw_bytes[13], 0x10);
    assert_eq!(save_back.header.waypoints.as_ref().unwrap().raw_bytes[10], 0x03);

    Ok(())
}

#[test]
fn test_alpha_v105_act2_transition_integrity() -> std::io::Result<()> {
    let path = repo_path("tests/fixtures/savegames/original/amazon_empty.d2s");
    let bytes = fs::read(path).expect("fixture should be readable");
    let huffman = HuffmanTree::new();
    let save = Save::from_bytes(&bytes)?;
    let version = save.header.version;

    // 1. Prepare Sections
    let mut quests_section = save.header.quests.clone().expect("v105 should have quests");
    let mut wps_section = save.header.waypoints.clone().expect("v105 should have waypoints");

    // 2. Perform Mutation: Complete Act 1, Unlock Act 2 Town
    
    // 2a. Quest: Sisters to the Slaughter (Normal)
    let mut q_set = QuestSet::from_v105_bytes(&quests_section.raw_bytes, 415, 439);
    {
        let q = q_set.quests_mut().iter_mut()
            .find(|q| q.difficulty() == 0 && q.act() == 1 && q.index() == 5)
            .expect("Should find Sisters to the Slaughter");
        q.set_completed(true);
    }
    q_set.sync_to_v105_bytes(&mut quests_section.raw_bytes, 415, 439);

    // 2b. Waypoint: Act 2 - Town (Normal)
    let wp_anchor = 589;
    let mut wp_set = WaypointSet::from_bytes(&wps_section.raw_bytes, 0, wp_anchor);
    {
        let wp = wp_set.waypoints_mut().iter_mut()
            .find(|w| w.name() == "Act 2 - Town")
            .expect("Should find Act 2 - Town");
        wp.set_active(true);
    }
    wp_set.sync_to_bytes(&mut wps_section.raw_bytes, wp_anchor);

    // 3. Rebuild
    let items = d2r_core::item::Item::read_player_items(&bytes, &huffman, version == 105)?;
    let mutated_bytes = rebuild_status_and_player_items(
        &bytes,
        None, // attributes
        None, // skills
        Some(&quests_section),
        Some(&wps_section),
        None, // expansion
        &items,
        &huffman,
    )?;

    // 4. Dual-Axis Verification
    
    // Axis 1: Semantic Whitelist (No unknown bit drift)
    // Based on forge scouter analysis and domain model logic:
    // Quest Q6: 3400, 3403, 3404, 3412 (Seen in code), 3415 (Seen in fixture)
    // Waypoint A2T: 5697
    let known_bits = vec![3400, 3403, 3404, 3412, 3415, 5697];
    
    // We filter out the checksum bits (64..96) and version/size bits from the comparison
    // to focus on the 'Semantic Drift' in the progression domain.
    let mut actual_progression = mutated_bytes.clone();
    let mut expected_progression = bytes.clone();
    
    // Mask metadata (checksum, size) to avoid noise in drift check
    for i in 8..16 { 
        actual_progression[i] = 0;
        expected_progression[i] = 0;
    }

    assert_no_unknown_drift(&actual_progression, &expected_progression, &known_bits);

    // Axis 2: Rebuild Symmetry (Stability)
    assert_rebuild_symmetry(&mutated_bytes, &huffman, version)?;

    Ok(())
}

#[test]
fn test_alpha_v105_waypoint_name_mapping() -> std::io::Result<()> {
    let mut wps = WaypointSection::from_slice(&[0u8; 80]);
    let mut ex = ExpansionSection::from_slice(&[0u8; 80]);

    // Act 1 Town (Index 0 in Act 1 Block, ws_bit 0)
    let wp_anchor = 589;
    wps.set_activated_by_name("Act 1 - Town", 0, true, wp_anchor);
    ex.set_activated_by_name("Act 1 - Town", 0, true); // ExpansionSection uses difficulty as difficulty?

    assert_eq!(wps.raw_bytes[10], 0x01);
    assert_eq!(ex.raw_bytes[34], 0x01);

    // Act 2 Town (Index 0 in Act 2 Block, ws_bit 9)
    wps.set_activated_by_name("Act 2 - Town", 0, true, wp_anchor);
    ex.set_activated_by_name("Act 2 - Town", 0, true);

    assert_eq!(wps.raw_bytes[11], 0x02);
    assert_eq!(ex.raw_bytes[35], 0x02);

    Ok(())
}
