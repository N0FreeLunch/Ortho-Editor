//! Integration tests for waypoint mutation contract pipeline.

use d2r_core::domain::contracts::{
    DifficultyLevel, WaypointMutationContract, WaypointWireDto,
};
use d2r_core::save::{apply_waypoint_mutation, Save};
use std::fs;
use std::path::Path;

fn load_fixture(rel_path: &str) -> Vec<u8> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let full_path = Path::new(manifest_dir).join(rel_path);
    fs::read(&full_path).unwrap_or_else(|_| panic!("Failed to read fixture: {}", full_path.display()))
}

#[test]
fn test_unlock_all_waypoints_on_fixture() {
    let original_bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");

    let wire_dto = WaypointWireDto {
        difficulty: Some(0), // Normal difficulty
        unlock_all: true,
        activate_waypoints: Vec::new(),
    };

    let contract = WaypointMutationContract::try_from(wire_dto)
        .expect("WaypointWireDto should parse into contract");

    let modified_bytes = apply_waypoint_mutation(&original_bytes, &contract)
        .expect("Waypoint mutation should succeed");

    // Verify roundtrip parsing and checksum
    let modified_save = Save::from_bytes(&modified_bytes).expect("Save should parse cleanly");
    assert_eq!(modified_save.header.magic, 0xAA55AA55);

    // Verify waypoints are activated
    let map = d2r_core::save::map_core_sections(&modified_bytes).expect("Map should succeed");
    let ws_pos = map.ws_pos.expect("WS should be present");
    let w4_pos = map.w4_pos.expect("w4 should be present");

    let wp_set = d2r_core::domain::progression::WaypointSet::from_bytes(
        &modified_bytes[ws_pos..w4_pos],
        0, // Normal
        ws_pos,
        true,
    );

    // Assert that waypoints in Normal are all active
    for wp in wp_set.waypoints() {
        assert!(
            wp.is_active(),
            "Waypoint '{}' should be active in Normal",
            wp.name()
        );
    }
}

#[test]
fn test_selective_waypoint_activation() {
    let original_bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");

    let wire_dto = WaypointWireDto {
        difficulty: Some(0), // Normal
        unlock_all: false,
        activate_waypoints: vec![
            "Act 1 - Wilderness 2".to_string(), // Cold Plains
            "Act 2 - Desert 2".to_string(),     // Lut Gholein desert
        ],
    };

    let contract = WaypointMutationContract::try_from(wire_dto).unwrap();
    let modified_bytes = apply_waypoint_mutation(&original_bytes, &contract).unwrap();

    let map = d2r_core::save::map_core_sections(&modified_bytes).unwrap();
    let ws_pos = map.ws_pos.unwrap();
    let w4_pos = map.w4_pos.unwrap();

    let wp_set = d2r_core::domain::progression::WaypointSet::from_bytes(
        &modified_bytes[ws_pos..w4_pos],
        0,
        ws_pos,
        true,
    );

    let wp_cold_plains = wp_set.find_by_name("Act 1 - Wilderness 2").unwrap();
    assert!(wp_cold_plains.is_active());

    let wp_desert = wp_set.find_by_name("Act 2 - Desert 2").unwrap();
    assert!(wp_desert.is_active());

    // Act 1 Catacombs 2 was not activated
    let wp_catacombs = wp_set.find_by_name("Act 1 - Catacombs 2").unwrap();
    assert!(!wp_catacombs.is_active());
}
