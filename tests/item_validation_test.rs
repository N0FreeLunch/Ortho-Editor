use d2r_core::engine::validation::{StatValidationStatus, validate_item};
use d2r_core::item::{Item, ItemBitRange, ItemProperty, ItemQuality};

fn base_item() -> Item {
    let mut item = Item::empty_for_tests();
    item.code = "hax ".to_string();
    item.is_compact = true;
    item.is_identified = true;
    item.properties_complete = true;
    item
}

fn prop(stat_id: u32, param: u32, value: i32) -> ItemProperty {
    ItemProperty::new(
        stat_id,
        format!("stat_{stat_id}"),
        param,
        value,
        value,
        ItemBitRange::default(),
    )
}

#[test]
fn validate_unique_item_perfect_roll() {
    let mut item = base_item();
    item.quality = Some(ItemQuality::Unique);
    item.unique_id = Some(0); // The Gnasher
    item.properties = vec![
        prop(0, 0, 8),
        prop(135, 0, 50),
        prop(136, 0, 20),
        prop(111, 0, 70),
    ];

    let result = validate_item(&item).expect("unique item should resolve a spec");
    assert_eq!(result.spec_name, "The Gnasher");
    assert!(result.is_perfect);
    assert!((result.score - 1.0).abs() < f32::EPSILON);
    assert!(
        result
            .stats
            .iter()
            .all(|s| s.status == StatValidationStatus::InRange)
    );
}

#[test]
fn validate_unique_item_out_of_range_roll() {
    let mut item = base_item();
    item.quality = Some(ItemQuality::Unique);
    item.unique_id = Some(0); // The Gnasher
    item.properties = vec![
        prop(0, 0, 8),
        prop(135, 0, 50),
        prop(136, 0, 20),
        prop(111, 0, 80), // out of range (60..70)
    ];

    let result = validate_item(&item).expect("unique item should resolve a spec");
    assert_eq!(result.spec_name, "The Gnasher");
    assert!(!result.is_perfect);
    assert!(result.score < 1.0);
    assert!(
        result
            .stats
            .iter()
            .any(|s| s.stat_id == 111 && s.status == StatValidationStatus::OutOfRange)
    );
}

#[test]
fn validate_runeword_item_static_data() {
    let mut item = base_item();
    item.code = "tors".to_string();
    item.is_runeword = true;
    item.runeword_id = Some(2); // Authority
    item.runeword_attributes = vec![
        prop(201, 387, 10),
        prop(198, 399, 12),
        prop(83, 7, 2),
        prop(111, 0, 50),
    ];

    let result = validate_item(&item).expect("runeword item should resolve a spec");
    assert_eq!(result.spec_name, "Authority");
    assert!(!result.is_perfect);
    assert!(result.score > 0.0 && result.score < 1.0);
    assert!(
        result
            .stats
            .iter()
            .all(|s| s.status == StatValidationStatus::InRange)
    );
}

#[test]
fn validate_param_mismatch_creates_missing_and_unexpected() {
    let mut item = base_item();
    item.code = "tors".to_string();
    item.is_runeword = true;
    item.runeword_id = Some(2); // Authority
    item.runeword_attributes = vec![
        prop(201, 0, 10), // wrong param (expected 387)
        prop(198, 399, 12),
        prop(83, 7, 2),
        prop(111, 0, 50),
    ];

    let result = validate_item(&item).expect("runeword item should resolve a spec");
    assert_eq!(result.spec_name, "Authority");
    assert!(
        result
            .stats
            .iter()
            .any(|s| s.status == StatValidationStatus::UnexpectedOnItem)
    );
    assert!(
        result
            .stats
            .iter()
            .any(|s| s.status == StatValidationStatus::MissingOnItem)
    );
}

#[test]
fn validate_magic_item_with_affixes() {
    let mut item = base_item();
    item.quality = Some(ItemQuality::Magic);
    item.magic_prefix = Some(1); // Sturdy (+20..30 Def)
    item.magic_suffix = Some(0); // of Health (1 DR)
    item.properties = vec![
        prop(16, 0, 25), // In range (Score 0.5)
        prop(36, 0, 1),  // In range (Score 1.0)
    ];

    let result = validate_item(&item).expect("magic item should validate");
    assert_eq!(result.spec_name, "Sturdy of Health");
    assert!(!result.is_perfect);
    // Score is 0.5 because only the variable stat (16: 20-30) is counted in variable_scores
    assert!((result.score - 0.5).abs() < 0.001);
}

#[test]
fn validate_rare_item_merged_stats() {
    let mut item = base_item();
    item.quality = Some(ItemQuality::Rare);
    // Let's use two affixes that both give Defense if possible, or just two different ones.
    // Prefix 1: Sturdy (20..30 Def)
    // Prefix 2: Strong (31..40 Def)
    item.rare_affixes[0] = Some(1);
    item.rare_affixes[2] = Some(2);
    item.properties = vec![
        prop(16, 0, 60), // Merged range: (20+31)..(30+40) = 51..70. 60 is in range.
    ];

    let result = validate_item(&item).expect("rare item should validate");
    assert!(!result.is_perfect);
    assert!(
        result
            .stats
            .iter()
            .all(|s| s.status == StatValidationStatus::InRange)
    );
    assert_eq!(result.stats[0].min, 51);
    assert_eq!(result.stats[0].max, 70);
}

#[test]
fn validate_magic_item_with_type_violation() {
    let mut item = base_item();
    item.code = "buc ".to_string(); // Buckler (Shield)
    item.quality = Some(ItemQuality::Magic);
    item.magic_prefix = Some(12); // Jagged (Weapon-only)
    item.magic_suffix = None;
    item.properties = vec![
        prop(111, 0, 15), // In range for Jagged (10-20)
    ];

    let result = validate_item(&item).expect("should validate");
    assert!(
        !result.warnings.is_empty(),
        "Should warn about type violation"
    );
    assert!(result.warnings[0].contains("not eligible"));
}

#[test]
fn incomplete_parsing_suppresses_validation() {
    let mut item = base_item();
    item.quality = Some(ItemQuality::Unique);
    item.unique_id = Some(0);
    // 1. Incomplete properties
    item.is_compact = false;
    item.properties_complete = false;
    assert!(
        validate_item(&item).is_none(),
        "Incomplete property parsing should suppress validation"
    );

    // 2. Opaque module present
    item.properties_complete = true;
    item.modules.push(d2r_core::item::ItemModule::Opaque(vec![true, false]));
    assert!(
        validate_item(&item).is_none(),
        "Opaque item should suppress validation"
    );
}

#[test]
fn rare_heavy_belt_defense_within_possibility_envelope_produces_no_warning() {
    let mut item = base_item();
    item.code = "hbl ".to_string(); // Heavy Belt, base ac: 8-11
    item.quality = Some(ItemQuality::Rare);
    item.defense = Some(19); // Rolled defense 19 (within rare envelope 8..71)
    item.rare_name_1 = Some(1);
    item.rare_name_2 = Some(2);

    let result = validate_item(&item).expect("rare item should validate");
    let defense_warnings: Vec<_> = result
        .detailed_warnings
        .iter()
        .filter(|w| w.category == "defense_range")
        .collect();
    assert!(
        defense_warnings.is_empty(),
        "Rare belt with defense 19 within possibility envelope should not warn, got: {:?}",
        defense_warnings
    );
}

#[test]
fn epistemic_tier_separates_hard_invariant_from_evolving_hypothesis() {
    use d2r_core::engine::validation::WarningConfidenceTier;

    let mut item = base_item();
    item.code = "hbl ".to_string();
    item.quality = Some(ItemQuality::Rare);
    item.defense = Some(200); // Exceeds even rare possibility envelope (11+60=71)
    item.rare_name_1 = Some(1);
    item.rare_name_2 = Some(2);

    let result = validate_item(&item).expect("rare item should validate");
    let defense_warning = result
        .detailed_warnings
        .iter()
        .find(|w| w.category == "defense_range");
    assert!(defense_warning.is_some());
    assert_eq!(
        defense_warning.unwrap().tier,
        WarningConfidenceTier::EvolvingHypothesis,
        "Rare item defense out-of-range should be tagged as EvolvingHypothesis"
    );
}

#[test]
fn rare_item_with_affix_property_suppresses_staffmod_warning() {
    let mut item = base_item();
    item.code = "buc ".to_string(); // Buckler, type "shie" -> "armo"
    item.level = Some(1); // ilvl 1
    item.quality = Some(ItemQuality::Rare);
    // Rare affix: Cobalt prefix (id 68: coldresist 21..30)
    item.rare_affixes[0] = Some(68);
    item.properties.push(prop(43, 0, 25)); // +25 Cold Resist

    let result = validate_item(&item).expect("rare item should validate");
    let staffmod_warnings: Vec<_> = result
        .detailed_warnings
        .iter()
        .filter(|w| w.category == "staffmod_ilvl")
        .collect();
    assert!(
        staffmod_warnings.is_empty(),
        "Property explained by rolled affix should not trigger automagic/staffmod warning, got: {:?}",
        staffmod_warnings
    );
}

