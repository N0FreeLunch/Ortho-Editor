use d2r_core::item::{HuffmanTree, Item};
use d2r_core::domain::forensic::v105::axioms::V105PropertyWidthAxiom;
use d2r_core::domain::vo::{InventoryCoordinate, InventoryPlacement, ItemSize};
use d2r_core::save::{
    apply_save_side_coordinated_relocation, AttributeSection, classify_item_slot,
    collect_player_slots, map_core_sections, parse_quest_section, parse_skill_section,
    ItemSlotClass,
    try_apply_save_side_coordinated_relocation,
    rebuild_status_and_player_items,
};
use std::fs;
use std::io;

mod common;
use common::repo_path;

fn load_fixture(path: &str) -> io::Result<Vec<u8>> {
    fs::read(repo_path(path))
}

#[test]
fn status_and_stash_roundtrip_fixtures() -> io::Result<()> {
    let fixtures = [
        "tests/fixtures/savegames/original/amazon_empty.d2s",
        "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
    ];
    let huffman = HuffmanTree::new();
    for fixture in fixtures {
        let bytes = load_fixture(fixture)?;
        let map = map_core_sections(&bytes)?;
        let attributes = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;
        let skills = parse_skill_section(&bytes, &map)?;
        let quests = parse_quest_section(&bytes, &map)?;
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let items = Item::read_player_items(&bytes, &huffman, version == 105)?;
        let rebuilt = rebuild_status_and_player_items(
            &bytes,
            Some(&attributes),
            Some(&skills),
            Some(&quests),
            None,
            None,
            &items,
            &huffman,
        )?;
        let rebuilt_map = map_core_sections(&rebuilt)?;
        let rebuilt_attributes = AttributeSection::parse(&rebuilt, rebuilt_map.gf_pos, rebuilt_map.if_pos)?;
        let rebuilt_skills = parse_skill_section(&rebuilt, &rebuilt_map)?;
        let rebuilt_quests = parse_quest_section(&rebuilt, &rebuilt_map)?;
        let rebuilt_slots = collect_player_slots(&rebuilt, &huffman)?;

        assert_eq!(rebuilt_attributes.raw_bytes, attributes.raw_bytes);
        assert_eq!(rebuilt_skills.as_slice(), skills.as_slice());
        assert_eq!(rebuilt_quests.as_slice(), quests.as_slice());
        assert!(
            rebuilt_slots
                .iter()
                .any(|(_, class)| *class == ItemSlotClass::InventoryLike)
        );
    }
    Ok(())
}

#[test]
fn relocation_mutation_same_owner_roundtrip() -> io::Result<()> {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s")?;
    let huffman = HuffmanTree::new();
    let map = map_core_sections(&bytes)?;
    let attributes = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;
    let skills = parse_skill_section(&bytes, &map)?;
    let quests = parse_quest_section(&bytes, &map)?;
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let mut items = Item::read_player_items(&bytes, &huffman, version == 105)?;

    let target_idx = items
        .iter()
        .position(|item| {
            matches!(
                classify_item_slot(item),
                ItemSlotClass::InventoryLike | ItemSlotClass::StashLike
            )
        })
        .expect("fixture should contain an inventory-like or stash-like item");

    let original_class = classify_item_slot(&items[target_idx]);
    let original_x = items[target_idx].x;
    let original_y = items[target_idx].y;
    let target_code = items[target_idx].code.clone();
    let target_location = items[target_idx].location;
    let target_page = items[target_idx].page;
    let target_mode = items[target_idx].mode;
    let new_x = if original_x == 0 { 1 } else { original_x - 1 };
    let new_y = if original_y == 0 { 1 } else { original_y - 1 };

    let placement = InventoryPlacement::new(
        InventoryCoordinate::new(new_x, new_y).expect("mutated coordinate should stay in bounds"),
        ItemSize::new(1, 1).expect("1x1 placement should be valid"),
    )
    .expect("mutated placement should stay within the grid");
    items[target_idx].set_placement(placement);

    let rebuilt = rebuild_status_and_player_items(
        &bytes,
        Some(&attributes),
        Some(&skills),
        Some(&quests),
        None,
        None,
        &items,
        &huffman,
    )?;

    let reparsed_items = Item::read_player_items(&rebuilt, &huffman, version == 105)?;

    let roundtripped = reparsed_items
        .iter()
        .find(|item| {
            item.code.trim() == target_code.trim()
                && item.location == target_location
                && item.page == target_page
                && item.mode == target_mode
                && classify_item_slot(item) == original_class
        })
        .expect("mutated item should survive rebuild and readback");
    assert_eq!((roundtripped.x, roundtripped.y), (new_x, new_y));
    assert_eq!(roundtripped.body.x, new_x);
    assert_eq!(roundtripped.body.y, new_y);
    assert_eq!(classify_item_slot(roundtripped), original_class);

    assert_ne!((original_x, original_y), (new_x, new_y));
    Ok(())
}

#[test]
fn relocation_mutation_owner_bucket_reclassification_roundtrip() -> io::Result<()> {
    let huffman = HuffmanTree::new();
    let w_axiom = V105PropertyWidthAxiom::default();

    let candidate_fixtures = [
        "tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s",
        "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
        "tests/fixtures/savegames/original/amazon_initial.d2s",
        "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
    ];

    let mut selected = None;
    for fixture in candidate_fixtures {
        let bytes = load_fixture(fixture)?;
        let map = map_core_sections(&bytes)?;
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        if version != 105 {
            continue;
        }

        let attributes = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;
        let skills = parse_skill_section(&bytes, &map)?;
        let quests = parse_quest_section(&bytes, &map)?;
        let items = Item::read_player_items(&bytes, &huffman, true)?;

        if let Some(target_idx) = items.iter().position(|item| {
            matches!(
                classify_item_slot(item),
                ItemSlotClass::InventoryLike | ItemSlotClass::EquipmentLike
            ) && !item.header.is_compact
                && !item.is_opaque()
                && !w_axiom.is_summary_item(5, &item.code)
        }) {
            selected = Some((bytes, attributes, skills, quests, items, target_idx));
            break;
        }
    }

    let (bytes, attributes, skills, quests, mut items, target_idx) = selected
        .expect("fixture should contain a non-opaque owner-bucket candidate");

    let original_class = classify_item_slot(&items[target_idx]);
    let original_location = items[target_idx].location;
    let original_mode = items[target_idx].mode;
    let target_code = items[target_idx].code.clone();

    items[target_idx].set_owner_bucket(4, original_location, original_mode);

    assert_ne!(original_class, ItemSlotClass::StashLike);
    assert_eq!(classify_item_slot(&items[target_idx]), ItemSlotClass::StashLike);

    let rebuilt = rebuild_status_and_player_items(
        &bytes,
        Some(&attributes),
        Some(&skills),
        Some(&quests),
        None,
        None,
        &items,
        &huffman,
    )?;

    let reparsed_items = Item::read_player_items(&rebuilt, &huffman, true)?;
    let roundtripped = reparsed_items
        .iter()
        .find(|item| {
            item.code.trim() == target_code.trim()
                && item.page == 4
                && item.location == original_location
                && item.mode == original_mode
                && classify_item_slot(item) == ItemSlotClass::StashLike
        })
        .expect("reclassified item should survive rebuild and readback");
    assert_eq!(roundtripped.page, 4);
    assert_eq!(roundtripped.body.page, 4);
    assert_eq!(roundtripped.body.location, original_location);
    assert_eq!(roundtripped.body.mode, original_mode);
    assert_eq!(classify_item_slot(roundtripped), ItemSlotClass::StashLike);

    Ok(())
}

#[test]
fn relocation_mutation_cross_owner_helper_roundtrip() -> io::Result<()> {
    let huffman = HuffmanTree::new();
    let w_axiom = V105PropertyWidthAxiom::default();

    let candidate_fixtures = [
        "tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s",
        "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
        "tests/fixtures/savegames/original/amazon_initial.d2s",
        "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
    ];

    let mut selected = None;
    for fixture in candidate_fixtures {
        let bytes = load_fixture(fixture)?;
        let map = map_core_sections(&bytes)?;
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        if version != 105 {
            continue;
        }

        let attributes = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;
        let skills = parse_skill_section(&bytes, &map)?;
        let quests = parse_quest_section(&bytes, &map)?;
        let items = Item::read_player_items(&bytes, &huffman, true)?;

        if let Some(target_idx) = items.iter().position(|item| {
            matches!(classify_item_slot(item), ItemSlotClass::InventoryLike)
                && !item.header.is_compact
                && !item.is_opaque()
                && !w_axiom.is_summary_item(5, &item.code)
        }) {
            selected = Some((bytes, attributes, skills, quests, items, target_idx));
            break;
        }
    }

    let (bytes, attributes, skills, quests, mut items, target_idx) = selected
        .expect("fixture should contain a non-opaque inventory-like candidate");

    let original_class = classify_item_slot(&items[target_idx]);
    assert_eq!(original_class, ItemSlotClass::InventoryLike);

    let original_location = items[target_idx].location;
    let original_mode = items[target_idx].mode;
    let original_x = items[target_idx].x;
    let original_y = items[target_idx].y;
    let target_code = items[target_idx].code.clone();

    let new_x = if original_x == 0 { 1 } else { original_x - 1 };
    let new_y = if original_y == 0 { 1 } else { original_y - 1 };
    let placement = InventoryPlacement::new(
        InventoryCoordinate::new(new_x, new_y).expect("mutated coordinate should stay in bounds"),
        ItemSize::new(1, 1).expect("1x1 placement should be valid"),
    )
    .expect("mutated placement should stay within the grid");

    apply_save_side_coordinated_relocation(
        &mut items[target_idx],
        placement,
        4,
        original_location,
        original_mode,
    );

    assert_eq!(classify_item_slot(&items[target_idx]), ItemSlotClass::StashLike);

    let rebuilt = rebuild_status_and_player_items(
        &bytes,
        Some(&attributes),
        Some(&skills),
        Some(&quests),
        None,
        None,
        &items,
        &huffman,
    )?;

    let reparsed_items = Item::read_player_items(&rebuilt, &huffman, true)?;
    let roundtripped = reparsed_items
        .iter()
        .find(|item| {
            item.code.trim() == target_code.trim()
                && item.page == 4
                && item.location == original_location
                && item.mode == original_mode
                && classify_item_slot(item) == ItemSlotClass::StashLike
        })
        .expect("cross-owner item should survive rebuild and readback");

    assert_eq!((roundtripped.x, roundtripped.y), (new_x, new_y));
    assert_eq!(roundtripped.body.x, new_x);
    assert_eq!(roundtripped.body.y, new_y);
    assert_eq!(roundtripped.page, 4);
    assert_eq!(roundtripped.body.page, 4);
    assert_eq!(roundtripped.body.location, original_location);
    assert_eq!(roundtripped.body.mode, original_mode);
    assert_eq!(classify_item_slot(roundtripped), ItemSlotClass::StashLike);

    assert_ne!((original_x, original_y), (new_x, new_y));
    Ok(())
}

#[test]
fn relocation_failure_leaves_owner_and_placement_unchanged() -> io::Result<()> {
    let huffman = HuffmanTree::new();
    let w_axiom = V105PropertyWidthAxiom::default();

    let candidate_fixtures = [
        "tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s",
        "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
        "tests/fixtures/savegames/original/amazon_initial.d2s",
        "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
    ];

    let mut selected = None;
    for fixture in candidate_fixtures {
        let bytes = load_fixture(fixture)?;
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        if version != 105 {
            continue;
        }

        let items = Item::read_player_items(&bytes, &huffman, true)?;

        if let Some(candidate_idx) = items.iter().position(|item| {
            matches!(classify_item_slot(item), ItemSlotClass::InventoryLike)
                && !item.header.is_compact
                && !item.is_opaque()
                && !w_axiom.is_summary_item(5, &item.code)
        }) {
            if let Some(occupied_idx) = items.iter().position(|item| {
                classify_item_slot(item) == ItemSlotClass::InventoryLike
                    && (item.x != items[candidate_idx].x || item.y != items[candidate_idx].y)
            }) {
                selected = Some((items, candidate_idx, occupied_idx));
                break;
            }
        }
    }

    let (items, candidate_idx, occupied_idx) = selected
        .expect("fixture should contain a relocatable inventory candidate and an occupied target");

    let mut candidate = items[candidate_idx].clone();
    let original_x = candidate.x;
    let original_y = candidate.y;
    let original_page = candidate.page;
    let original_location = candidate.location;
    let original_mode = candidate.mode;
    let original_class = classify_item_slot(&candidate);
    let occupied_item = &items[occupied_idx];

    let err = try_apply_save_side_coordinated_relocation(
        &mut candidate,
        occupied_item.x,
        occupied_item.y,
        &items,
        4,
        original_location,
        original_mode,
    )
    .expect_err("occupied target should be rejected before mutation");
    assert_eq!(err, "Item placement overlaps occupied inventory cells");
    assert_eq!((candidate.x, candidate.y), (original_x, original_y));
    assert_eq!(candidate.page, original_page);
    assert_eq!(candidate.location, original_location);
    assert_eq!(candidate.mode, original_mode);
    assert_eq!(classify_item_slot(&candidate), original_class);

    Ok(())
}

#[test]
fn test_level_and_header_sync() -> io::Result<()> {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s")?;
    let huffman = HuffmanTree::new();

    // Level 1 -> 99
    let patched = d2r_core::save::patch_level(&bytes, 99, &huffman)?;

    let map = map_core_sections(&patched)?;
    let attributes = AttributeSection::parse(&patched, map.gf_pos, map.if_pos)?;

    assert_eq!(
        patched[d2r_core::save::CHAR_LEVEL_OFFSET],
        99,
        "Header level should be 99"
    );
    assert_eq!(
        attributes.actual_value(12, true),
        Some(99),
        "GF level should be 99"
    );

    // Skill patch test
    let mut skills = d2r_core::save::parse_skill_section(&patched, &map)?;
    // Amazon Critical Strike is index 3 (ID 9)
    // We update it to level 5
    let mut data = *skills.as_slice();
    data[3] = 5;
    let skills_updated = d2r_core::save::SkillSection::from_slice(&data)?;

    let version = u32::from_le_bytes(patched[4..8].try_into().unwrap_or([0; 4]));
    let items = Item::read_player_items(&patched, &huffman, version == 105)?;
    let final_rebuilt = rebuild_status_and_player_items(
        &patched,
        Some(&attributes),
        Some(&skills_updated),
        None,
        None,
        None,
        &items,
        &huffman,
    )?;

    let final_map = map_core_sections(&final_rebuilt)?;
    let final_skills = d2r_core::save::parse_skill_section(&final_rebuilt, &final_map)?;
    assert_eq!(final_skills.as_slice()[3], 5, "Critical Strike should be 5");
    Ok(())
}

#[test]
fn test_variable_length_rebuild() -> io::Result<()> {
    let bytes = fs::read(repo_path(
        "tests/fixtures/savegames/original/amazon_empty.d2s",
    ))?;
    let huffman = HuffmanTree::new();
    let map = map_core_sections(&bytes)?;
    let mut attrs = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;

    let original_len = bytes.len();

    // Add a stat ID 16 (item_armor_percent) which has 9 bits in stat_costs.rs
    // This is not in the special character stats list, so it tests the fallback/dynamic path.
    attrs.set_raw(16, 42);

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items = Item::read_player_items(&bytes, &huffman, version == 105)?;
    let rebuilt = rebuild_status_and_player_items(
        &bytes,
        Some(&attrs),
        None,
        None,
        None,
        None,
        &items,
        &huffman,
    )?;

    assert!(
        rebuilt.len() >= original_len,
        "Rebuilt save should be at least as large as original"
    );

    // Check that we can parse it back
    let new_map = map_core_sections(&rebuilt)?;
    let new_attrs = AttributeSection::parse(&rebuilt, new_map.gf_pos, new_map.if_pos)?;

    let found = new_attrs
        .entries
        .iter()
        .any(|e| e.stat_id == 16 && e.raw_value == 42);
    assert!(found, "New attribute entry (ID 16) should be preserved");

    let file_size_in_header = u32::from_le_bytes(rebuilt[8..12].try_into().unwrap());
    assert_eq!(
        file_size_in_header,
        rebuilt.len() as u32,
        "Header file size should be updated"
    );

    Ok(())
}
