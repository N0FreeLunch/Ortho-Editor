use crate::data::affixes::{PREFIXES, SUFFIXES};
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::localization::LOCALIZATIONS;
// use crate::data::monsters::MONSTER_TYPES;
use crate::data::rare_names::{RARE_PREFIXES, RARE_SUFFIXES};
use crate::data::runewords::RUNEWORDS;
use crate::data::set_items::SET_ITEMS;
use crate::data::sets::SET_BONUSES;
use crate::data::skills::SKILLS;
use crate::data::stat_costs::STAT_COSTS;
use crate::data::unique_items::UNIQUE_ITEMS;
use crate::engine::validation::validate_item;
use crate::item::{Item, ItemBitRange, ItemProperty, ItemQuality};

use crate::data::char_stats::CHAR_STATS;

pub struct FormattedSetBonus {
    pub active: bool,
    pub required_count: u8,
    pub lines: Vec<String>,
}

pub struct FormattedItem {
    pub name: String,
    pub level: u8,
    pub quality_name: String,
    pub base_attributes: Vec<String>,
    pub properties: Vec<String>,
    pub set_bonuses: Vec<FormattedSetBonus>,
    pub warnings: Vec<String>,
}

pub fn format_item(item: &Item, language: &str, active_set_count: usize, char_level: u8) -> FormattedItem {
    let mut base_attributes = Vec::new();

    if let Some(def) = item.defense {
        let label = get_loc("ItemStats1h", language);
        if label.contains('%') {
            base_attributes.push(format_template(label, &[def.to_string()]));
        } else {
            base_attributes.push(format!("{} {}", label, def));
        }
    }

    if let (Some(cur), Some(max)) = (item.current_durability, item.max_durability) {
        if max > 0 {
            let label = get_loc("ItemStats1d", language);
            let val_str = format!("{} / {}", cur, max);
            if label.contains('%') {
                base_attributes.push(format_template(label, &[val_str]));
            } else {
                base_attributes.push(format!("{} {}", label, val_str));
            }
        }
    }

    if let Some(qty) = item.quantity {
        let label = get_loc("ItemStats1i", language);
        if label.contains('%') {
            base_attributes.push(format_template(label, &[qty.to_string()]));
        } else {
            base_attributes.push(format!("{} {}", label, qty));
        }
    }

    if item.is_ethereal {
        base_attributes.push(get_loc("strethereal", language).to_string());
    }

    if let Some(s) = item.sockets {
        if s > 0 {
            let label = get_loc("Socketable", language);
            base_attributes.push(label.replace("%d", &s.to_string()));
        }
    }

    let mut properties: Vec<String> = item
        .properties
        .iter()
        .map(|p| format_property(p, char_level, language))
        .collect();

    if item.is_runeword {
        properties.extend(
            item.runeword_attributes
                .iter()
                .map(|p| format_property(p, char_level, language))
        );
    }

    let mut set_bonuses = Vec::new();
    if item.quality == Some(ItemQuality::Set) && item.is_identified {
        if let Some(id) = item.unique_id {
            if let Some(set_item) = SET_ITEMS.iter().find(|s| s.id == id as u32) {
                if let Some(group) = SET_BONUSES.iter().find(|g| g.index == set_item.set_id) {
                    let mut counts: Vec<u8> =
                        group.partial.iter().map(|p| p.required_count).collect();
                    counts.sort();
                    counts.dedup();

                    for count in counts {
                        let mut lines = Vec::new();
                        for bonus in group.partial.iter().filter(|p| p.required_count == count) {
                            let prop = ItemProperty {
                                stat_id: bonus.stat.stat_id,
                                name: String::new(), // Not used by formatter?
                                param: bonus.stat.param,
                                raw_value: bonus.stat.min,
                                value: bonus.stat.min,
                                range: ItemBitRange::default(),
                            };
                            lines.push(format_property(&prop, char_level, language));
                        }
                        set_bonuses.push(FormattedSetBonus {
                            active: active_set_count >= count as usize,
                            required_count: count,
                            lines,
                        });
                    }

                    if !group.full.is_empty() {
                        let total_pieces = SET_ITEMS
                            .iter()
                            .filter(|s| s.set_id == set_item.set_id)
                            .count();
                        let mut lines = Vec::new();
                        for stat in group.full {
                            let prop = ItemProperty {
                                stat_id: stat.stat_id,
                                name: String::new(),
                                param: stat.param,
                                raw_value: stat.min,
                                value: stat.min,
                                range: ItemBitRange::default(),
                            };
                            lines.push(format_property(&prop, char_level, language));
                        }
                        set_bonuses.push(FormattedSetBonus {
                            active: active_set_count >= total_pieces,
                            required_count: total_pieces as u8,
                            lines,
                        });
                    }
                }
            }
        }
    }

    let warnings = validate_item(item).map(|res| res.warnings).unwrap_or_default();

    FormattedItem {
        name: strip_d2_color_codes(&resolve_item_name(item, language)),
        level: item.level.unwrap_or(0),
        quality_name: format!("{:?}", item.quality),
        base_attributes,
        properties,
        set_bonuses,
        warnings,
    }
}

pub fn format_property(prop: &ItemProperty, char_level: u8, language: &str) -> String {
    let stat_id = prop.stat_id as u32;
    let Some(cost) = STAT_COSTS.get(stat_id as usize) else {
        return format!("Unknown Stat {} (value: {})", stat_id, prop.value);
    };

    let descfunc = cost.descfunc.unwrap_or(0);
    let key = if prop.value >= 0 {
        cost.descstrpos
    } else {
        cost.descstrneg
    };
    let Some(key_str) = key else {
        return format!("{} (value: {})", cost.name, prop.value);
    };

    let loc_str = get_loc(key_str, language).trim();
    
    // Slice 1: DescStr2 handling (e.g. Based on Character Level)
    let phrase2 = if let Some(d2) = cost.descstr2 {
        Some(get_loc(d2, language))
    } else {
        None
    };
    
    // Slice 2: Level Scaling logic utilizing op and op_param
    let mut display_value = prop.value;
    if let Some(op) = cost.op {
        if op != 0 {
            match op {
                2 | 4 | 5 => {
                    display_value = (prop.value * char_level as i32) >> cost.op_param.unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    
    let signed_value = format!("{:+}", display_value);

    let format_with_phrase2 = |base: String| -> String {
        if let Some(p2) = phrase2 {
            format!("{} {}", base, p2)
        } else {
            base
        }
    };

    let f = |base_fallback: String| -> String {
        if loc_str.contains('%') {
            format_with_phrase2(format_template(loc_str, &[display_value.to_string()]))
        } else {
            format_with_phrase2(base_fallback)
        }
    };

    match descfunc {
        1 | 19 => f(format!("{} {}", signed_value, loc_str)),
        2 => f(format!("{}% {}", display_value, loc_str)),
        3 => f(format!("{} {}", display_value, loc_str)),
        4 | 8 => f(format!("{}% {}", signed_value, loc_str)),
        5 => f(format!("{} {}%", loc_str, display_value)),
        6 => f(format!("{}% {}", signed_value, loc_str)),
        7 => f(format!("{}% {}", display_value, loc_str)),
        11 => f(format!("{} {}", display_value, loc_str)),
        12 => f(format!("{} {}", loc_str, signed_value)),
        13 => {
            let class_id = prop.param as usize;
            if class_id == 0 && !loc_str.starts_with("ItemModifier") {
                format!("{} {}", signed_value, loc_str)
            } else if let Some(stats) = CHAR_STATS.get(class_id) {
                let class_name = get_loc(stats.class, language);
                let mod_str_key = if !stats.all_skills.is_empty() {
                    stats.all_skills
                } else {
                    "strModAllSkillLevels"
                };
                let template = get_loc(mod_str_key, language);
                if template.contains('%') {
                    format_template(template, &[signed_value, class_name.to_string()])
                } else if language == "ko" {
                    format!("{} {} 스킬 레벨 상승", signed_value, class_name)
                } else {
                    format!("{} to {} Skill Levels", signed_value, class_name)
                }
            } else {
                format!("{} {} (class #{})", signed_value, loc_str, class_id)
            }
        }
        14 => {
            let (class_id, tab_index) = decode_skill_tab_param(prop.param);
            if class_id == 0 && tab_index == 0 && loc_str.contains('%') {
                format_template(loc_str, &[display_value.to_string()])
            } else if let Some(tab_name) = skill_tab_name(class_id, tab_index, language) {
                format_template(tab_name, &[display_value.to_string()])
            } else {
                format!(
                    "{} {} (class #{}, tab #{})",
                    signed_value, loc_str, class_id, tab_index
                )
            }
        }
        15 => {
            let (skill_id, skill_level) = decode_skill_param(prop.param);
            let skill = skill_name(skill_id, language);
            if loc_str.contains('%') {
                format_template(
                    loc_str,
                    &[
                        display_value.to_string(),
                        skill_level.to_string(),
                        skill.to_string(),
                    ],
                )
            } else {
                // descfunc 15 appends a descstr2 phrase (e.g. "on attack")
                let phrase2 = if let Some(d2) = cost.descstr2 {
                    get_loc(d2, language)
                } else {
                    get_loc("ItemExpansiveChancX", language) // Fallback to template if possible
                };
                
                let template = get_loc_optional("ItemExpansiveChancX", language).unwrap_or("%d%% Chance to cast level %d %s on attack");
                format_template(template, &[display_value.to_string(), skill_level.to_string(), skill, phrase2.to_string()])
            }
        }
        16 => {
            let skill = skill_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[display_value.to_string(), skill.to_string()])
            } else {
                let template = get_loc_optional("ModStr16", language).unwrap_or("Level %d %s Aura When Equipped");
                format_template(template, &[display_value.to_string(), skill])
            }
        }
        17 | 18 => {
            let label = if descfunc == 17 { loc_str } else { &format!("{}%", display_value) };
            let suffix = get_loc("itemstats-increasesovertime", language);
            if descfunc == 17 {
                format!("{} {} ({})", display_value, label, suffix)
            } else {
                format!("{} {} ({})", label, loc_str, suffix)
            }
        }
        20 => f(format!("-{}% {}", display_value.abs(), loc_str)),
        21 => f(format!("-{} {}", display_value.abs(), loc_str)),
        22 => {
            let monster = monster_type_name(prop.param, language);
            let base = if loc_str.contains('%') {
                format_template(loc_str, &[signed_value])
            } else if cost.name == "attack_vs_montype" {
                format!("{} {}", signed_value, loc_str)
            } else {
                format!("{}% {}", signed_value, loc_str)
            };
            format!("{} {}", base, monster)
        }
        23 => {
            let monster = if language == "ko" {
                format!("몬스터 #{}", prop.param)
            } else {
                format!("Monster #{}", prop.param)
            };
            format!("{}% {} {}", display_value, loc_str, monster)
        }
        24 => {
            let (skill_id, skill_level) = decode_skill_param(prop.param);
            let skill = skill_name(skill_id, language);
            let current_charges = prop.value & 0xFF;
            let max_charges = (prop.value >> 8) & 0xFF;
            format_skill_charges(skill_level, &skill, current_charges as u32, max_charges as u32, language)
        }
        27 => {
            let skill = skill_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[signed_value, skill])
            } else {
                let class_id = CHAR_STATS.iter().position(|s| {
                    cost.name.starts_with(&s.class.to_lowercase())
                });
                
                let only_text = if let Some(cid) = class_id {
                    get_loc(CHAR_STATS[cid].class_only, language)
                } else {
                    ""
                };
                format!("{} {} {}", signed_value, skill, only_text)
            }
        }
        28 => {
            let skill = skill_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[signed_value, skill])
            } else {
                format!("{} to {}", signed_value, skill)
            }
        }
        29 => {
            if cost.name == "damageresist" {
                // Damage reduction uses different keys for pos/neg
                let key = if prop.value >= 0 { "ModStr2uPercent" } else { "ModStr2uPercentNegative" };
                let label = get_loc(key, language);
                format_template(label, &[prop.value.abs().to_string()])
            } else if loc_str.contains('%') {
                format_template(loc_str, &[display_value.abs().to_string()])
            } else {
                format!("{} {}", signed_value, loc_str)
            }
        }
        _ => strip_d2_color_codes(&format!("{} (func {}): {}", loc_str, descfunc, display_value)),
    }
}

pub fn strip_d2_color_codes(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{00FF}' && i + 1 < chars.len() && chars[i + 1] == 'c' {
            i += 3; // Skip ÿ, c, and the color code character
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn skill_tab_name(class_id: usize, tab_index: usize, language: &str) -> Option<&'static str> {
    CHAR_STATS
        .get(class_id)
        .and_then(|stats| stats.skill_tabs.get(tab_index))
        .map(|&key| get_loc(key, language))
}

fn skill_name(skill_id: u32, language: &str) -> String {
    let key = SKILLS.iter().find(|s| s.id == skill_id).map(|s| s.key);
    if let Some(k) = key {
        return get_loc(k, language).to_string();
    }

    if language == "ko" {
        format!("기술 #{}", skill_id)
    } else {
        format!("Skill #{}", skill_id)
    }
}

fn monster_type_name(montype_id: u32, language: &str) -> String {
    /*
    let key = MONSTER_TYPES.iter().find(|m| m.id == montype_id).map(|m| m.key);
    if let Some(k) = key {
        return get_loc(k, language).to_string();
    }
    */

    if language == "ko" {
        format!("몬스터 유형 #{}", montype_id)
    } else {
        format!("Monster Type #{}", montype_id)
    }
}

fn decode_skill_param(param: u32) -> (u32, u32) {
    let skill_level = param & 0x3F;
    let skill_id = param >> 6;
    (skill_id, skill_level)
}

fn decode_skill_tab_param(param: u32) -> (usize, usize) {
    let packed_class = (param >> 3) as usize;
    let packed_tab = (param & 0x7) as usize;
    if packed_class < CHAR_STATS.len() && packed_tab < 3 {
        return (packed_class, packed_tab);
    }

    let absolute = (param & 0xFF) as usize;
    (absolute / 3, absolute % 3)
}

fn format_skill_charges(level: u32, skill: &str, cur: u32, max: u32, language: &str) -> String {
    let template = get_loc_optional("ModStre10d", language).unwrap_or("Level %d %s (%d/%d Charges)");
    if template.contains("%s") {
        format_template(template, &[level.to_string(), skill.to_string(), cur.to_string(), max.to_string()])
    } else {
        // Korean pattern like "(%d/%d 회)" -> needs skill/level prefix
        let level_label = get_loc("ModStre10b", language);
        let charges_formatted = format_template(template, &[cur.to_string(), max.to_string()]);
        format!("{} {} {} {}", level_label, level, skill, charges_formatted)
    }
}

fn format_template(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    let mut arg_idx = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            if i + 1 < chars.len() {
                match chars[i + 1] {
                    '%' => {
                        out.push('%');
                        i += 2;
                        continue;
                    }
                    'd' | 'i' | 'u' | 's' => {
                        if let Some(val) = args.get(arg_idx) {
                            out.push_str(val);
                            arg_idx += 1;
                        }
                        i += 2;
                        continue;
                    }
                    '+' => {
                        if i + 2 < chars.len() {
                            match chars[i + 2] {
                                'd' | 'i' | 'u' => {
                                    if let Some(val) = args.get(arg_idx) {
                                        if !val.starts_with('-') && !val.starts_with('+') {
                                            out.push('+');
                                        }
                                        out.push_str(val);
                                        arg_idx += 1;
                                    }
                                    i += 3;
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

pub fn resolve_item_name(item: &Item, language: &str) -> String {
    // 1. Runeword
    if item.is_runeword {
        if let Some(id) = item.runeword_id {
            let name_id = (id & 0x7FF) as u32;
            if let Some(rw) = RUNEWORDS.iter().find(|rw| rw.id == name_id) {
                return get_loc(rw.name, language).to_string();
            }
            let key = format!("Runeword{}", name_id);
            return get_loc(&key, language).to_string();
        }
    }

    // 2. Unique
    if item.quality == Some(ItemQuality::Unique) && item.is_identified {
        if let Some(id) = item.unique_id {
            if let Some(unique) = UNIQUE_ITEMS.iter().find(|u| u.id == id as u32) {
                return get_loc(unique.index, language).to_string();
            }
        }
    }

    // 3. Set
    if item.quality == Some(ItemQuality::Set) && item.is_identified {
        if let Some(id) = item.unique_id {
            if let Some(set_item) = SET_ITEMS.iter().find(|s| s.id == id as u32) {
                return get_loc(set_item.index, language).to_string();
            }
        }
    }

    // 4. Rare / Crafted
    if (item.quality == Some(ItemQuality::Rare) || item.quality == Some(ItemQuality::Crafted))
        && item.is_identified
    {
        if let (Some(pref), Some(suff)) = (item.rare_name_1, item.rare_name_2) {
            let p = RARE_PREFIXES.get(pref as usize).copied().unwrap_or("");
            let s = RARE_SUFFIXES.get(suff as usize).copied().unwrap_or("");

            let p_loc = get_loc(p, language);
            let s_loc = get_loc(s, language);

            if language == "ko" {
                return format!("{}{}", p_loc, s_loc);
            } else {
                return format!("{} {}", p_loc, s_loc);
            }
        }
    }

    // 5. Magic
    if item.quality == Some(ItemQuality::Magic) && item.is_identified {
        let mut name = String::new();
        if let Some(id) = item.magic_prefix {
            if let Some(affix) = PREFIXES.iter().find(|a| a.id == id as u32) {
                name.push_str(get_loc(affix.name, language));
            }
        }

        let base_name = ITEM_TEMPLATES
            .iter()
            .find(|t| t.code == item.code.trim())
            .map(|t| get_loc(t.name, language))
            .unwrap_or(&item.code);

        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(base_name);

        if let Some(id) = item.magic_suffix {
            if let Some(affix) = SUFFIXES.iter().find(|a| a.id == id as u32) {
                let s_loc = get_loc(affix.name, language);
                if language != "ko" {
                    name.push(' ');
                }
                name.push_str(s_loc);
            }
        }
        return name;
    }

    // Default: Base Name
    ITEM_TEMPLATES
        .iter()
        .find(|t| t.code == item.code.trim())
        .map(|t| get_loc(t.name, language))
        .unwrap_or(&item.code)
        .to_string()
}

fn get_loc(key: &str, language: &str) -> &'static str {
    get_loc_optional(key, language).unwrap_or_else(|| Box::leak(key.to_string().into_boxed_str()))
}

fn get_loc_optional(key: &str, language: &str) -> Option<&'static str> {
    LOCALIZATIONS
        .iter()
        .find(|l| l.key == key)
        .map(|l| if language == "ko" { l.ko } else { l.en })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{HuffmanTree, ItemQuality};
    use std::fs;
    use std::path::PathBuf;

    fn repo_path(relative: &str) -> PathBuf {
        let _ = dotenvy::dotenv();
        let base = std::env::var("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        base.join(relative)
    }

    fn make_prop(stat_id: u32, param: u32, value: i32) -> ItemProperty {
        ItemProperty {
            stat_id,
            name: String::new(),
            param,
            raw_value: value,
            value,
            range: ItemBitRange::default(),
        }
    }

    #[test]
    fn test_format_unique_axe() {
        let bytes = fs::read(repo_path(
            "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
        ))
        .expect("fixture should exist");
        let huffman = HuffmanTree::new();
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let items = Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");

        let buckler = &items[15];
        let formatted_en = format_item(&buckler, "en", 0, 99);
        let formatted_ko = format_item(&buckler, "ko", 0, 99);

        assert!(!formatted_en.base_attributes.is_empty());
        assert!(formatted_en.base_attributes[0].contains("Defense"));
        assert!(formatted_ko.base_attributes[0].contains("방어"));
    }

    #[test]
    fn test_descfunc_13_class_skills() {
        let prop = make_prop(83, 1, 2); // ID 1 is Sorceress
        assert_eq!(format_property(&prop, 99, "en"), "+2 to Sorceress Skill Levels");
    }

    #[test]
    fn test_descfunc_14_skill_tab() {
        let prop = make_prop(188, 0, 3);
        assert_eq!(
            format_property(&prop, 99, "en"),
            "+3 to Javelin and Spear Skills"
        );
    }

    #[test]
    fn test_descfunc_15_chance_to_cast() {
        let param = (5 << 6) | 3;
        let prop = make_prop(195, param, 25);
        assert_eq!(
            format_property(&prop, 99, "en"),
            "25% Chance to cast level 3 Left Hand Swing on attack"
        );
    }

    #[test]
    fn test_descfunc_24_charged_skill() {
        let param = (7 << 6) | 2; // Skill 7 (Fire Arrow), Level 2
        let value = (20 << 8) | 5;
        let prop = make_prop(204, param, value);
        // "Level 2 Fire Arrow (5/20 Charges)" is the English template in LOCALIZATIONS
        assert_eq!(
            format_property(&prop, 99, "en"),
            "Level 2 Fire Arrow (5/20 Charges)"
        );
    }

    #[test]
    fn test_descfunc_22_vs_monster_type() {
        let prop = make_prop(179, 4, 120);
        assert_eq!(
            format_property(&prop, 99, "en"),
            "+120% to Attack Rating versus Monster Type #4"
        );
    }

    #[test]
    fn test_format_property_damage_resist() {
        let prop = make_prop(36, 0, 15);
        assert_eq!(format_property(&prop, 99, "en"), "Physical Damage Received Reduced by 15%");
    }

    #[test]
    fn test_format_item_with_warnings() {
        let mut item = Item::empty_for_tests();
        item.code = "lsd ".to_string();  // Long Sword
        item.level = Some(1);            // ilvl 1 -> max sockets = 3
        item.sockets = Some(6);          // Violation!
        item.quality = Some(ItemQuality::Normal);
        item.is_identified = true;
        item.properties_complete = true;

        let formatted = format_item(&item, "en", 0, 99);
        assert!(!formatted.warnings.is_empty(), "Formatted item should contain warnings");
        assert!(formatted.warnings[0].contains("Socket count 6 exceeds max 3"));
    }

    #[test]
    fn test_resolve_unique_name() {
        let mut item = Item::empty_for_tests();
        item.code = "hax ".to_string(); // Hand Axe
        item.quality = Some(ItemQuality::Unique);
        item.unique_id = Some(0); // The Gnasher
        item.is_identified = true;

        assert_eq!(resolve_item_name(&item, "en"), "The Gnasher");
        assert_eq!(resolve_item_name(&item, "ko"), "더 내셔");
    }

    #[test]
    fn test_set_bonus_rendering() {
        let mut item = Item::empty_for_tests();
        item.code = "lrg ".to_string(); // Large Shield (Civerb's Ward)
        item.quality = Some(ItemQuality::Set);
        item.unique_id = Some(0); // Civerb's Ward
        item.is_identified = true;
        item.properties_complete = true;

        let formatted = format_item(&item, "en", 2, 99);
        // Civerb's Vestments has 3 pieces total? Let's check how many total pieces.
        // For now just check it has bonuses.
        assert!(!formatted.set_bonuses.is_empty());
        assert!(formatted.set_bonuses[0].active);
    }

    #[test]
    fn test_per_level_text() {
        let prop = ItemProperty {
            stat_id: 216,
            name: String::new(),
            param: 0,
            raw_value: 12,
            value: 12,
            range: ItemBitRange::default(),
        };
        // 12 * 80 / 8 = 120
        let formatted = format_property(&prop, 80, "en");
        assert_eq!(formatted, "+120 to Life (Based on Character Level)");
    }

    #[test]
    fn test_per_level_ko_text() {
        let prop = ItemProperty {
            stat_id: 216,
            name: String::new(),
            param: 0,
            raw_value: 12,
            value: 12,
            range: ItemBitRange::default(),
        };
        // 12 * 80 / 8 = 120
        let formatted = format_property(&prop, 80, "ko");
        // ModStr1u in ko is "생명력" (Wait, I should check this)
        // Let's assume it's "+120 생명력 (캐릭터 레벨에 비례해서)"
        assert_eq!(formatted, "+120 라이프 (캐릭터 레벨에 비례해서)");
    }

    #[test]
    fn test_strip_color_codes() {
        assert_eq!(strip_d2_color_codes("ÿc1Redÿc0White"), "RedWhite");
        assert_eq!(strip_d2_color_codes("ÿcUUnique"), "Unique");
        assert_eq!(strip_d2_color_codes("Normal"), "Normal");
    }
}
