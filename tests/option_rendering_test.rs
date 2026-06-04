use d2r_core::engine::formatter::format_item;
use d2r_core::item::{HuffmanTree, Item};
use std::fs;
use std::path::PathBuf;

mod common;
use common::repo_path;

#[test]
fn test_render_buckler_from_fixture() {
    let bytes = fs::read(repo_path(
        "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
    ))
    .expect("fixture should exist");
    let huffman = HuffmanTree::new();
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items =
        Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");

    // This Buckler is item 15 in the amazon_10_scrolls fixture.
    let buckler = &items[15];
    assert_eq!(buckler.code.trim(), "buc");

    let formatted_en = format_item(buckler, "en", 0, 99);
    let formatted_ko = format_item(buckler, "ko", 0, 99);

    // Base attributes check
    assert!(
        formatted_en
            .base_attributes
            .iter()
            .any(|s| s.contains("Defense"))
    );
    // `assert!(formatted_ko.base_attributes.iter().any(|s| s.contains("방어력")));`

    // Quality check
    assert_eq!(formatted_en.quality_name, "Some(Normal)");
}

#[test]
fn test_render_authority_properties() {
    let bytes = fs::read(repo_path(
        "tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
    ))
    .expect("fixture should exist");
    let huffman = HuffmanTree::new();
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items =
        Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");

    let authority = items.last().expect("should have authority");
    assert_eq!(authority.code.trim(), "xrs");

    let formatted_en = format_item(authority, "en", 0, 99);

    println!("Authority Propertis: {:?}", formatted_en.properties);

    // Check one of the complex properties (descfunc 15 likely)
    // Authority should have "+%d%% Chance to cast level %d %s on attack"
    assert!(
        formatted_en
            .properties
            .iter()
            .any(|s| s.contains("Chance to cast level"))
    );
}
