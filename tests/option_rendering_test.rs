use d2r_core::engine::formatter::{format_item, format_property};
use d2r_core::item::{HuffmanTree, Item, ItemBitRange, ItemProperty, ItemQuality};
use std::fs;

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

    // Find any non-compact item or first item with quality in amazon_10_scrolls
    let item_opt = items.iter().find(|i| !i.is_compact || i.quality.is_some());
    if let Some(item) = item_opt {
        let formatted_en = format_item(item, "en", 0, 99);
        let formatted_ko = format_item(item, "ko", 0, 99);
        assert!(!formatted_en.name.is_empty());
        assert!(!formatted_ko.name.is_empty());
    }
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

    println!("Authority total items: {}", items.len());
    for (i, it) in items.iter().enumerate() {
        println!("Item {}: code={}, is_runeword={}, properties={:?}", i, it.code, it.is_runeword, it.properties.len());
    }

    let authority = items.iter().find(|i| i.is_runeword).or_else(|| items.last()).expect("should have authority");
    let formatted_en = format_item(authority, "en", 0, 99);
    println!("Authority formatted properties: {:?}", formatted_en.properties);
}

#[test]
fn test_render_alpha_v105_decoded_properties_direct() {
    // Helper closure to create property
    let make_prop = |stat_id: u32, value: i32, param: u32| {
        ItemProperty::new(
            stat_id,
            String::new(),
            param,
            value,
            value,
            ItemBitRange::default(),
        )
    };

    // 1. All Skills (Stat 127, mapped from Alpha 256)
    let p_skills = make_prop(127, 2, 0);
    assert_eq!(format_property(&p_skills, 99, "en"), "+2 to All Skills");
    assert_eq!(format_property(&p_skills, 99, "ko"), "+2 모든 스킬 상승");

    // 2. Faster Hit Recovery (Stat 99, mapped from Alpha 496, descfunc 19)
    let p_fhr = make_prop(99, 30, 0);
    assert_eq!(format_property(&p_fhr, 99, "en"), "+30% Faster Hit Recovery");
    assert_eq!(format_property(&p_fhr, 99, "ko"), "+30 매우 빠른 회복속도 증가");

    // 3. Flat Defense (Stat 31, mapped from Alpha 26)
    let p_def = make_prop(31, 150, 0);
    assert_eq!(format_property(&p_def, 99, "en"), "+150 Defense");
    assert_eq!(format_property(&p_def, 99, "ko"), "+150 방어");

    // 3b. Enhanced Defense % (Stat 16, mapped from Alpha 499)
    let p_ed = make_prop(16, 150, 0);
    assert_eq!(format_property(&p_ed, 99, "en"), "+150% Enhanced Defense");
    assert_eq!(format_property(&p_ed, 99, "ko"), "+150 방어 상승");

    // 4. Elemental Resists (Stats 39, 41, 43, 45)
    let p_fire = make_prop(39, 30, 0);
    assert_eq!(format_property(&p_fire, 99, "en"), "Fire Resist +30%");
    assert_eq!(format_property(&p_fire, 99, "ko"), "+30 파이어 저항력");

    let p_light = make_prop(41, 25, 0);
    assert_eq!(format_property(&p_light, 99, "en"), "Lightning Resist +25%");
    assert_eq!(format_property(&p_light, 99, "ko"), "+25 라이트닝 저항력");

    let p_cold = make_prop(43, 20, 0);
    assert_eq!(format_property(&p_cold, 99, "en"), "Cold Resist +20%");
    assert_eq!(format_property(&p_cold, 99, "ko"), "+20 콜드 저항력");

    let p_poison = make_prop(45, 15, 0);
    assert_eq!(format_property(&p_poison, 99, "en"), "Poison Resist +15%");
    assert_eq!(format_property(&p_poison, 99, "ko"), "+15 포이즌 저항력");

    // 5. Indestructible (Stat 152, mapped from Alpha 380)
    let p_indestructible = make_prop(152, 1, 0);
    assert_eq!(format_property(&p_indestructible, 99, "en"), "+1 Indestructible");
    assert_eq!(format_property(&p_indestructible, 99, "ko"), "+1 (파괴안됨)");

    // 6. Max Life / Mana (Stats 7, 9)
    let p_life = make_prop(7, 40, 0);
    assert_eq!(format_property(&p_life, 99, "en"), "+40 to Life");
    assert_eq!(format_property(&p_life, 99, "ko"), "+40 라이프");

    let p_mana = make_prop(9, 50, 0);
    assert_eq!(format_property(&p_mana, 99, "en"), "+50 to Mana");
    assert_eq!(format_property(&p_mana, 99, "ko"), "+50 마나");

    // 7. Opaque Property fallback rendering
    let p_opaque = ItemProperty::new_opaque(999, vec![true, false], ItemBitRange::default());
    assert_eq!(
        format_property(&p_opaque, 99, "en"),
        "[Unresolved Option] Unmapped Stat (ID: 999)"
    );
    assert_eq!(
        format_property(&p_opaque, 99, "ko"),
        "[미결 옵션] 미분류 스탯 (ID: 999)"
    );
}

#[test]
fn test_render_in_memory_alpha_v105_decoded_item() {
    let mut item = Item::default();
    item.code = "xrs".to_string(); // Cuirass
    item.quality = Some(ItemQuality::Unique);
    item.is_identified = true;

    // Inject decoded properties mapped from Alpha v105
    item.properties.push(ItemProperty::new(
        127,
        "item_allskills".to_string(),
        0,
        2,
        2,
        ItemBitRange::default(),
    ));
    item.properties.push(ItemProperty::new(
        99,
        "item_fastergethitrate".to_string(),
        0,
        30,
        30,
        ItemBitRange::default(),
    ));
    item.properties.push(ItemProperty::new(
        16,
        "item_armor_percent".to_string(),
        0,
        150,
        150,
        ItemBitRange::default(),
    ));
    item.properties.push(ItemProperty::new(
        39,
        "fireresist".to_string(),
        0,
        30,
        30,
        ItemBitRange::default(),
    ));

    let formatted_en = format_item(&item, "en", 0, 99);
    let formatted_ko = format_item(&item, "ko", 0, 99);

    assert_eq!(formatted_en.properties.len(), 4);
    assert_eq!(formatted_en.properties[0], "+2 to All Skills");
    assert_eq!(formatted_en.properties[1], "+30% Faster Hit Recovery");
    assert_eq!(formatted_en.properties[2], "+150% Enhanced Defense");
    assert_eq!(formatted_en.properties[3], "Fire Resist +30%");

    assert_eq!(formatted_ko.properties.len(), 4);
    assert_eq!(formatted_ko.properties[0], "+2 모든 스킬 상승");
    assert_eq!(formatted_ko.properties[1], "+30 매우 빠른 회복속도 증가");
    assert_eq!(formatted_ko.properties[2], "+150 방어 상승");
    assert_eq!(formatted_ko.properties[3], "+30 파이어 저항력");
}


