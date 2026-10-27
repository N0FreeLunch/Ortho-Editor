use std::fs;
use d2r_core::domain::contracts::{CharacterMutationContract, CharacterWireDto, NumericOp};
use d2r_core::item::HuffmanTree;
use d2r_core::save::{apply_character_mutation, Save};

mod common;
use common::repo_path;

fn load_fixture(relative: &str) -> Vec<u8> {
    fs::read(repo_path(relative)).expect("fixture should be readable")
}

#[test]
fn test_character_mutation_pipeline_on_fixture() {
    let original_bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    let original_save = Save::from_bytes(&original_bytes).expect("original save should parse");
    assert_eq!(original_save.header.char_level, 1);
    assert_eq!(original_save.header.char_name, "TESTAMAZON");

    // 1. Create external Wire DTO with mutations
    let wire_dto = CharacterWireDto {
        name: Some("VALKYRIE".to_string()),
        level: Some(NumericOp::Set(99)),
        strength: Some(NumericOp::Set(350)),
        dexterity: Some(NumericOp::Set(250)),
        vitality: Some(NumericOp::Set(400)),
        energy: Some(NumericOp::Set(50)),
        stat_points: Some(NumericOp::Set(15)),
        gold_character: Some(NumericOp::Set(500_000)),
        gold_stash: Some(NumericOp::Set(2_500_000)),
        experience: Some(NumericOp::Set(3_520_485_254)),
    };

    // 2. Parse into domain mutation contract ("Parse, don't validate")
    let contract = CharacterMutationContract::try_from(wire_dto)
        .expect("Wire DTO should parse into valid contract");

    // 3. Apply mutation to raw save bytes
    let huffman = HuffmanTree::new();
    let modified_bytes = apply_character_mutation(&original_bytes, &contract, &huffman)
        .expect("Character mutation application should succeed");

    // 4. Verify modified save header and attributes
    let modified_save = Save::from_bytes(&modified_bytes)
        .expect("Modified save should parse successfully");

    assert_eq!(modified_save.header.char_level, 99);
    assert_eq!(modified_save.header.char_name, "VALKYRIE");

    // Verify attribute section values
    let map = d2r_core::save::map_core_sections(&modified_bytes).expect("Section map should succeed");
    let attrs = d2r_core::domain::stats::AttributeSection::parse(&modified_bytes, map.gf_pos, map.if_pos)
        .expect("Attributes should parse");

    assert_eq!(attrs.actual_value(12, true), Some(99)); // Level
    assert_eq!(attrs.actual_value(0, true), Some(350)); // Strength
    assert_eq!(attrs.actual_value(2, true), Some(250)); // Dexterity
    assert_eq!(attrs.actual_value(3, true), Some(400)); // Vitality
    assert_eq!(attrs.actual_value(1, true), Some(50));  // Energy
    assert_eq!(attrs.actual_value(4, true), Some(15));  // Stat Points
    assert_eq!(attrs.actual_value(14, true), Some(500_000)); // Gold Character
    assert_eq!(attrs.actual_value(15, true), Some(2_500_000)); // Gold Stash

    // 5. Verify Checksum validity
    let expected_checksum = d2r_core::save::recalculate_checksum(&modified_bytes)
        .expect("checksum recalculation should succeed");
    assert_eq!(modified_save.header.checksum, expected_checksum);
}

#[test]
fn test_relative_delta_mutation_pipeline() {
    let original_bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");

    // Initial amazon stats in fixture: str=20, dex=25, vit=20, enr=15, lvl=1
    // Apply relative mutations: level +5, strength +30, dexterity -5
    let wire_dto = CharacterWireDto {
        name: None,
        level: Some(NumericOp::Add(5)),
        strength: Some(NumericOp::Add(30)),
        dexterity: Some(NumericOp::Sub(5)),
        vitality: None,
        energy: None,
        stat_points: None,
        gold_character: None,
        gold_stash: Some(NumericOp::Add(100_000)),
        experience: None,
    };

    let contract = CharacterMutationContract::try_from(wire_dto)
        .expect("Relative wire DTO should parse into contract");

    let huffman = HuffmanTree::new();
    let modified_bytes = apply_character_mutation(&original_bytes, &contract, &huffman)
        .expect("Relative mutation should apply");

    let modified_save = Save::from_bytes(&modified_bytes).expect("Save should parse");
    assert_eq!(modified_save.header.char_level, 6); // 1 + 5 = 6

    let map = d2r_core::save::map_core_sections(&modified_bytes).expect("Map should succeed");
    let attrs = d2r_core::domain::stats::AttributeSection::parse(&modified_bytes, map.gf_pos, map.if_pos)
        .expect("Attrs should parse");

    assert_eq!(attrs.actual_value(12, true), Some(6));  // Level: 1 + 5 = 6
    assert_eq!(attrs.actual_value(0, true), Some(50));  // Strength: 20 + 30 = 50
    assert_eq!(attrs.actual_value(2, true), Some(20));  // Dexterity: 25 - 5 = 20
    assert_eq!(attrs.actual_value(15, true), Some(100_000)); // Stash Gold: 0 + 100,000 = 100,000
}

#[test]
fn test_boundary_violations_on_fixture_fail_gracefully() {
    let original_bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    let huffman = HuffmanTree::new();

    // 1. Test underflow: initial dex is 25, subtracting 30 should fail (25 - 30 < 1)
    let underflow_dto = CharacterWireDto {
        dexterity: Some(NumericOp::Sub(30)),
        ..Default::default()
    };
    let contract = CharacterMutationContract::try_from(underflow_dto).unwrap();
    let err = apply_character_mutation(&original_bytes, &contract, &huffman).unwrap_err();
    assert!(err.to_string().contains("Boundary underflow for 'dexterity'"));

    // 2. Test overflow: initial str is 20, adding 1010 should fail (20 + 1010 = 1030 > 1023)
    let overflow_dto = CharacterWireDto {
        strength: Some(NumericOp::Add(1010)),
        ..Default::default()
    };
    let contract = CharacterMutationContract::try_from(overflow_dto).unwrap();
    let err = apply_character_mutation(&original_bytes, &contract, &huffman).unwrap_err();
    assert!(err.to_string().contains("Boundary overflow for 'strength'"));

    // 3. Test dynamic gold character cap: level 1 max gold is 10,000. Setting 50,000 must fail.
    let gold_overflow_dto = CharacterWireDto {
        gold_character: Some(NumericOp::Set(50_000)),
        ..Default::default()
    };
    let contract = CharacterMutationContract::try_from(gold_overflow_dto).unwrap();
    let err = apply_character_mutation(&original_bytes, &contract, &huffman).unwrap_err();
    assert!(err.to_string().contains("Boundary overflow for 'gold_character'"));
}
