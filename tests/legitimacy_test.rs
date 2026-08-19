use d2r_core::data::legitimacy::calc_alvl;
use d2r_core::engine::validation::{
    check_alvl_legitimacy, check_socket_legitimacy, check_staffmod_legitimacy,
    check_item_code_legitimacy,
};
use d2r_core::item::{Item, ItemBitRange, ItemProperty, ItemQuality};

#[test]
fn calc_alvl_matches_expected_formula() {
    // ilvl=50, qlvl=30, magic_lvl=0 -> temp=50, 50 < 84, alvl=50-15=35
    assert_eq!(calc_alvl(50, 30, 0), 35);
}

#[test]
fn test_socket_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string(); // Long Sword, type "swor"
    item.level = Some(1); // ilvl 1 -> max_sock_low = 3 for "swor"
    item.sockets = Some(6); // Exceeds max 3
    item.quality = Some(ItemQuality::Normal);
    item.is_identified = true;
    item.properties_complete = true;

    let warnings = check_socket_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about socket count exceeding max for ilvl 1"
    );
    // "Socket count 6 exceeds max 3 for ilvl 1"
    assert!(
        warnings[0].contains("exceeds max 3"),
        "Warning missing 'exceeds max 3': {}",
        warnings[0]
    );
}

#[test]
fn test_socket_legitimacy_valid() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string(); // Long Sword, type "swor"
    item.level = Some(42); // ilvl 42 -> max_sock_high = 6 for "swor"
    item.sockets = Some(6); // Valid
    item.quality = Some(ItemQuality::Normal);
    item.is_identified = true;
    item.properties_complete = true;

    let warnings = check_socket_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Should NOT warn for valid socket count: {:?}",
        warnings
    );
}

#[test]
fn test_alvl_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string(); // Long Sword, qlvl=20
    item.level = Some(1); // ilvl 1 -> alvl=10
    item.quality = Some(ItemQuality::Magic);
    item.magic_prefix = Some(194); // Prefix "Cruel", level 51

    let warnings = check_alvl_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about Cruel prefix on ilvl 1 Long Sword"
    );
    assert!(
        warnings[0].contains("Cruel"),
        "Warning missing 'Cruel': {}",
        warnings[0]
    );
    assert!(
        warnings[0].contains("item aLvl 10"),
        "Expected aLvl 10 mismatch: {}",
        warnings[0]
    );
}

#[test]
fn test_alvl_legitimacy_valid() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string();
    item.level = Some(99); // ilvl 99 -> alvl=99
    item.quality = Some(ItemQuality::Magic);
    item.magic_prefix = Some(194); // Prefix "Cruel", level 51

    let warnings = check_alvl_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Should NOT warn about Cruel on ilvl 99 item: {:?}",
        warnings
    );
}

#[test]
fn test_staffmod_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "buc ".to_string(); // Buckler, type "shie" -> "armo"
    item.level = Some(1); // ilvl 1
    item.quality = Some(ItemQuality::Normal);

    // fireresist (39) with value 25 matches Tier 3 (Level 28) for itype "armo" in legitimacy.rs
    item.properties.push(ItemProperty::new(
        39,
        "fireresist".to_string(),
        0,
        25,
        25,
        ItemBitRange::default(),
    ));

    let warnings = check_staffmod_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about high tier Automod on ilvl 1 shield"
    );
    assert!(
        warnings[0].contains("requires ilvl 39"),
        "Warning mismatch: {}",
        warnings[0]
    );
}

#[test]
fn test_staffmod_legitimacy_valid() {
    let mut item = Item::empty_for_tests();
    item.code = "buc ".to_string();
    item.level = Some(40); // ilvl 40
    item.quality = Some(ItemQuality::Normal);

    item.properties.push(ItemProperty::new(
        39,
        "fireresist".to_string(),
        0,
        25,
        25,
        ItemBitRange::default(),
    ));

    let warnings = check_staffmod_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Should NOT warn for valid Automod tier: {:?}",
        warnings
    );
}

#[test]
fn test_alvl_legitimacy_with_magic_lvl() {
    let mut item = Item::empty_for_tests();
    item.code = "ci0 ".to_string(); // Circlet, qlvl=24, magic_lvl=3
    item.level = Some(1); // ilvl 1 -> aLvl = max(1, 24) + 3 = 27
    item.quality = Some(ItemQuality::Magic);

    // Prefix "Knight's" (id 38), level 25. Should be VALID (25 <= 27).
    item.magic_prefix = Some(38);

    let warnings = check_alvl_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Knight's (lvl 25) should be valid on ilvl 1 Circlet (aLvl 27): {:?}",
        warnings
    );

    // Prefix "Meteoric" (id 33), level 27. Should be VALID (27 <= 27).
    item.magic_prefix = Some(33);
    let warnings = check_alvl_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Meteoric (lvl 27) should be valid on ilvl 1 Circlet (aLvl 27): {:?}",
        warnings
    );

    // Prefix "Platinum" (id 32), level 22. Should be VALID.
    item.magic_prefix = Some(32);
    let warnings = check_alvl_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Platinum (lvl 22) should be valid on ilvl 1 Circlet (aLvl 27)"
    );
}

#[test]
fn test_affix_group_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string();
    item.quality = Some(ItemQuality::Rare);

    // Group 101: Sturdy (id 1), Strong (id 2)
    // Rare items store prefixes in rare_affixes[0, 2, 4]
    item.rare_affixes[0] = Some(1); // Sturdy
    item.rare_affixes[2] = Some(2); // Strong

    let warnings = d2r_core::engine::validation::check_affix_group_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about duplicate affix group 101"
    );
    assert!(
        warnings[0].contains("Duplicate affix group found: 101"),
        "Warning mismatch: {}",
        warnings[0]
    );
}

#[test]
fn test_ethereal_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string(); // Long Sword, base durability 44
    item.is_ethereal = true;
    // Expected max_durability = (44/2) + 1 = 23
    item.max_durability = Some(10); // Too low

    let warnings = d2r_core::engine::validation::check_ethereal_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about low ethereal durability"
    );
    assert!(
        warnings[0].contains("lower than expected durability"),
        "Warning mismatch: {}",
        warnings[0]
    );
    assert!(
        warnings[0].contains("expected ~23"),
        "Warning missing expected value: {}",
        warnings[0]
    );
}

#[test]
fn test_ethereal_legitimacy_valid() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string();
    item.is_ethereal = true;
    item.max_durability = Some(23); // Correct

    let warnings = d2r_core::engine::validation::check_ethereal_legitimacy(&item);
    assert!(
        warnings.is_empty(),
        "Should NOT warn for correct ethereal durability: {:?}",
        warnings
    );
}

#[test]
fn test_ethereal_defense_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "utp ".to_string(); // Archon Plate, max_ac=524
    item.is_ethereal = true;

    // Add 100% ED. Expected Base = 524 + 1 = 525.
    // Eth_Base = floor(525 * 1.5) = 787
    // Expected_Def = floor(787 * 2.0) = 1574
    item.properties.push(ItemProperty::new(
        16,
        "item_armor_percent".to_string(),
        0,
        100,
        100,
        ItemBitRange::default(),
    ));

    item.defense = Some(1000); // Way too low for ethereal + 100% ED

    let warnings = d2r_core::engine::validation::check_base_stat_legitimacy(&item);
    assert!(
        !warnings.is_empty(),
        "Should warn about defense mismatch on ethereal armor"
    );
    assert!(
        warnings[0].contains("does not match expected 1574"),
        "Warning mismatch: {}",
        warnings[0]
    );
}

#[test]
fn test_illegal_ethereal_bow() {
    let mut item = Item::empty_for_tests();
    item.code = "8hb ".to_string(); // Hydra Bow
    item.is_ethereal = true;

    let warnings = d2r_core::engine::validation::check_ethereal_legitimacy(&item);
    assert!(!warnings.is_empty(), "Should warn about ethereal bow");
    assert!(
        warnings[0].contains("Bows and crossbows cannot be ethereal"),
        "Warning missing 'cannot be ethereal': {}",
        warnings[0]
    );
}

#[test]
fn test_item_code_legitimacy_violation() {
    let mut item = Item::empty_for_tests();
    item.code = "abc ".to_string(); // Non-existent custom code

    let warnings = check_item_code_legitimacy(&item);
    assert!(!warnings.is_empty(), "Should warn about invalid item code");
    assert!(
        warnings[0].contains("Invalid item code 'abc'"),
        "Warning missing 'Invalid item code 'abc'': {}",
        warnings[0]
    );
}

#[test]
fn test_item_code_legitimacy_valid() {
    let mut item = Item::empty_for_tests();
    item.code = "lsd ".to_string(); // Valid Long Sword code

    let warnings = check_item_code_legitimacy(&item);
    assert!(warnings.is_empty(), "Should NOT warn for valid item code: {:?}", warnings);
}
