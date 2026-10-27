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
    };

    let mutated_bytes = try_apply_item_mutation(&save_bytes, &contract, &huffman)
        .expect("Item mutation must succeed");

    // Verify roundtrip checksum and save integrity
    let modified_save = Save::from_bytes(&mutated_bytes).expect("Mutated save must be valid");
    assert_ne!(modified_save.header.checksum, 0);

    // Verify that the resulting save file size is valid
    assert!(mutated_bytes.len() >= 335);
}
