//! Integration tests for semantic skill options (Class Skills, Skill Tabs, Single Skills, Charges).
//! Verified synthetically in-memory without requiring external .d2s fixture files.

use d2r_core::domain::item::entity::ItemBitRange;
use d2r_core::domain::stats::semantics::{
    decode_skill_param, decode_skill_tab_param, decode_stat_semantics,
    skill_tab_clean_title, PropertySemantics,
};
use d2r_core::domain::stats::ItemProperty;
use d2r_core::engine::formatter::{
    format_item, format_properties_list_with_mode, format_socketed_child, StatDisplayMode,
};
use d2r_core::engine::socket_effects::resolve_socketed_child_properties;
use d2r_core::item::{Item, ItemQuality};

#[test]
fn test_all_8_classes_class_skills_semantics() {
    let expected_classes = [
        (0, "Amazon", "아마존"),
        (1, "Sorceress", "소서리스"),
        (2, "Necromancer", "네크로맨서"),
        (3, "Paladin", "팔라딘"),
        (4, "Barbarian", "바바리안"),
        (5, "Druid", "드루이드"),
        (6, "Assassin", "어쎄신"),
        (7, "Warlock", "Warlock"),
    ];

    for (class_id, exp_en, exp_ko) in expected_classes {
        let sem = decode_stat_semantics(83, class_id as u32, 2)
            .expect(&format!("Class {} skills should decode", class_id));

        match sem {
            PropertySemantics::ClassSkills {
                class_id: cid,
                class_name_en,
                class_name_ko,
                level,
            } => {
                assert_eq!(cid, class_id);
                assert_eq!(class_name_en, exp_en);
                assert_eq!(class_name_ko, exp_ko);
                assert_eq!(level, 2);
            }
            _ => panic!("Expected ClassSkills for class {}", class_id),
        }
    }
}

#[test]
fn test_all_24_skill_tabs_coverage_and_dual_encoding() {
    let expected_tabs = [
        // Amazon (0)
        (0, 0, "Bow and Crossbow", "보우 & 크로스보우"),
        (0, 1, "Passive and Magic", "패시브 & 마법"),
        (0, 2, "Javelin and Spear", "재벌린 & 스피어"),
        // Sorceress (1)
        (1, 0, "Fire Spells", "파이어 스펠"),
        (1, 1, "Lightning Spells", "라이트닝 스펠"),
        (1, 2, "Cold Spells", "콜드 스펠"),
        // Necromancer (2)
        (2, 0, "Curses", "저주"),
        (2, 1, "Poison and Bone", "포이즌 & 본"),
        (2, 2, "Summoning", "소환"),
        // Paladin (3)
        (3, 0, "Combat Skills", "전투 스킬"),
        (3, 1, "Offensive Auras", "공격 오오라"),
        (3, 2, "Defensive Auras", "방어 오오라"),
        // Barbarian (4)
        (4, 0, "Combat Skills", "전투 스킬"),
        (4, 1, "Combat Masteries", "전투 마스터리"),
        (4, 2, "Warcries", "함성"),
        // Druid (5)
        (5, 0, "Summoning", "소환"),
        (5, 1, "Shape Shifting", "변신"),
        (5, 2, "Elemental", "원소"),
        // Assassin (6)
        (6, 0, "Traps", "트랩"),
        (6, 1, "Shadow Disciplines", "쉐도우 디서플린"),
        (6, 2, "Martial Arts", "마샬 아트"),
        // Warlock (7)
        (7, 0, "Demon", "악마"),
        (7, 1, "Eldritch", "엘드리치"),
        (7, 2, "Chaos", "혼돈"),
    ];

    assert_eq!(expected_tabs.len(), 24);

    for (class_id, tab_index, exp_title_en, exp_title_ko) in expected_tabs {
        // Test clean titles helper
        let (title_en, title_ko) = skill_tab_clean_title(class_id, tab_index)
            .expect(&format!("Clean title for class {} tab {}", class_id, tab_index));
        assert_eq!(title_en, exp_title_en);
        assert_eq!(title_ko, exp_title_ko);

        // Test packed param: (class << 3) | tab
        let packed_param = ((class_id as u32) << 3) | (tab_index as u32);
        let (dec_class_packed, dec_tab_packed) = decode_skill_tab_param(packed_param);
        assert_eq!(dec_class_packed, class_id);
        assert_eq!(dec_tab_packed, tab_index);

        let sem_packed = decode_stat_semantics(188, packed_param, 3)
            .expect(&format!("Packed tab param {} should decode", packed_param));

        match sem_packed {
            PropertySemantics::SkillTab {
                class_id: cid,
                tab_index: tid,
                tab_title_en,
                tab_title_ko,
                tab_name_en,
                tab_name_ko,
                level,
                ..
            } => {
                assert_eq!(cid, class_id);
                assert_eq!(tid, tab_index);
                assert_eq!(tab_title_en, exp_title_en);
                assert_eq!(tab_title_ko, exp_title_ko);
                assert!(!tab_name_en.is_empty());
                assert!(!tab_name_ko.is_empty());
                assert_eq!(level, 3);
            }
            _ => panic!("Expected SkillTab for packed param {}", packed_param),
        }

        // Test flat param: class * 3 + tab
        let flat_param = (class_id as u32) * 3 + (tab_index as u32);
        let (dec_class_flat, dec_tab_flat) = decode_skill_tab_param(flat_param);
        // For Amazon (0, 1, 2) packed and flat are identical (0..2 < 3)
        // For other classes, flat param resolves to correct class and tab
        if flat_param < 3 || flat_param % 8 >= 3 || (flat_param >> 3) >= 8 {
            assert_eq!(dec_class_flat, class_id);
            assert_eq!(dec_tab_flat, tab_index);
        }
    }
}

#[test]
fn test_item_property_semantics_extension_method() {
    // Amazon Javelin and Spear +3: stat 188, param 2, value 3
    let prop = ItemProperty::new(
        188,
        "item_addskill_tab".to_string(),
        2,
        3,
        3,
        ItemBitRange::default(),
    );

    let sem = prop.semantics().expect("ItemProperty::semantics should resolve");
    match sem {
        PropertySemantics::SkillTab {
            class_id,
            class_name_en,
            tab_index,
            tab_title_en,
            level,
            ..
        } => {
            assert_eq!(class_id, 0);
            assert_eq!(class_name_en, "Amazon");
            assert_eq!(tab_index, 2);
            assert_eq!(tab_title_en, "Javelin and Spear");
            assert_eq!(level, 3);
        }
        _ => panic!("Expected SkillTab variant"),
    }
}

#[test]
fn test_single_and_charged_skill_semantics() {
    // Single skill (stat 107): Magic Arrow (skill 6) +2
    let sem_single = decode_stat_semantics(107, 6, 2).expect("Single skill should decode");
    match sem_single {
        PropertySemantics::SingleSkill {
            skill_id,
            skill_name_en,
            level,
            ..
        } => {
            assert_eq!(skill_id, 6);
            assert_eq!(skill_name_en, "Magic Arrow");
            assert_eq!(level, 2);
        }
        _ => panic!("Expected SingleSkill variant"),
    }

    // Charged skill (stat 204): Skill 54 (Teleport), Level 3, 20/33 charges
    let param = (54 << 6) | 3;
    let (skill_id, skill_level) = decode_skill_param(param);
    assert_eq!(skill_id, 54);
    assert_eq!(skill_level, 3);

    let value = (33 << 8) | 20; // max=33, cur=20
    let sem_charge = decode_stat_semantics(204, param, value).expect("Charge skill should decode");
    match sem_charge {
        PropertySemantics::ChargedSkill {
            skill_id,
            skill_name_en,
            level,
            current_charges,
            max_charges,
            ..
        } => {
            assert_eq!(skill_id, 54);
            assert_eq!(skill_name_en, "Teleport");
            assert_eq!(level, 3);
            assert_eq!(current_charges, 20);
            assert_eq!(max_charges, 33);
        }
        _ => panic!("Expected ChargedSkill variant"),
    }
}

#[test]
fn test_json_serialization_payload_integrity() {
    let prop = ItemProperty::new(
        188,
        "item_addskill_tab".to_string(),
        (3 << 3) | 0, // Paladin Combat Skills
        2,
        2,
        ItemBitRange::default(),
    );

    let mut json_val = serde_json::to_value(&prop).expect("ItemProperty serialization");
    if let Some(sem) = prop.semantics() {
        json_val["semantics"] = serde_json::to_value(sem).expect("semantics serialization");
    }

    assert_eq!(json_val["stat_id"], 188);
    assert_eq!(json_val["name"], "item_addskill_tab");
    assert_eq!(json_val["semantics"]["kind"], "skill_tab");
    assert_eq!(json_val["semantics"]["class_name_en"], "Paladin");
    assert_eq!(json_val["semantics"]["class_name_ko"], "팔라딘");
    assert_eq!(json_val["semantics"]["tab_title_en"], "Combat Skills");
    assert_eq!(json_val["semantics"]["tab_title_ko"], "전투 스킬");
    assert_eq!(json_val["semantics"]["level"], 2);
}

#[test]
fn test_aura_and_elemental_skills_semantics() {
    // Aura: Stat 151 (item_aura), Param 123 (Conviction), Value 12
    let sem_aura = decode_stat_semantics(151, 123, 12).expect("Aura should decode");
    match sem_aura {
        PropertySemantics::Aura {
            skill_id,
            skill_name_en,
            skill_name_ko,
            level,
        } => {
            assert_eq!(skill_id, 123);
            assert_eq!(skill_name_en, "Conviction");
            assert_eq!(skill_name_ko, "컨빅션");
            assert_eq!(level, 12);
        }
        _ => panic!("Expected Aura variant"),
    }

    // Elemental Skills: Stat 126 (item_elemskill), Param 0 (Fire), Value 1
    let sem_elem = decode_stat_semantics(126, 0, 1).expect("ElementalSkills should decode");
    match sem_elem {
        PropertySemantics::ElementalSkills {
            element_id,
            element_name_en,
            element_name_ko,
            level,
        } => {
            assert_eq!(element_id, 0);
            assert_eq!(element_name_en, "Fire");
            assert_eq!(element_name_ko, "화염");
            assert_eq!(level, 1);
        }
        _ => panic!("Expected ElementalSkills variant"),
    }
}

#[test]
fn test_non_class_skill_semantics() {
    // NonClassSkill: Stat 97 (item_nonclassskill), Param 149 (Battle Orders), Value 6
    let sem_oskill = decode_stat_semantics(97, 149, 6).expect("NonClassSkill should decode");
    match sem_oskill {
        PropertySemantics::NonClassSkill {
            skill_id,
            skill_name_en,
            skill_name_ko,
            level,
        } => {
            assert_eq!(skill_id, 149);
            assert_eq!(skill_name_en, "Battle Orders");
            assert_eq!(skill_name_ko, "배틀 오더스");
            assert_eq!(level, 6);
        }
        _ => panic!("Expected NonClassSkill variant"),
    }
}

#[test]
fn test_monster_bonus_and_reanimate_semantics() {
    // MonsterBonus: Stat 179 (attack_vs_montype), Param 1 (Undead), Value 100
    let sem_bonus = decode_stat_semantics(179, 1, 100).expect("MonsterBonus should decode");
    match sem_bonus {
        PropertySemantics::MonsterBonus {
            montype_id,
            montype_name_en,
            montype_name_ko,
            is_attack,
            value,
        } => {
            assert_eq!(montype_id, 1);
            assert_eq!(montype_name_en, "Undead");
            assert_eq!(montype_name_ko, "언데드");
            assert!(is_attack);
            assert_eq!(value, 100);
        }
        _ => panic!("Expected MonsterBonus variant"),
    }

    // Reanimate: Stat 155 (item_reanimate), Param 1 (Returned), Value 10
    let sem_reanim = decode_stat_semantics(155, 1, 10).expect("Reanimate should decode");
    match sem_reanim {
        PropertySemantics::Reanimate {
            monster_id,
            monster_name_en,
            monster_name_ko,
            chance_percent,
        } => {
            assert_eq!(monster_id, 1);
            assert_eq!(monster_name_en, "Returned");
            assert_eq!(monster_name_ko, "리턴드");
            assert_eq!(chance_percent, 10);
        }
        _ => panic!("Expected Reanimate variant"),
    }
}

#[test]
fn test_item_quality_localization_and_superior_low_prefixes() {
    use d2r_core::domain::header::quality::ItemQuality;
    use d2r_core::engine::formatter::{format_item, quality_display_name, resolve_item_name};
    use d2r_core::item::Item;

    // 1. Localized quality names
    assert_eq!(quality_display_name(Some(ItemQuality::Low), "en"), "Low Quality");
    assert_eq!(quality_display_name(Some(ItemQuality::Low), "ko"), "로우 퀄리티");
    assert_eq!(quality_display_name(Some(ItemQuality::Normal), "en"), "Normal");
    assert_eq!(quality_display_name(Some(ItemQuality::Normal), "ko"), "노멀");
    assert_eq!(quality_display_name(Some(ItemQuality::High), "en"), "Superior");
    assert_eq!(quality_display_name(Some(ItemQuality::High), "ko"), "슈페리어");
    assert_eq!(quality_display_name(Some(ItemQuality::Magic), "en"), "Magic");
    assert_eq!(quality_display_name(Some(ItemQuality::Magic), "ko"), "매직");
    assert_eq!(quality_display_name(Some(ItemQuality::Rare), "en"), "Rare");
    assert_eq!(quality_display_name(Some(ItemQuality::Rare), "ko"), "레어");
    assert_eq!(quality_display_name(Some(ItemQuality::Set), "en"), "Set");
    assert_eq!(quality_display_name(Some(ItemQuality::Set), "ko"), "세트");
    assert_eq!(quality_display_name(Some(ItemQuality::Unique), "en"), "Unique");
    assert_eq!(quality_display_name(Some(ItemQuality::Unique), "ko"), "유니크");
    assert_eq!(quality_display_name(Some(ItemQuality::Crafted), "en"), "Crafted");
    assert_eq!(quality_display_name(Some(ItemQuality::Crafted), "ko"), "크래프트");

    // 2. Superior item prefix: Mage Plate (xtp)
    let mut superior_item = Item::empty_for_tests();
    superior_item.code = "xtp ".to_string();
    superior_item.quality = Some(ItemQuality::High);
    assert_eq!(resolve_item_name(&superior_item, "en"), "Superior Mage Plate");
    assert_eq!(resolve_item_name(&superior_item, "ko"), "슈페리어 메이지플레이트");

    let formatted_sup = format_item(&superior_item, "ko", 0, 99);
    assert_eq!(formatted_sup.quality_name, "슈페리어");
    assert_eq!(formatted_sup.name, "슈페리어 메이지플레이트");

    // 3. Low Quality item prefixes: Short Sword (ssd)
    let mut low_item = Item::empty_for_tests();
    low_item.code = "ssd ".to_string();
    low_item.quality = Some(ItemQuality::Low);

    // 0: Crude / `크루드`
    low_item.low_high_graphic_bits = Some(0);
    assert_eq!(resolve_item_name(&low_item, "en"), "Crude Short Sword");
    assert_eq!(resolve_item_name(&low_item, "ko"), "크루드 숏소드");

    // 1: Cracked / `크랙트`
    low_item.low_high_graphic_bits = Some(1);
    assert_eq!(resolve_item_name(&low_item, "en"), "Cracked Short Sword");
    assert_eq!(resolve_item_name(&low_item, "ko"), "크랙트 숏소드");

    // 2: Damaged / `데미지드`
    low_item.low_high_graphic_bits = Some(2);
    assert_eq!(resolve_item_name(&low_item, "en"), "Damaged Short Sword");
    assert_eq!(resolve_item_name(&low_item, "ko"), "데미지드 숏소드");

    // 3: Low Quality / `로우 퀄리티`
    low_item.low_high_graphic_bits = Some(3);
    assert_eq!(resolve_item_name(&low_item, "en"), "Low Quality Short Sword");
    assert_eq!(resolve_item_name(&low_item, "ko"), "로우 퀄리티 숏소드");
}

#[test]
fn test_socketed_gems_and_runes_slot_projection() {
    // 1. Chipped Ruby (`gcr`) in Weapon (Short Sword `ssd `)
    let mut weapon = Item::empty_for_tests();
    weapon.code = "ssd ".to_string();

    let mut chip_ruby = Item::empty_for_tests();
    chip_ruby.code = "gcr ".to_string();
    chip_ruby.header.is_compact = true;

    let props_weapon = resolve_socketed_child_properties(&chip_ruby, &weapon);
    assert_eq!(props_weapon.len(), 2);
    assert_eq!(props_weapon[0].stat_id, 48); // firemindam
    assert_eq!(props_weapon[0].value, 3);
    assert_eq!(props_weapon[1].stat_id, 49); // firemaxdam
    assert_eq!(props_weapon[1].value, 4);

    let lines_w_en = format_socketed_child(&chip_ruby, &weapon, "en", 99);
    assert_eq!(lines_w_en, vec!["Adds 3-4 fire damage"]);
    let lines_w_ko = format_socketed_child(&chip_ruby, &weapon, "ko", 99);
    assert_eq!(lines_w_ko, vec!["3 - 4 파이어 데미지 추가"]);

    // 2. Chipped Ruby (`gcr`) in Helm (Cap `cap `)
    let mut helm = Item::empty_for_tests();
    helm.code = "cap ".to_string();

    let props_helm = resolve_socketed_child_properties(&chip_ruby, &helm);
    assert_eq!(props_helm.len(), 1);
    assert_eq!(props_helm[0].stat_id, 7); // maxhp
    assert_eq!(props_helm[0].value, 10);

    let lines_h_en = format_socketed_child(&chip_ruby, &helm, "en", 99);
    assert_eq!(lines_h_en.len(), 1);
    assert!(lines_h_en[0].contains("10") && lines_h_en[0].contains("Life"));

    // 3. Chipped Ruby (`gcr`) in Shield (Buckler `buc `)
    let mut shield = Item::empty_for_tests();
    shield.code = "buc ".to_string();

    let props_shield = resolve_socketed_child_properties(&chip_ruby, &shield);
    assert_eq!(props_shield.len(), 1);
    assert_eq!(props_shield[0].stat_id, 39); // fireresist
    assert_eq!(props_shield[0].value, 12);

    let lines_s_en = format_socketed_child(&chip_ruby, &shield, "en", 99);
    assert_eq!(lines_s_en.len(), 1);
    assert!(lines_s_en[0].contains("12%") && lines_s_en[0].contains("Fire Resist"));

    // 4. El Rune (`r01`) in Weapon vs Armor (Quilted Armor `qui `)
    let mut el_rune = Item::empty_for_tests();
    el_rune.code = "r01 ".to_string();
    el_rune.header.is_compact = true;

    let props_el_w = resolve_socketed_child_properties(&el_rune, &weapon);
    assert_eq!(props_el_w.len(), 2);
    assert_eq!(props_el_w[0].stat_id, 89); // item_lightradius
    assert_eq!(props_el_w[0].value, 1);
    assert_eq!(props_el_w[1].stat_id, 19); // item_tohit
    assert_eq!(props_el_w[1].value, 50);

    let lines_el_w_en = format_socketed_child(&el_rune, &weapon, "en", 99);
    assert!(lines_el_w_en.iter().any(|l| l.contains("50") && l.contains("Attack Rating")));
    assert!(lines_el_w_en.iter().any(|l| l.contains("1") && l.contains("Light Radius")));

    let mut armor = Item::empty_for_tests();
    armor.code = "qui ".to_string();

    let props_el_a = resolve_socketed_child_properties(&el_rune, &armor);
    assert_eq!(props_el_a.len(), 2);
    assert_eq!(props_el_a[0].stat_id, 89); // item_lightradius
    assert_eq!(props_el_a[0].value, 1);
    assert_eq!(props_el_a[1].stat_id, 31); // item_defense
    assert_eq!(props_el_a[1].value, 15);

    let lines_el_a_en = format_socketed_child(&el_rune, &armor, "en", 99);
    assert!(lines_el_a_en.iter().any(|l| l.contains("15") && l.contains("Defense")));
    assert!(lines_el_a_en.iter().any(|l| l.contains("1") && l.contains("Light Radius")));

    // 5. Jewel (`jew`) preserving rolled properties
    let mut jewel = Item::empty_for_tests();
    jewel.code = "jew ".to_string();
    jewel.header.is_compact = false;
    jewel.properties.push(ItemProperty::new(
        25, // damagepercent (Enhanced Damage)
        "damagepercent".to_string(),
        0,
        20,
        20,
        ItemBitRange::default(),
    ));

    let props_jew = resolve_socketed_child_properties(&jewel, &weapon);
    assert_eq!(props_jew.len(), 1);
    assert_eq!(props_jew[0].stat_id, 25);
    assert_eq!(props_jew[0].value, 20);

    let lines_jew = format_socketed_child(&jewel, &weapon, "en", 99);
    assert_eq!(lines_jew.len(), 1);
    assert!(lines_jew[0].contains("+20%") && lines_jew[0].contains("Enhanced Damage"));
}

#[test]
fn test_set_item_piece_bonuses_and_group_semantics() {
    // Civerb's Cudgel (`gsc `): id 2, set_id "Civerb's Vestments"
    let mut cudgel = Item::empty_for_tests();
    cudgel.code = "gsc ".to_string();
    cudgel.quality = Some(ItemQuality::Set);
    cudgel.unique_id = Some(2);
    cudgel.is_identified = true;

    // Piece set bonus: e.g. +100 to Attack Rating (stat 19) at 2 pieces
    cudgel.set_attributes.push(vec![ItemProperty::new(
        19,
        "item_tohit".to_string(),
        0,
        100,
        100,
        ItemBitRange::default(),
    )]);

    // 1. Single piece equipped (active_set_count = 1)
    let f1_en = format_item(&cudgel, "en", 1, 99);
    let f1_ko = format_item(&cudgel, "ko", 1, 99);

    assert_eq!(f1_en.set_name, Some("Civerb's Vestments".to_string()));
    assert_eq!(f1_ko.set_name, Some("시버브 베스트먼트".to_string()));

    assert_eq!(f1_en.set_pieces.len(), 3);
    assert!(f1_en.set_pieces.contains(&"Civerb's Cudgel".to_string()));
    assert!(f1_en.set_pieces.contains(&"Civerb's Icon".to_string()));
    assert!(f1_en.set_pieces.contains(&"Civerb's Ward".to_string()));

    assert_eq!(f1_ko.set_pieces.len(), 3);
    assert!(f1_ko.set_pieces.contains(&"시버브의 커질".to_string()));
    assert!(f1_ko.set_pieces.contains(&"시버브의 아이콘".to_string()));
    assert!(f1_ko.set_pieces.contains(&"시버브의 워드".to_string()));

    // Piece bonus: 1 bonus requiring 2 items, inactive
    assert_eq!(f1_en.piece_set_bonuses.len(), 1);
    assert_eq!(f1_en.piece_set_bonuses[0].required_count, 2);
    assert!(!f1_en.piece_set_bonuses[0].active);
    assert!(f1_en.piece_set_bonuses[0].lines[0].contains("100") && f1_en.piece_set_bonuses[0].lines[0].contains("Attack Rating"));

    // Group bonuses: partial (2) and full (3), both inactive
    assert_eq!(f1_en.set_bonuses.len(), 2);
    assert_eq!(f1_en.set_bonuses[0].required_count, 2);
    assert!(!f1_en.set_bonuses[0].active);
    assert_eq!(f1_en.set_bonuses[1].required_count, 3);
    assert!(!f1_en.set_bonuses[1].active);

    // 2. Two pieces equipped (active_set_count = 2)
    let f2_en = format_item(&cudgel, "en", 2, 99);
    assert!(f2_en.piece_set_bonuses[0].active);
    assert!(f2_en.set_bonuses[0].active); // partial active
    assert!(!f2_en.set_bonuses[1].active); // full inactive

    // 3. Full set equipped (active_set_count = 3)
    let f3_en = format_item(&cudgel, "en", 3, 99);
    assert!(f3_en.piece_set_bonuses[0].active);
    assert!(f3_en.set_bonuses[0].active); // partial active
    assert!(f3_en.set_bonuses[1].active); // full active
}

#[test]
fn test_runeword_base_qualification_and_sequence_validation() {
    use d2r_core::engine::validation::{check_runeword_legitimacy_detailed, WarningConfidenceTier};
    use d2r_core::item::{Item, ItemQuality};

    // Helper to build a socketed rune item
    fn rune_item(code: &str) -> Item {
        let mut r = Item::empty_for_tests();
        r.code = format!("{:<4}", code);
        r
    }

    // 1. Valid Enigma (Jah + Ith + Ber in Archon Plate)
    // Runeword id 32 = Enigma (r31, r06, r30), item_types: ["tors"]
    let mut enigma = Item::empty_for_tests();
    enigma.is_runeword = true;
    enigma.runeword_id = Some(32);
    enigma.code = "utp ".to_string(); // Archon Plate (code: "utp", item_type: "tors")
    enigma.quality = Some(ItemQuality::Normal);
    enigma.sockets = Some(3);
    enigma.socketed_items = vec![rune_item("r31"), rune_item("r06"), rune_item("r30")];

    let warnings = check_runeword_legitimacy_detailed(&enigma);
    assert!(warnings.is_empty(), "Valid Enigma should have no warnings: {:?}", warnings);

    // 2. Base Quality Violation: Magic quality cannot form a runeword
    let mut magic_enigma = enigma.clone();
    magic_enigma.quality = Some(ItemQuality::Magic);
    let warnings = check_runeword_legitimacy_detailed(&magic_enigma);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].category, "runeword_base_quality");
    assert_eq!(warnings[0].tier, WarningConfidenceTier::SettledContract);
    assert!(warnings[0].message.contains("cannot be made in an item of quality 'Magic'"));

    // 3. Socket Count Mismatch: 4 sockets instead of 3
    let mut four_sock = enigma.clone();
    four_sock.sockets = Some(4);
    let warnings = check_runeword_legitimacy_detailed(&four_sock);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].category, "runeword_socket_count");
    assert_eq!(warnings[0].tier, WarningConfidenceTier::SettledContract);
    assert!(warnings[0].message.contains("requires 3 sockets, but item has 4"));

    // 4. Ineligible Item Type: Hand Axe (axe/weap) cannot form Enigma (tors)
    let mut axe_enigma = enigma.clone();
    axe_enigma.code = "hax ".to_string();
    let warnings = check_runeword_legitimacy_detailed(&axe_enigma);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].category, "runeword_item_type");
    assert_eq!(warnings[0].tier, WarningConfidenceTier::SettledContract);
    assert!(warnings[0].message.contains("not eligible for item type 'hax'"));

    // 5. Rune Sequence Violation: Jah + Ber + Ith (r31, r30, r06)
    let mut wrong_order = enigma.clone();
    wrong_order.socketed_items = vec![rune_item("r31"), rune_item("r30"), rune_item("r06")];
    let warnings = check_runeword_legitimacy_detailed(&wrong_order);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].category, "runeword_rune_sequence");
    assert_eq!(warnings[0].tier, WarningConfidenceTier::SettledContract);
    assert!(warnings[0].message.contains("incorrect rune sequence: expected 'r06' at socket position 2, found 'r30'"));
}

#[test]
fn test_dual_mode_stat_fusion_and_poison_formula() {
    let make_prop = |stat_id: u32, name: &str, val: i32| {
        ItemProperty::new(stat_id, name.to_string(), 0, val, val, ItemBitRange::default())
    };

    // 1. Test All Resistances Fusion (stats 39, 41, 43, 45)
    let res_props = vec![
        make_prop(39, "fireresist", 15),
        make_prop(41, "lightresist", 15),
        make_prop(43, "coldresist", 15),
        make_prop(45, "poisonresist", 15),
    ];

    let fused_res_en = format_properties_list_with_mode(&res_props, 1, "en", StatDisplayMode::Fused);
    let fused_res_ko = format_properties_list_with_mode(&res_props, 1, "ko", StatDisplayMode::Fused);
    assert_eq!(fused_res_en, vec!["All Resistances +15"]);
    assert_eq!(fused_res_ko, vec!["모든 저항력 +15"]);

    let discrete_res_en = format_properties_list_with_mode(&res_props, 1, "en", StatDisplayMode::Discrete);
    assert_eq!(discrete_res_en.len(), 4);
    assert!(discrete_res_en.iter().any(|s| s.contains("Fire Resist +15%")));
    assert!(discrete_res_en.iter().any(|s| s.contains("Lightning Resist +15%")));
    assert!(discrete_res_en.iter().any(|s| s.contains("Cold Resist +15%")));
    assert!(discrete_res_en.iter().any(|s| s.contains("Poison Resist +15%")));

    // 2. Test All Attributes Fusion (stats 0, 1, 2, 3)
    let attr_props = vec![
        make_prop(0, "strength", 5),
        make_prop(1, "energy", 5),
        make_prop(2, "dexterity", 5),
        make_prop(3, "vitality", 5),
    ];

    let fused_attr_en = format_properties_list_with_mode(&attr_props, 1, "en", StatDisplayMode::Fused);
    let fused_attr_ko = format_properties_list_with_mode(&attr_props, 1, "ko", StatDisplayMode::Fused);
    assert_eq!(fused_attr_en, vec!["+5 to all Attributes"]);
    assert_eq!(fused_attr_ko, vec!["+5 모든 능력치"]);

    let discrete_attr_en = format_properties_list_with_mode(&attr_props, 1, "en", StatDisplayMode::Discrete);
    assert_eq!(discrete_attr_en.len(), 4);
    assert!(discrete_attr_en.iter().any(|s| s.contains("+5 to Strength")));
    assert!(discrete_attr_en.iter().any(|s| s.contains("+5 to Energy")));
    assert!(discrete_attr_en.iter().any(|s| s.contains("+5 to Dexterity")));
    assert!(discrete_attr_en.iter().any(|s| s.contains("+5 to Vitality")));

    // 3. Test Poison Damage In-Game Verification Formula
    // Case A: Venom Grip (min=153, max=153, frames=100) -> 60 total over 4 seconds
    let venom_grip_props = vec![
        make_prop(57, "poisonmindam", 153),
        make_prop(58, "poisonmaxdam", 153),
        make_prop(59, "poisonlength", 100),
    ];
    let vg_fused_en = format_properties_list_with_mode(&venom_grip_props, 1, "en", StatDisplayMode::Fused);
    let vg_fused_ko = format_properties_list_with_mode(&venom_grip_props, 1, "ko", StatDisplayMode::Fused);
    assert_eq!(vg_fused_en, vec!["+59 poison damage over 4 seconds"]); // 153 * 100 / 256 = 59
    assert_eq!(vg_fused_ko, vec!["+59 포이즌 데미지 (4 초 이상)"]);

    // Case B: Discrete Mode maintains each raw component without collapsing
    let vg_discrete_en = format_properties_list_with_mode(&venom_grip_props, 1, "en", StatDisplayMode::Discrete);
    assert_eq!(vg_discrete_en.len(), 3);
}

#[test]
fn test_per_level_scaling_and_charged_proc_skill_semantics() {
    use d2r_core::domain::stats::entity::ItemProperty;
    use d2r_core::engine::formatter::format_property;

    let make_prop_with_param = |stat_id: u32, name: &str, param: u32, val: i32| {
        ItemProperty::new(stat_id, name.to_string(), param, val, val, ItemBitRange::default())
    };

    // 1. Per-Level Stat Scaling with Factor (Stat 216 item_hp_perlevel, raw=12, char_level=80)
    // 12 * 80 / 8 = 120, rate = +1.5 per level
    let hp_per_lvl = make_prop_with_param(216, "item_hp_perlevel", 0, 12);

    assert_eq!(
        format_property(&hp_per_lvl, 80, "en"),
        "+120 to Life (+1.5 Based on Character Level)"
    );
    assert_eq!(
        format_property(&hp_per_lvl, 80, "ko"),
        "+120 라이프 (+1.5 캐릭터 레벨에 비례해서)"
    );

    // 2. Chance to Cast on Attack (Stat 195: descfunc 15)
    // Left Hand Swing (skill 5), Level 3, 25% chance
    let proc_prop = make_prop_with_param(195, "item_skillonattack", (5 << 6) | 3, 25);

    assert_eq!(
        format_property(&proc_prop, 99, "en"),
        "25% Chance to cast level 3 Left Hand Swing on attack"
    );
    assert_eq!(
        format_property(&proc_prop, 99, "ko"),
        "25% 기회로 레벨 3 왼손 휘두르기  시전 (공격시)"
    );

    // 3. Charged Skill (Stat 204: descfunc 24)
    // Fire Arrow (skill 7), Level 2, 5/20 charges
    let charge_prop = make_prop_with_param(204, "item_charged_skill", (7 << 6) | 2, (20 << 8) | 5);

    assert_eq!(
        format_property(&charge_prop, 99, "en"),
        "Level 2 Fire Arrow (5/20 Charges)"
    );
    assert_eq!(
        format_property(&charge_prop, 99, "ko"),
        "레벨 2 파이어 애로우  (5/20 회)"
    );
}



