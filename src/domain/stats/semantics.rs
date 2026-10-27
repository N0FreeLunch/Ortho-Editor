use crate::data::char_stats::CHAR_STATS;
use crate::data::monsters::{MONSTERS, MONSTER_TYPES};
use crate::data::skills::SKILLS;
use serde::{Deserialize, Serialize};

use crate::domain::stats::ItemProperty;
use crate::engine::formatter::{format_template, get_loc};

/// Semantic interpretation of item properties (skills, skill tabs, class skills, charges, auras, monster bonuses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertySemantics {
    /// Class-specific skill bonus (Stat 83: `item_addclassskills`).
    ClassSkills {
        class_id: usize,
        class_name_en: String,
        class_name_ko: String,
        level: i32,
    },
    /// Skill tab bonus (Stat 188: `item_addskill_tab`).
    SkillTab {
        class_id: usize,
        class_name_en: String,
        class_name_ko: String,
        tab_index: usize,
        tab_name_en: String,
        tab_name_ko: String,
        tab_title_en: String,
        tab_title_ko: String,
        level: i32,
    },
    /// Individual skill bonus (Stat 107: `item_singleskill`, Stat 84: `item_nonclassskill`).
    SingleSkill {
        skill_id: u32,
        skill_name_en: String,
        skill_name_ko: String,
        level: i32,
    },
    /// Non-class skill bonus / Oskill (Stat 97: `item_nonclassskill`).
    NonClassSkill {
        skill_id: u32,
        skill_name_en: String,
        skill_name_ko: String,
        level: i32,
    },
    /// Aura granted when equipped (Stat 151: `item_aura`).
    Aura {
        skill_id: u32,
        skill_name_en: String,
        skill_name_ko: String,
        level: i32,
    },
    /// Elemental skill bonus (Stat 126: `item_elemskill`).
    ElementalSkills {
        element_id: u32,
        element_name_en: String,
        element_name_ko: String,
        level: i32,
    },
    /// Attack rating or damage bonus against monster types (Stat 179: `attack_vs_montype`, Stat 180: `damage_vs_montype`).
    MonsterBonus {
        montype_id: u32,
        montype_name_en: String,
        montype_name_ko: String,
        is_attack: bool,
        value: i32,
    },
    /// Reanimate as monster on kill (Stat 155: `item_reanimate`).
    Reanimate {
        monster_id: u32,
        monster_name_en: String,
        monster_name_ko: String,
        chance_percent: i32,
    },
    /// Charged skill (Stat 204: `item_charged_skill`).
    ChargedSkill {
        skill_id: u32,
        skill_name_en: String,
        skill_name_ko: String,
        level: u32,
        current_charges: u32,
        max_charges: u32,
    },
    /// Chance to cast on striking / attacking / struck (Stat 195, 196, 197, 198, 199, 201).
    ChanceToCast {
        skill_id: u32,
        skill_name_en: String,
        skill_name_ko: String,
        level: u32,
        chance_percent: i32,
        trigger: String,
    },
}

/// Decodes packed or absolute skill tab parameter into `(class_id, tab_index)`.
pub fn decode_skill_tab_param(param: u32) -> (usize, usize) {
    let packed_class = (param >> 3) as usize;
    let packed_tab = (param & 0x7) as usize;
    if packed_class < CHAR_STATS.len() && packed_tab < 3 {
        return (packed_class, packed_tab);
    }

    let absolute = (param & 0xFF) as usize;
    (absolute / 3, absolute % 3)
}

/// Decodes packed skill parameter into `(skill_id, skill_level)`.
pub fn decode_skill_param(param: u32) -> (u32, u32) {
    let skill_level = param & 0x3F;
    let skill_id = param >> 6;
    (skill_id, skill_level)
}

/// Returns clean human-readable title for a skill tab (English, Korean).
pub fn skill_tab_clean_title(class_id: usize, tab_index: usize) -> Option<(&'static str, &'static str)> {
    match (class_id, tab_index) {
        // Amazon (0)
        (0, 0) => Some(("Bow and Crossbow", "보우 & 크로스보우")),
        (0, 1) => Some(("Passive and Magic", "패시브 & 마법")),
        (0, 2) => Some(("Javelin and Spear", "재벌린 & 스피어")),

        // Sorceress (1)
        (1, 0) => Some(("Fire Spells", "파이어 스펠")),
        (1, 1) => Some(("Lightning Spells", "라이트닝 스펠")),
        (1, 2) => Some(("Cold Spells", "콜드 스펠")),

        // Necromancer (2)
        (2, 0) => Some(("Curses", "저주")),
        (2, 1) => Some(("Poison and Bone", "포이즌 & 본")),
        (2, 2) => Some(("Summoning", "소환")),

        // Paladin (3)
        (3, 0) => Some(("Combat Skills", "전투 스킬")),
        (3, 1) => Some(("Offensive Auras", "공격 오오라")),
        (3, 2) => Some(("Defensive Auras", "방어 오오라")),

        // Barbarian (4)
        (4, 0) => Some(("Combat Skills", "전투 스킬")),
        (4, 1) => Some(("Combat Masteries", "전투 마스터리")),
        (4, 2) => Some(("Warcries", "함성")),

        // Druid (5)
        (5, 0) => Some(("Summoning", "소환")),
        (5, 1) => Some(("Shape Shifting", "변신")),
        (5, 2) => Some(("Elemental", "원소")),

        // Assassin (6)
        (6, 0) => Some(("Traps", "트랩")),
        (6, 1) => Some(("Shadow Disciplines", "쉐도우 디서플린")),
        (6, 2) => Some(("Martial Arts", "마샬 아트")),

        // Warlock (7)
        (7, 0) => Some(("Demon", "악마")),
        (7, 1) => Some(("Eldritch", "엘드리치")),
        (7, 2) => Some(("Chaos", "혼돈")),

        _ => None,
    }
}

/// Retrieves localized template for a skill tab.
pub fn skill_tab_template(class_id: usize, tab_index: usize, language: &str) -> Option<&'static str> {
    CHAR_STATS
        .get(class_id)
        .and_then(|stats| stats.skill_tabs.get(tab_index))
        .map(|&key| get_loc(key, language))
}

/// Retrieves localized name for a skill ID.
pub fn skill_name(skill_id: u32, language: &str) -> String {
    if let Some(skill) = SKILLS.iter().find(|s| s.id == skill_id) {
        return get_loc(skill.key, language).to_string();
    }
    if language == "ko" {
        format!("기술 #{}", skill_id)
    } else {
        format!("Skill #{}", skill_id)
    }
}

/// Retrieves human-readable name for monster type ID.
pub fn monster_type_name(montype_id: u32, language: &str) -> String {
    let (en, ko) = match montype_id {
        1 => ("Undead", "언데드"),
        2 => ("Demons", "악마"),
        3 => ("Insects", "곤충"),
        4 => ("Humans", "인간"),
        5 => ("Constructs", "구조물"),
        6 => ("Low Undead", "하급 언데드"),
        7 => ("High Undead", "상급 언데드"),
        8 => ("Skeletons", "해골"),
        9 => ("Zombies", "좀비"),
        10 => ("Big Heads", "빅헤드"),
        11 => ("Foul Crows", "파울 크로우"),
        12 => ("Fallens", "폴른"),
        13 => ("Brutes", "브루트"),
        14 => ("Sand Raiders", "샌드 레이더"),
        15 => ("Wraiths", "레이스"),
        16 => ("Corrupt Rogues", "타락한 로그"),
        17 => ("Baboons", "바분"),
        18 => ("Goatmen", "염소인간"),
        19 => ("Quill Rats", "가시 쥐"),
        20 => ("Sand Maggots", "모래 구더기"),
        21 => ("Claw Vipers", "클로 바이퍼"),
        22 => ("Sand Leapers", "샌드 리퍼"),
        23 => ("Panther Women", "판서 우먼"),
        24 => ("Swarms", "스웜"),
        25 => ("Scarabs", "스카라브"),
        _ => {
            if let Some(m) = MONSTER_TYPES.iter().find(|m| m.id == montype_id) {
                return m.key.to_string();
            }
            if language == "ko" {
                return format!("몬스터 유형 #{}", montype_id);
            } else {
                return format!("Monster Type #{}", montype_id);
            }
        }
    };
    if language == "ko" {
        ko.to_string()
    } else {
        en.to_string()
    }
}

/// Retrieves human-readable name for monster ID.
pub fn monster_name(monster_id: u32, language: &str) -> String {
    if let Some(m) = MONSTERS.iter().find(|m| m.id == monster_id) {
        let loc = get_loc(m.key, language);
        if !loc.is_empty() && loc != m.key {
            return loc.to_string();
        }
        return m.key.to_string();
    }
    if language == "ko" {
        format!("몬스터 #{}", monster_id)
    } else {
        format!("Monster #{}", monster_id)
    }
}

/// Decodes semantic meaning from an `ItemProperty`.
pub fn decode_property_semantics(prop: &ItemProperty) -> Option<PropertySemantics> {
    decode_stat_semantics(prop.stat_id, prop.param, prop.value)
}

/// Decodes semantic meaning from raw stat ID, param, and value.
pub fn decode_stat_semantics(stat_id: u32, param: u32, value: i32) -> Option<PropertySemantics> {
    match stat_id {
        83 => {
            // item_addclassskills
            let class_id = param as usize;
            let stats = CHAR_STATS.get(class_id)?;
            let class_name_en = get_loc(stats.class, "en").to_string();
            let class_name_ko = get_loc(stats.class, "ko").to_string();
            Some(PropertySemantics::ClassSkills {
                class_id,
                class_name_en,
                class_name_ko,
                level: value,
            })
        }
        188 => {
            // item_addskill_tab
            let (class_id, tab_index) = decode_skill_tab_param(param);
            let stats = CHAR_STATS.get(class_id)?;
            let class_name_en = get_loc(stats.class, "en").to_string();
            let class_name_ko = get_loc(stats.class, "ko").to_string();

            let (tab_title_en, tab_title_ko) = skill_tab_clean_title(class_id, tab_index)
                .map(|(en, ko)| (en.to_string(), ko.to_string()))
                .unwrap_or_else(|| (format!("Tab #{}", tab_index), format!("탭 #{}", tab_index)));

            let tab_tmpl_en = skill_tab_template(class_id, tab_index, "en")
                .unwrap_or("%+d to Skills");
            let tab_tmpl_ko = skill_tab_template(class_id, tab_index, "ko")
                .unwrap_or("+%d 스킬");

            let tab_name_en = format_template(tab_tmpl_en, &[value.to_string()]);
            let tab_name_ko = format_template(tab_tmpl_ko, &[value.to_string()]);

            Some(PropertySemantics::SkillTab {
                class_id,
                class_name_en,
                class_name_ko,
                tab_index,
                tab_name_en,
                tab_name_ko,
                tab_title_en,
                tab_title_ko,
                level: value,
            })
        }
        84 | 107 => {
            // item_nonclassskill or item_singleskill
            let skill_id = param;
            let skill_name_en = skill_name(skill_id, "en");
            let skill_name_ko = skill_name(skill_id, "ko");
            Some(PropertySemantics::SingleSkill {
                skill_id,
                skill_name_en,
                skill_name_ko,
                level: value,
            })
        }
        97 => {
            // item_nonclassskill (Oskill, e.g. Battle Orders, Teleport)
            let skill_id = param;
            let skill_name_en = skill_name(skill_id, "en");
            let skill_name_ko = skill_name(skill_id, "ko");
            Some(PropertySemantics::NonClassSkill {
                skill_id,
                skill_name_en,
                skill_name_ko,
                level: value,
            })
        }
        151 => {
            // item_aura (e.g. Conviction, Fanaticism, Holy Fire when equipped)
            let skill_id = param;
            let skill_name_en = skill_name(skill_id, "en");
            let skill_name_ko = skill_name(skill_id, "ko");
            Some(PropertySemantics::Aura {
                skill_id,
                skill_name_en,
                skill_name_ko,
                level: value,
            })
        }
        126 => {
            // item_elemskill (e.g. +1 to Fire Skills on Magefist)
            let (elem_en, elem_ko) = match param {
                0 => ("Fire", "화염"),
                1 => ("Lightning", "번개"),
                2 => ("Magic", "마법"),
                3 => ("Cold", "냉기"),
                4 => ("Poison", "독"),
                _ => ("Elemental", "원소"),
            };
            Some(PropertySemantics::ElementalSkills {
                element_id: param,
                element_name_en: elem_en.to_string(),
                element_name_ko: elem_ko.to_string(),
                level: value,
            })
        }
        179 | 180 => {
            // attack_vs_montype (179) / damage_vs_montype (180)
            let montype_name_en = monster_type_name(param, "en");
            let montype_name_ko = monster_type_name(param, "ko");
            Some(PropertySemantics::MonsterBonus {
                montype_id: param,
                montype_name_en,
                montype_name_ko,
                is_attack: stat_id == 179,
                value,
            })
        }
        155 => {
            // item_reanimate (e.g. Tomb Reaver: 10% Reanimate as Returned)
            let monster_name_en = monster_name(param, "en");
            let monster_name_ko = monster_name(param, "ko");
            Some(PropertySemantics::Reanimate {
                monster_id: param,
                monster_name_en,
                monster_name_ko,
                chance_percent: value,
            })
        }
        204 => {
            // item_charged_skill
            let (skill_id, skill_level) = decode_skill_param(param);
            let skill_name_en = skill_name(skill_id, "en");
            let skill_name_ko = skill_name(skill_id, "ko");
            let cur_charges = (value & 0xFF) as u32;
            let max_charges = ((value >> 8) & 0xFF) as u32;
            Some(PropertySemantics::ChargedSkill {
                skill_id,
                skill_name_en,
                skill_name_ko,
                level: skill_level,
                current_charges: cur_charges,
                max_charges,
            })
        }
        195..=201 => {
            // Chance to cast
            let (skill_id, skill_level) = decode_skill_param(param);
            let skill_name_en = skill_name(skill_id, "en");
            let skill_name_ko = skill_name(skill_id, "ko");
            let trigger = match stat_id {
                195 => "on_attack",
                196 => "on_kill",
                197 => "on_death",
                198 => "on_striking",
                199 => "on_levelup",
                201 => "when_struck",
                _ => "custom",
            };
            Some(PropertySemantics::ChanceToCast {
                skill_id,
                skill_name_en,
                skill_name_ko,
                level: skill_level,
                chance_percent: value,
                trigger: trigger.to_string(),
            })
        }
        _ => None,
    }
}
