//! Comprehensive integration tests for the unified SaveMutationPipeline,
//! multi-domain boundary validation (RFC 7807 422), skills/respec, and batch operations.

use std::fs;
use std::path::Path;

use d2r_core::domain::contracts::{
    BatchItemWireDto, CharacterWireDto, ItemTarget, ItemWireDto, NumericOp,
    SaveMutationDto, SaveMutationPipeline, SaveMutationValidator, SkillOpWireDto,
    SkillWireDto, WaypointWireDto,
};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::Save;

#[test]
fn test_save_mutation_dto_json_roundtrip() {
    let dto = SaveMutationDto {
        character: Some(CharacterWireDto {
            name: Some("Valkyrie".to_string()),
            level: Some(NumericOp::Set(85)),
            strength: Some(NumericOp::Add(20)),
            ..Default::default()
        }),
        waypoints: Some(WaypointWireDto {
            unlock_all: true,
            difficulty: Some(0),
            ..Default::default()
        }),
        skills: Some(SkillWireDto {
            skills: vec![
                SkillOpWireDto::new(6, NumericOp::Set(20)),
                SkillOpWireDto::new(7, NumericOp::Set(10)),
            ],
            skill_points: Some(NumericOp::Set(5)),
            respec: Some(true),
        }),
        batch_items: Some(BatchItemWireDto {
            repair_all: Some(true),
            identify_all: Some(true),
            socket_all: Some(4),
        }),
        items: vec![ItemWireDto {
            target: ItemTarget::by_code("buc"),
            defense: Some(NumericOp::Set(25)),
            ..Default::default()
        }],
    };

    let json = serde_json::to_string_pretty(&dto).expect("Serialization must succeed");
    assert!(json.contains("\"Valkyrie\""));
    assert!(json.contains("\"repair_all\": true"));
    assert!(json.contains("\"unlock_all\": true"));

    let deserialized: SaveMutationDto =
        serde_json::from_str(&json).expect("Deserialization must succeed");
    assert_eq!(dto, deserialized);
}

#[test]
fn test_save_mutation_simultaneous_422_aggregation() {
    let fixture_path = Path::new("tests/fixtures/savegames/retail/amazon_initial.d2s");
    if !fixture_path.exists() {
        eprintln!("Fixture not found, skipping: {:?}", fixture_path);
        return;
    }

    let save_bytes = fs::read(fixture_path).expect("Failed to read fixture");
    let huffman = HuffmanTree::new();

    // Construct a payload with simultaneous violations across character, skills, and items
    let invalid_dto = SaveMutationDto {
        character: Some(CharacterWireDto {
            level: Some(NumericOp::Set(150)),       // Violation: level > 99
            ..Default::default()
        }),
        skills: Some(SkillWireDto {
            skills: vec![
                SkillOpWireDto::new(6, NumericOp::Set(30)), // Violation: skill > 20
                SkillOpWireDto::new(999, NumericOp::Set(5)), // Violation: not an Amazon skill
            ],
            ..Default::default()
        }),
        items: vec![ItemWireDto {
            target: ItemTarget::by_code("zzz"), // Violation: item not found
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = SaveMutationValidator::validate(&invalid_dto, &save_bytes, &huffman);
    assert!(result.is_err(), "Simultaneous invalid payload must be rejected");

    let resp = result.unwrap_err();
    assert_eq!(resp.code, 422);
    assert_eq!(resp.status, "validation_error");

    // Verify all domains are aggregated into a single RFC 7807 error map
    assert!(resp.errors.contains_key("level"), "Missing level violation");
    assert!(resp.errors.contains_key("skills.skills[0].level"), "Missing skill 0 level violation");
    assert!(resp.errors.contains_key("skills.skills[1].skill_id"), "Missing skill 1 class violation");
    assert!(resp.errors.contains_key("items[0].target"), "Missing item target violation");
}

#[test]
fn test_save_mutation_pipeline_atomic_execution() {
    let fixture_path = Path::new("tests/fixtures/savegames/retail/amazon_initial.d2s");
    if !fixture_path.exists() {
        eprintln!("Fixture not found, skipping: {:?}", fixture_path);
        return;
    }

    let save_bytes = fs::read(fixture_path).expect("Failed to read fixture");
    let huffman = HuffmanTree::new();

    // Multi-domain valid mutation request
    let mutation_dto = SaveMutationDto {
        character: Some(CharacterWireDto {
            name: Some("Athena".to_string()),
            level: Some(NumericOp::Set(40)),
            strength: Some(NumericOp::Set(100)),
            dexterity: Some(NumericOp::Set(90)),
            vitality: Some(NumericOp::Set(110)),
            energy: Some(NumericOp::Set(50)),
            stat_points: Some(NumericOp::Set(15)),
            gold_stash: Some(NumericOp::Set(100_000)),
            ..Default::default()
        }),
        waypoints: Some(WaypointWireDto {
            unlock_all: true,
            difficulty: Some(0), // Normal difficulty
            ..Default::default()
        }),
        skills: Some(SkillWireDto {
            skills: vec![
                SkillOpWireDto::new(6, NumericOp::Set(15)), // Jab to 15
                SkillOpWireDto::new(14, NumericOp::Set(10)), // Power Strike to 10
            ],
            skill_points: Some(NumericOp::Set(8)),
            respec: None,
        }),
        batch_items: Some(BatchItemWireDto {
            repair_all: Some(true),
            identify_all: Some(true),
            socket_all: None,
        }),
        items: vec![ItemWireDto {
            target: ItemTarget::by_code("buc"),
            defense: Some(NumericOp::Set(22)),
            is_identified: Some(true),
            ..Default::default()
        }],
    };

    let stream_orig = d2r_core::domain::item::stream::PlayerItemStream::read(&save_bytes, &huffman).unwrap();
    let buc_orig = stream_orig.items.iter().find(|it| it.code.trim() == "buc").unwrap();
    println!("BUC ORIG: defense={:?}, cur_dur={:?}, max_dur={:?}, is_compact={}, flags={:#x}",
        buc_orig.defense, buc_orig.current_durability, buc_orig.max_durability, buc_orig.header.is_compact, buc_orig.header.flags);

    // 1. Unified boundary validation
    let contract = SaveMutationValidator::validate(&mutation_dto, &save_bytes, &huffman)
        .expect("Validation should pass for valid mutation DTO");

    // 2. Atomic execution
    let mutated_bytes = SaveMutationPipeline::execute(&save_bytes, &contract, &huffman)
        .expect("Pipeline execution should succeed");

    // 3. Verify resulting save integrity and checksum
    let modified_save = Save::from_bytes(&mutated_bytes).expect("Mutated save must be valid");
    assert_eq!(modified_save.header.char_name, "Athena");
    assert_eq!(modified_save.header.char_level, 40);
    assert_ne!(modified_save.header.checksum, 0);

    // 4. Verify attribute values
    let map = d2r_core::save::map_core_sections(&mutated_bytes).expect("Should map core sections");
    let attrs = d2r_core::domain::stats::AttributeSection::parse(&mutated_bytes, map.gf_pos, map.if_pos)
        .expect("Should parse attributes");

    assert_eq!(attrs.actual_value(0, false), Some(100)); // Strength
    assert_eq!(attrs.actual_value(2, false), Some(90));  // Dexterity
    assert_eq!(attrs.actual_value(3, false), Some(110)); // Vitality
    assert_eq!(attrs.actual_value(1, false), Some(50));  // Energy
    assert_eq!(attrs.actual_value(4, false), Some(15));  // Stat points
    assert_eq!(attrs.actual_value(5, false), Some(8));   // Skill points
    assert_eq!(attrs.actual_value(15, false), Some(100_000)); // Gold stash

    // 5. Verify skill levels
    let skills_sec = d2r_core::domain::character::skills::parse_skill_section(
        &mutated_bytes,
        map.if_pos,
        map.jm_positions.first().copied(),
    )
    .expect("Should parse skill section");

    // Amazon base skill id = 6
    assert_eq!(skills_sec.get_level(6, 6), 15);
    assert_eq!(skills_sec.get_level(6, 14), 10);

    // 6. Verify item mutation
    let stream = d2r_core::domain::item::stream::PlayerItemStream::read(&mutated_bytes, &huffman)
        .expect("Should parse player items via PlayerItemStream");
    let buc = stream
        .items
        .iter()
        .find(|it| it.code.trim() == "buc")
        .expect("Shield 'buc' should exist");
    println!("BUC AFTER: defense={:?}, cur_dur={:?}, max_dur={:?}, is_compact={}, flags={:#x}",
        buc.defense, buc.current_durability, buc.max_durability, buc.header.is_compact, buc.header.flags);
    assert_eq!(buc.defense, Some(22));
    assert!(buc.header.is_identified);
}

#[test]
fn test_skill_respec_and_refund_pipeline() {
    let fixture_path = Path::new("tests/fixtures/savegames/retail/amazon_initial.d2s");
    if !fixture_path.exists() {
        eprintln!("Fixture not found, skipping: {:?}", fixture_path);
        return;
    }

    let save_bytes = fs::read(fixture_path).expect("Failed to read fixture");
    let huffman = HuffmanTree::new();

    // Step A: First give the Amazon some allocated skill points
    let setup_dto = SaveMutationDto {
        skills: Some(SkillWireDto {
            skills: vec![
                SkillOpWireDto::new(6, NumericOp::Set(5)),  // Jab = 5
                SkillOpWireDto::new(10, NumericOp::Set(3)), // Inner Sight = 3
            ],
            skill_points: Some(NumericOp::Set(2)),
            respec: None,
        }),
        ..Default::default()
    };

    let contract_a = SaveMutationValidator::validate(&setup_dto, &save_bytes, &huffman)
        .expect("Setup validation must succeed");
    let setup_bytes = SaveMutationPipeline::execute(&save_bytes, &contract_a, &huffman)
        .expect("Setup pipeline execution must succeed");

    // Step B: Now execute a Respec on the save with allocated skills
    let respec_dto = SaveMutationDto {
        skills: Some(SkillWireDto {
            respec: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let contract_b = SaveMutationValidator::validate(&respec_dto, &setup_bytes, &huffman)
        .expect("Respec validation must succeed");
    let respec_bytes = SaveMutationPipeline::execute(&setup_bytes, &contract_b, &huffman)
        .expect("Respec pipeline execution must succeed");

    let map = d2r_core::save::map_core_sections(&respec_bytes).expect("Should map core sections");
    let attrs = d2r_core::domain::stats::AttributeSection::parse(&respec_bytes, map.gf_pos, map.if_pos)
        .expect("Should parse attributes");
    let skills_sec = d2r_core::domain::character::skills::parse_skill_section(
        &respec_bytes,
        map.if_pos,
        map.jm_positions.first().copied(),
    )
    .expect("Should parse skill section");

    // All skills should be zeroed
    assert_eq!(skills_sec.get_level(6, 6), 0);
    assert_eq!(skills_sec.get_level(6, 10), 0);

    // Unspent skill points should equal previous unspent (2) + refunded (5 + 3 = 8) = 10
    let unspent = attrs.actual_value(5, false).expect("Skill points should exist");
    assert_eq!(unspent, 10);
}
