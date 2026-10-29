//! Integration tests for Item Mutation API, boundary validation, and 422 error response.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use d2r_core::domain::contracts::{
    ItemBoundaryValidator, ItemMutationContract, ItemStatOpWireDto, ItemTarget, ItemWireDto,
    NumericOp, ValidationErrorResponse,
};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::{try_apply_item_mutation, Save};

#[test]
fn test_item_stat_op_parsing() {
    let op1 = ItemStatOpWireDto::from_str("9:+20").expect("should parse +20");
    assert_eq!(op1.stat_id, 9);
    assert_eq!(op1.op, NumericOp::Add(20));

    let op2 = ItemStatOpWireDto::from_str("127:=2").expect("should parse =2");
    assert_eq!(op2.stat_id, 127);
    assert_eq!(op2.op, NumericOp::Set(2));

    let op3 = ItemStatOpWireDto::from_str("31:-5").expect("should parse -5");
    assert_eq!(op3.stat_id, 31);
    assert_eq!(op3.op, NumericOp::Sub(5));

    assert!(ItemStatOpWireDto::from_str("invalid").is_err());
    assert!(ItemStatOpWireDto::from_str("abc:+10").is_err());
}

#[test]
fn test_item_boundary_validation_success() {
    let dto = ItemWireDto {
        target: ItemTarget::by_index(0),
        defense: Some(NumericOp::Set(250)),
        durability: Some(NumericOp::Set(40)),
        max_durability: Some(NumericOp::Set(50)),
        quantity: Some(NumericOp::Set(20)),
        sockets: Some(3),
        is_ethereal: Some(true),
        is_identified: Some(true),
        stats: vec![
            ItemStatOpWireDto::new(9, NumericOp::Add(20)),
            ItemStatOpWireDto::new(127, NumericOp::Set(2)),
        ],
        ..Default::default()
    };

    let contract = ItemBoundaryValidator::validate(&dto, None)
        .expect("DTO with values within allowable boundaries should succeed");

    assert_eq!(contract.defense, Some(250));
    assert_eq!(contract.durability, Some(40));
    assert_eq!(contract.max_durability, Some(50));
    assert_eq!(contract.quantity, Some(20));
    assert_eq!(contract.sockets, Some(3));
    assert_eq!(contract.is_ethereal, Some(true));
    assert_eq!(contract.is_identified, Some(true));
    assert_eq!(contract.stats.len(), 2);
}

#[test]
fn test_item_boundary_validation_422_field_violations() {
    let dto = ItemWireDto {
        target: ItemTarget::by_index(0),
        defense: Some(NumericOp::Set(999_999)), // exceeds max 10,000
        durability: Some(NumericOp::Sub(50)),   // will underflow against 0 base
        max_durability: Some(NumericOp::Set(300)), // exceeds max 255
        quantity: Some(NumericOp::Set(1000)),   // exceeds max 511
        sockets: Some(10),                      // exceeds max 6
        is_ethereal: None,
        is_identified: None,
        stats: vec![],
        ..Default::default()
    };

    let result = ItemBoundaryValidator::validate(&dto, None);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let resp = ValidationErrorResponse::from(err);

    assert_eq!(resp.code, 422);
    assert_eq!(resp.status, "validation_error");

    // Verify field-level violation arrays and actionable hints
    let defense_errs = resp.errors.get("item.defense").expect("item.defense violation missing");
    assert_eq!(defense_errs[0].code, "invalid_range");
    assert!(defense_errs[0].hint.contains("between 0 and 10000"));

    let sockets_errs = resp.errors.get("item.sockets").expect("item.sockets violation missing");
    assert_eq!(sockets_errs[0].code, "invalid_range");
    assert!(sockets_errs[0].hint.contains("between 0 and 6"));

    let max_dur_errs = resp.errors.get("item.max_durability").expect("item.max_durability violation missing");
    assert_eq!(max_dur_errs[0].code, "invalid_range");

    let dur_errs = resp.errors.get("item.durability").expect("item.durability violation missing");
    assert_eq!(dur_errs[0].code, "underflow");
}

#[test]
fn test_item_mutation_save_roundtrip() {
    let fixture_path = Path::new("tests/fixtures/savegames/retail/amazon_initial.d2s");
    if !fixture_path.exists() {
        eprintln!("Fixture not found, skipping roundtrip test: {:?}", fixture_path);
        return;
    }

    let save_bytes = fs::read(fixture_path).expect("Failed to read fixture");
    let huffman = HuffmanTree::new();
    let version = u32::from_le_bytes(save_bytes[4..8].try_into().unwrap());
    let is_alpha = version == 105;

    let items_before = Item::read_player_items(&save_bytes, &huffman, is_alpha)
        .expect("Failed to parse player items before mutation");
    assert!(!items_before.is_empty(), "Fixture should contain items");

    // Target the buckler shield (buc) equipped on the Amazon
    let contract = ItemMutationContract {
        target: ItemTarget::by_code("buc"),
        defense: Some(20),
        durability: Some(10),
        max_durability: Some(15),
        quantity: None,
        sockets: None,
        is_ethereal: None,
        is_identified: Some(true),
        stats: vec![(9, 45)], // Add stat 9 (max mana) to 45
        ..Default::default()
    };

    let mutated_bytes = try_apply_item_mutation(&save_bytes, &contract, &huffman)
        .expect("Item mutation must succeed");

    // Verify roundtrip checksum and save integrity
    let modified_save = Save::from_bytes(&mutated_bytes).expect("Mutated save must be valid");
    assert_ne!(modified_save.header.checksum, 0);

    // Verify that the resulting save file size is valid
    assert!(mutated_bytes.len() >= 335);
}

#[test]
fn test_item_boundary_validation_dynamic_sockets() {
    let mut ring_item = Item {
        code: "rin".to_string(),
        ..Default::default()
    };
    ring_item.header.level = Some(80);

    let dto = ItemWireDto {
        target: ItemTarget::by_code("rin"),
        sockets: Some(1), // Rings have max 0 sockets in D2
        ..Default::default()
    };

    let result = ItemBoundaryValidator::validate(&dto, Some(&ring_item));
    assert!(result.is_err());
    let resp = ValidationErrorResponse::from(result.unwrap_err());
    assert_eq!(resp.code, 422);

    let socket_errs = resp.errors.get("item.sockets").expect("item.sockets violation missing");
    assert_eq!(socket_errs[0].code, "invalid_range");
    assert!(socket_errs[0].hint.contains("cannot have sockets"));

    // Now test sword with ilvl-based socket rules (e.g. low ilvl = 3 sockets max)
    let mut sword_item = Item {
        code: "crs".to_string(), // Crystal Sword
        ..Default::default()
    };
    sword_item.header.level = Some(15); // ilvl < 26 -> max 3 sockets

    let dto_sword = ItemWireDto {
        target: ItemTarget::by_code("crs"),
        sockets: Some(5), // exceeds max 3 for ilvl 15
        ..Default::default()
    };

    let result_sword = ItemBoundaryValidator::validate(&dto_sword, Some(&sword_item));
    assert!(result_sword.is_err());
    let resp_sword = ValidationErrorResponse::from(result_sword.unwrap_err());
    assert_eq!(resp_sword.code, 422);
    let sword_errs = resp_sword.errors.get("item.sockets").expect("sword sockets violation missing");
    assert!(sword_errs[0].message.contains("exceeds maximum allowable of 3"));
}

#[test]
fn test_item_boundary_validation_indestructible_durability_invariant() {
    let helm_item = Item {
        code: "cap".to_string(),
        max_durability: Some(12),
        current_durability: Some(12),
        ..Default::default()
    };

    // Indestructible with 0 durability
    let dto_zero = ItemWireDto {
        target: ItemTarget::by_code("cap"),
        durability: Some(NumericOp::Set(0)),
        is_indestructible: Some(true),
        ..Default::default()
    };

    let result_zero = ItemBoundaryValidator::validate(&dto_zero, Some(&helm_item));
    assert!(result_zero.is_err());
    let resp_zero = ValidationErrorResponse::from(result_zero.unwrap_err());
    assert_eq!(resp_zero.code, 422);
    let errs_zero = resp_zero.errors.get("item.durability").expect("durability error missing");
    assert_eq!(errs_zero[0].code, "invalid_state");
    assert!(errs_zero[0].message.contains("cannot have 0 durability"));

    // Indestructible with degraded durability (< max)
    let dto_degraded = ItemWireDto {
        target: ItemTarget::by_code("cap"),
        durability: Some(NumericOp::Set(5)),
        is_indestructible: Some(true),
        ..Default::default()
    };

    let result_degraded = ItemBoundaryValidator::validate(&dto_degraded, Some(&helm_item));
    assert!(result_degraded.is_err());
    let resp_degraded = ValidationErrorResponse::from(result_degraded.unwrap_err());
    assert_eq!(resp_degraded.code, 422);
    let errs_degraded = resp_degraded.errors.get("item.durability").expect("durability error missing");
    assert_eq!(errs_degraded[0].code, "invalid_state");
    assert!(errs_degraded[0].message.contains("cannot be less than max durability"));
}

#[test]
fn test_item_boundary_validation_ethereal_jewelry_rejected() {
    let amulet_item = Item {
        code: "amu".to_string(),
        ..Default::default()
    };

    let dto = ItemWireDto {
        target: ItemTarget::by_code("amu"),
        is_ethereal: Some(true),
        ..Default::default()
    };

    let result = ItemBoundaryValidator::validate(&dto, Some(&amulet_item));
    assert!(result.is_err());
    let resp = ValidationErrorResponse::from(result.unwrap_err());
    assert_eq!(resp.code, 422);
    let eth_errs = resp.errors.get("item.is_ethereal").expect("ethereal error missing");
    assert_eq!(eth_errs[0].code, "invalid_attribute");
}

#[test]
fn test_item_boundary_validation_high_level_damage_and_replenish() {
    let mut bow_item = Item {
        code: "8lb".to_string(), // Kuko Shakaku base
        ..Default::default()
    };
    bow_item.header.level = Some(60);

    let dto = ItemWireDto {
        target: ItemTarget::by_code("8lb"),
        enhanced_damage: Some(NumericOp::Set(150)),
        min_damage: Some(NumericOp::Add(15)),
        max_damage: Some(NumericOp::Add(30)),
        replenish_durability: Some(NumericOp::Set(3)),
        replenish_quantity: Some(NumericOp::Set(1)),
        is_indestructible: Some(true),
        ..Default::default()
    };

    let contract = ItemBoundaryValidator::validate(&dto, Some(&bow_item))
        .expect("High level damage and replenishment fields should validate successfully");

    assert_eq!(contract.enhanced_damage, Some(150));
    assert_eq!(contract.min_damage, Some(15));
    assert_eq!(contract.max_damage, Some(30));
    assert_eq!(contract.replenish_durability, Some(3));
    assert_eq!(contract.replenish_quantity, Some(1));
    assert_eq!(contract.is_indestructible, Some(true));

    // Verify stats map contains corresponding stat IDs
    let stats_map: std::collections::HashMap<u32, i32> = contract.stats.into_iter().collect();
    assert_eq!(stats_map.get(&17), Some(&150)); // ED%
    assert_eq!(stats_map.get(&21), Some(&15));  // Min damage
    assert_eq!(stats_map.get(&22), Some(&30));  // Max damage
    assert_eq!(stats_map.get(&252), Some(&3));  // Replenish durability
    assert_eq!(stats_map.get(&253), Some(&1));  // Replenish quantity
    assert_eq!(stats_map.get(&152), Some(&1));  // Indestructible
}
