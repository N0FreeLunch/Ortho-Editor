//! Integration tests for semantic skill options (Class Skills, Skill Tabs, Single Skills, Charges).
//! Verified synthetically in-memory without requiring external .d2s fixture files.

use d2r_core::domain::item::entity::ItemBitRange;
use d2r_core::domain::stats::semantics::{
    decode_skill_param, decode_skill_tab_param, decode_stat_semantics,
    skill_tab_clean_title, PropertySemantics,
};
use d2r_core::domain::stats::ItemProperty;

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
