use crate::data::item_specs::{Affix, ItemStatRange, Runeword, SetItem, UniqueItem, BASE_ITEM_SPECS};
use crate::data::{affixes, runes, runewords, set_items, unique_items};
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::item_types::ITEM_TYPES;
use crate::data::legitimacy::{SOCKET_RULES, calc_alvl, STAFFMOD_ENTRIES};
use crate::item::{Item, ItemBitRange, ItemProperty, ItemQuality};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub spec_name: String,
    pub is_perfect: bool,
    pub score: f32, // Overall perfection score
    pub stats: Vec<StatValidation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StatValidation {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub current: i32,
    pub min: i32,
    pub max: i32,
    pub is_perfect: bool,
    pub score: f32,
    pub status: StatValidationStatus,
    pub range: ItemBitRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatValidationStatus {
    InRange,
    OutOfRange,
    MissingOnItem,
    UnexpectedOnItem,
}

pub enum ItemSpec {
    Unique(&'static UniqueItem),
    Set(&'static SetItem),
    Runeword(&'static Runeword),
    Affix(&'static Affix),
}

impl ItemSpec {
    pub fn name(&self) -> &str {
        match self {
            ItemSpec::Unique(ui) => ui.index,
            ItemSpec::Set(si) => si.index,
            ItemSpec::Runeword(rw) => runeword_display_name(rw),
            ItemSpec::Affix(a) => a.name,
        }
    }
}

pub fn lookup_prefix(id: u16) -> Option<&'static Affix> {
    affixes::PREFIXES.iter().find(|a| a.id == id as u32)
}

pub fn lookup_suffix(id: u16) -> Option<&'static Affix> {
    affixes::SUFFIXES.iter().find(|a| a.id == id as u32)
}

fn lookup_runeword_rune_data(rw: &'static Runeword) -> Option<&'static runes::RuneData> {
    runes::RUNES.iter().find(|entry| entry.name == rw.name)
}

fn runeword_display_name(rw: &'static Runeword) -> &'static str {
    if rw.id == 2 && rw.name.starts_with("Runeword") {
        return "Authority";
    }
    lookup_runeword_rune_data(rw)
        .map(|entry| entry.rune_name)
        .filter(|name: &&str| !name.is_empty())
        .unwrap_or(rw.name)
}

fn runeword_expected_runes(rw: &'static Runeword) -> Vec<&'static str> {
    if rw.id == 2 && rw.name.starts_with("Runeword") {
        return vec!["r15", "r13", "r08"]; // Thul Shael Ral
    }
    if !rw.runes.is_empty() {
        return rw.runes.to_vec();
    }

    lookup_runeword_rune_data(rw)
        .map(|entry| {
            [entry.rune1, entry.rune2, entry.rune3, entry.rune4, entry.rune5, entry.rune6]
                .into_iter()
                .filter(|rune: &&str| !rune.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn runeword_expected_item_types(rw: &'static Runeword) -> Vec<&'static str> {
    if !rw.item_types.is_empty() {
        return rw.item_types.to_vec();
    }

    lookup_runeword_rune_data(rw)
        .map(|entry| {
            [entry.itype1, entry.itype2, entry.itype3, entry.itype4, entry.itype5, entry.itype6]
                .into_iter()
                .filter(|item_type: &&str| !item_type.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn lookup_spec(item: &Item) -> Option<ItemSpec> {
    if item.is_runeword {
        if let Some(rw_id) = item.runeword_id {
            return runewords::RUNEWORDS.iter()
                .find(|rw| rw.id == rw_id as u32)
                .map(ItemSpec::Runeword);
        }
    }

    match item.quality {
        Some(ItemQuality::Unique) => {
            if let Some(uid) = item.unique_id {
                return unique_items::UNIQUE_ITEMS.iter()
                    .find(|ui| ui.id == uid as u32)
                    .map(ItemSpec::Unique);
            }
        }
        Some(ItemQuality::Set) => {
            if let Some(sid) = item.unique_id {
                return set_items::SET_ITEMS.iter()
                    .find(|si| si.id == sid as u32)
                    .map(ItemSpec::Set);
            }
        }
        _ => {}
    }
    None
}


pub fn validate_item(item: &Item) -> Option<ValidationResult> {
    let mut result = if let Some(spec) = lookup_spec(item) {
        match spec {
            ItemSpec::Unique(unique_spec) => Some(validate_item_properties(
                unique_spec.index,
                unique_spec.stats,
                &item.properties,
            )),
            ItemSpec::Set(set_spec) => Some(validate_item_properties(
                set_spec.index,
                set_spec.stats,
                &item.properties,
            )),
            ItemSpec::Runeword(rw_spec) => Some(validate_item_properties(
                runeword_display_name(rw_spec),
                rw_spec.stats,
                &item.runeword_attributes,
            )),
            ItemSpec::Affix(_) => None,
        }
    } else {
        match item.quality {
            Some(ItemQuality::Magic) | Some(ItemQuality::Rare) | Some(ItemQuality::Crafted) => {
                let prefixes = item.prefixes();
                let suffixes = item.suffixes();

                if prefixes.is_empty() && suffixes.is_empty() {
                    return None;
                }

                let item_types = get_all_item_types(&item.code);
                let mut warnings = Vec::new();

                for affix in prefixes.iter().chain(suffixes.iter()) {
                    if !is_affix_eligible_for(affix, &item_types) {
                        warnings.push(format!(
                            "Affix '{}' (id:{}) is not eligible for this item type",
                            affix.name, affix.id
                        ));
                    }
                }

                let consolidated_stats = consolidate_affix_stats(&prefixes, &suffixes);
                let names: Vec<&str> = prefixes
                    .iter()
                    .chain(suffixes.iter())
                    .map(|a| a.name)
                    .collect();
                let spec_name = if names.is_empty() {
                    "Unknown Affix Item".to_string()
                } else {
                    names.join(" ")
                };

                let mut res = validate_item_properties(
                    &spec_name,
                    &consolidated_stats,
                    &item.properties,
                );
                res.warnings.extend(warnings);
                Some(res)
            }
            _ => Some(ValidationResult {
                spec_name: format!("{:?}", item.quality.unwrap_or(ItemQuality::Normal)),
                is_perfect: true,
                score: 1.0,
                stats: Vec::new(),
                warnings: Vec::new(),
            }),
        }
    };

    if let Some(ref mut res) = result {
        res.warnings.extend(check_socket_legitimacy(item));
        res.warnings.extend(check_alvl_legitimacy(item));
        res.warnings.extend(check_staffmod_legitimacy(item));
        res.warnings.extend(check_affix_group_legitimacy(item));
        res.warnings.extend(check_runeword_legitimacy(item));
        res.warnings.extend(check_base_stat_legitimacy(item));
        res.warnings.extend(check_item_code_legitimacy(item));
    }
    result
}

pub fn check_item_code_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    let trimmed = item.code.trim();
    if ITEM_TEMPLATES.iter().find(|t| t.code == trimmed).is_none() {
        warnings.push(format!("Invalid item code '{}': not found in game database", trimmed));
    }
    warnings
}

pub fn check_staffmod_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    let item_types = get_all_item_types(&item.code);

    for prop in &item.properties {
        let mut possible_entries = Vec::new();

        for type_code in &item_types {
            for entry in STAFFMOD_ENTRIES {
                if entry.itype == *type_code {
                    for s in entry.stats {
                        if s.stat_id == prop.stat_id && s.param == prop.param {
                            possible_entries.push(entry);
                        }
                    }
                }
            }
        }

        let mut violations = Vec::new();
        for entry in possible_entries {
            // Check if THIS specific entry matches our property value
            for s in entry.stats {
                if s.stat_id == prop.stat_id && s.param == prop.param {
                    if prop.value >= s.min as i32 && prop.value <= s.max as i32 {
                        // This property matches this specific tier!
                        if ilvl < entry.level {
                            violations.push(entry.level);
                        }
                    }
                }
            }
        }

        if !violations.is_empty() {
            // Pick the highest level required among satisfied tiers 
            // (normally only one, but theoretically could match overlapping ranges)
            let req_lvl = violations.into_iter().max().unwrap();
            let stat_name = crate::data::stat_costs::STAT_COSTS.iter()
                .find(|s| s.id == prop.stat_id)
                .map(|s| s.name)
                .unwrap_or("unknown");

            warnings.push(format!(
                "Property '{}' value {} requires ilvl {}, but item is ilvl {} (at bits {}..{})",
                stat_name, prop.value, req_lvl, ilvl, prop.range.start, prop.range.end
            ));
        }
    }

    warnings
}

pub fn check_socket_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    let types = get_all_item_types(&item.code);
    let mut max_sockets = None;

    for type_code in types {
        if let Some(rule) = SOCKET_RULES.iter().find(|r| r.item_type == type_code) {
            let max = if ilvl >= 41 {
                rule.max_sock_high
            } else if ilvl >= 26 {
                rule.max_sock_mid
            } else {
                rule.max_sock_low
            };
            max_sockets = Some(max);
            break;
        }
    }

    if let Some(max) = max_sockets {
        if let Some(actual) = item.sockets {
            if actual > max {
                warnings.push(format!("Socket count {} exceeds max {} for ilvl {}", actual, max, ilvl));
            }
        }
    }

    warnings
}

pub fn check_alvl_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();

    let quality = match item.quality {
        Some(q) => q,
        None => return warnings,
    };

    if !matches!(quality, ItemQuality::Magic | ItemQuality::Rare | ItemQuality::Crafted) {
        return warnings;
    }

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    // Lookup qlvl
    let trimmed = item.code.trim();
    let qlvl = match BASE_ITEM_SPECS.iter().find(|s| s.code == trimmed) {
        Some(spec) => spec.qlvl as u8,
        None => 0,
    };

    // Lookup magic_lvl from item types
    let types = get_all_item_types(&item.code);
    let mut magic_lvl = 0;
    for type_code in types {
        if let Some(it) = ITEM_TYPES.iter().find(|it| it.code == type_code) {
            if it.magic_lvl > magic_lvl {
                magic_lvl = it.magic_lvl;
            }
        }
    }

    let alvl = calc_alvl(ilvl, qlvl, magic_lvl);

    for affix in item.prefixes().iter().chain(item.suffixes().iter()) {
        if affix.level > alvl as u32 {
            warnings.push(format!(
                "Affix '{}' (level {}) exceeds item aLvl {}",
                affix.name, affix.level, alvl
            ));
        }
    }

    warnings
}

pub fn check_affix_group_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut groups = std::collections::HashSet::new();
    
    let prefixes = item.prefixes();
    let suffixes = item.suffixes();

    for affix in prefixes.iter().chain(suffixes.iter()) {
        if affix.group > 0 && !groups.insert(affix.group) {
            warnings.push(format!("Duplicate affix group found: {} (Affix: {})", affix.group, affix.name));
        }
    }
    
    warnings
}

pub fn check_ethereal_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    if !item.is_ethereal {
        return warnings;
    }

    let trimmed = item.code.trim();
    let spec = if let Some(s) = BASE_ITEM_SPECS.iter().find(|s| s.code == trimmed) {
        s
    } else {
        return warnings;
    };

    // 1. Illegal Ethereal State Check
    let item_types = get_all_item_types(&item.code);
    if item_types.contains(&"bow") || item_types.contains(&"xbow") {
        warnings.push("Bows and crossbows cannot be ethereal".to_string());
    }

    // 2. Durability Verification
    if spec.durability > 0 {
        let eth_base_durability = (spec.durability as u32 / 2) + 1;
        let mut edur_percent = 0;
        for prop in &item.properties {
            if prop.stat_id == 75 {
                edur_percent += prop.value;
            }
        }
        let expected_max_dur = (eth_base_durability * (100 + edur_percent as u32)) / 100;
        if let Some(max_dur) = item.max_durability {
            if max_dur > 0 {
                if (max_dur as u32) < expected_max_dur {
                    warnings.push(format!(
                        "Ethereal item has lower than expected durability: {} (expected ~{})",
                        max_dur, expected_max_dur
                    ));
                } else if (max_dur as u32) > expected_max_dur + 1 {
                    warnings.push(format!(
                        "Ethereal item has higher than expected durability: {} (expected ~{})",
                        max_dur, expected_max_dur
                    ));
                }
            }
        }
    }
    warnings
}

pub fn check_base_stat_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    let trimmed = item.code.trim();
    let spec = if let Some(s) = BASE_ITEM_SPECS.iter().find(|s| s.code == trimmed) {
        s
    } else {
        return warnings;
    };

    // Defense Verification
    if let Some(actual_def) = item.defense {
        let mut ed_percent = 0;
        let mut flat_def = 0;
        for prop in &item.properties {
            if prop.stat_id == 16 { ed_percent += prop.value; }
            else if prop.stat_id == 31 { flat_def += prop.value; }
        }

        let is_max_ac_plus_one = ed_percent > 0 || item.is_runeword || matches!(item.quality, Some(ItemQuality::Unique) | Some(ItemQuality::Set));
        
        let eth_mul = if item.is_ethereal { 1.5 } else { 1.0 };

        if is_max_ac_plus_one {
            let base_ac = spec.max_ac as f32 + 1.0;
            let expected = ((base_ac * eth_mul).floor() * (100.0 + ed_percent as f32) / 100.0).floor() as u32 + flat_def as u32;
            if actual_def != expected && (actual_def as i32 - expected as i32).abs() > 1 {
                warnings.push(format!("Defense {} does not match expected {} (Superior/Unique Rule)", actual_def, expected));
            }
        } else {
            let min_expected = ((spec.min_ac as f32 * eth_mul).floor() + flat_def as f32).floor() as u32;
            let max_expected = ((spec.max_ac as f32 * eth_mul).floor() + flat_def as f32).floor() as u32;
            if actual_def < min_expected || actual_def > max_expected {
                warnings.push(format!("Defense {} out of range {}-{}", actual_def, min_expected, max_expected));
            }
        }
    }
    
    warnings
}
pub fn check_runeword_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();

    if !item.is_runeword {
        return warnings;
    }

    let rw = match lookup_spec(item) {
        Some(ItemSpec::Runeword(rw)) => rw,
        _ => return warnings,
    };

    let rw_name = runeword_display_name(rw);
    let expected_item_types = runeword_expected_item_types(rw);

    // 1. Type Eligibility
    let item_types = get_all_item_types(&item.code);
    let is_eligible = expected_item_types
        .iter()
        .any(|&item_type| item_types.contains(&item_type));
    if !is_eligible {
        warnings.push(format!(
            "Runeword '{}' is not eligible for item type '{}'",
            rw_name,
            item.code.trim()
        ));
    }

    let expected_runes = runeword_expected_runes(rw);

    // 2. Socket Count
    if let Some(sockets) = item.sockets {
        if !expected_runes.is_empty() && sockets as usize != expected_runes.len() {
            warnings.push(format!(
                "Runeword '{}' requires {} sockets, but item has {}",
                rw_name,
                expected_runes.len(),
                sockets
            ));
        }
    }

    // 3. Rune Sequence
    if !expected_runes.is_empty() && item.socketed_items.len() == expected_runes.len() {
        for (i, socketed) in item.socketed_items.iter().enumerate() {
            if socketed.code.trim() != expected_runes[i] {
                warnings.push(format!("Runeword '{}' has incorrect rune sequence", rw_name));
                break;
            }
        }
    } else if !expected_runes.is_empty() && !item.socketed_items.is_empty() {
        warnings.push(format!(
            "Runeword '{}' has incorrect number of socketed items",
            rw_name
        ));
    }

    warnings
}

fn consolidate_affix_stats(prefixes: &[&Affix], suffixes: &[&Affix]) -> Vec<ItemStatRange> {
    let mut merged: std::collections::HashMap<(u32, u32), ItemStatRange> =
        std::collections::HashMap::new();

    let all_affixes = prefixes.iter().chain(suffixes.iter());
    for affix in all_affixes {
        for spec_stat in affix.stats {
            let key = (spec_stat.stat_id, spec_stat.param);
            let entry = merged.entry(key).or_insert(ItemStatRange {
                stat_id: spec_stat.stat_id,
                param: spec_stat.param,
                min: 0,
                max: 0,
            });
            entry.min += spec_stat.min;
            entry.max += spec_stat.max;
        }
    }

    merged.into_values().collect()
}

pub fn is_affix_eligible_for(affix: &Affix, item_types: &[&str]) -> bool {
    // If include_types is empty, it's generally allowed on all types unless excluded
    if !affix.include_types.is_empty() {
        let has_included = affix.include_types.iter().any(|&inc| item_types.contains(&inc));
        if !has_included {
            return false;
        }
    }

    // If any excluded type is in item_types, it's not eligible
    let has_excluded = affix.exclude_types.iter().any(|&exc| item_types.contains(&exc));
    if has_excluded {
        return false;
    }

    true
}

fn validate_item_properties(
    spec_name: &str,
    spec_stats: &[ItemStatRange],
    item_properties: &[ItemProperty],
) -> ValidationResult {
    let mut stats: Vec<StatValidation> = Vec::new();
    let mut used_specs = vec![false; spec_stats.len()];

    for prop in item_properties {
        if let Some(spec_idx) = find_matching_spec_index(prop, spec_stats, &used_specs) {
            used_specs[spec_idx] = true;
            let spec_stat = &spec_stats[spec_idx];
            let (score, in_range, is_perfect) = calculate_score(prop.value, spec_stat.min, spec_stat.max);

            stats.push(StatValidation {
                stat_id: prop.stat_id,
                name: prop.name.clone(),
                param: prop.param,
                current: prop.value,
                min: spec_stat.min,
                max: spec_stat.max,
                is_perfect,
                score,
                status: if in_range {
                    StatValidationStatus::InRange
                } else {
                    StatValidationStatus::OutOfRange
                },
                range: prop.range,
            });
        } else {
            stats.push(StatValidation {
                stat_id: prop.stat_id,
                name: prop.name.clone(),
                param: prop.param,
                current: prop.value,
                min: 0,
                max: 0,
                is_perfect: false,
                score: 0.0,
                status: StatValidationStatus::UnexpectedOnItem,
                range: prop.range,
            });
        }
    }

    for (idx, spec_stat) in spec_stats.iter().enumerate() {
        if used_specs[idx] {
            continue;
        }

        stats.push(StatValidation {
            stat_id: spec_stat.stat_id,
            name: format!("stat_{}", spec_stat.stat_id),
            param: spec_stat.param,
            current: 0,
            min: spec_stat.min,
            max: spec_stat.max,
            is_perfect: false,
            score: 0.0,
            status: StatValidationStatus::MissingOnItem,
            range: ItemBitRange::default(),
        });
    }

    let variable_scores: Vec<f32> = stats
        .iter()
        .filter(|entry| {
            (entry.status == StatValidationStatus::InRange
                || entry.status == StatValidationStatus::OutOfRange)
                && entry.min != entry.max
        })
        .map(|entry| entry.score)
        .collect();

    let score = if !variable_scores.is_empty() {
        variable_scores.iter().sum::<f32>() / variable_scores.len() as f32
    } else {
        let matched_scores: Vec<f32> = stats
            .iter()
            .filter(|entry| {
                entry.status == StatValidationStatus::InRange
                    || entry.status == StatValidationStatus::OutOfRange
            })
            .map(|entry| entry.score)
            .collect();
        if matched_scores.is_empty() {
            0.0
        } else {
            matched_scores.iter().sum::<f32>() / matched_scores.len() as f32
        }
    };

    let has_only_in_range = stats
        .iter()
        .all(|entry| entry.status == StatValidationStatus::InRange);
    let variable_perfect = stats
        .iter()
        .filter(|entry| entry.min != entry.max)
        .all(|entry| entry.is_perfect);
    let is_perfect = !stats.is_empty() && has_only_in_range && variable_perfect;

    ValidationResult {
        spec_name: spec_name.to_string(),
        is_perfect,
        score,
        stats,
        warnings: Vec::new(),
    }
}

fn find_matching_spec_index(
    prop: &ItemProperty,
    spec_stats: &[ItemStatRange],
    used_specs: &[bool],
) -> Option<usize> {
    spec_stats
        .iter()
        .enumerate()
        .find_map(|(idx, spec_stat)| {
            if used_specs[idx] {
                return None;
            }

            if spec_stat.stat_id == prop.stat_id && spec_stat.param == prop.param {
                Some(idx)
            } else {
                None
            }
        })
}

fn calculate_score(current: i32, min: i32, max: i32) -> (f32, bool, bool) {
    if min == max {
        let in_range = current == min;
        let score = if in_range { 1.0 } else { 0.0 };
        return (score, in_range, in_range);
    }

    if min < max {
        if current < min || current > max {
            return (0.0, false, false);
        }

        let range = (max - min) as f32;
        let ratio = ((current - min) as f32 / range).clamp(0.0, 1.0);
        return (ratio, true, current == max);
    }

    if current > min || current < max {
        return (0.0, false, false);
    }

    let range = (min - max) as f32;
    let ratio = ((min - current) as f32 / range).clamp(0.0, 1.0);
    (ratio, true, current == max)
}

pub fn get_all_item_types(item_code: &str) -> Vec<&'static str> {
    let trimmed = item_code.trim();
    let template = match ITEM_TEMPLATES.iter().find(|t| t.code == trimmed) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut seeds = Vec::new();
    if let Some(t1) = template.item_type {
        seeds.push(t1);
    }
    if let Some(t2) = template.item_type2 {
        seeds.push(t2);
    }

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    let mut current_level = seeds;
    let mut depth = 0;

    while !current_level.is_empty() && depth < 10 {
        let mut next_level = Vec::new();
        for code in current_level {
            if visited.insert(code) {
                result.push(code);
                if let Some(it) = ITEM_TYPES.iter().find(|it| it.code == code) {
                    if let Some(e1) = it.equiv1 {
                        next_level.push(e1);
                    }
                    if let Some(e2) = it.equiv2 {
                        next_level.push(e2);
                    }
                }
            }
        }
        current_level = next_level;
        depth += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_item_types() {
        let axe_types = get_all_item_types("hax "); // Hand Axe
        assert!(axe_types.contains(&"axe"));
        assert!(axe_types.contains(&"mele"));
        assert!(axe_types.contains(&"weap"));

        let shld_types = get_all_item_types("buc "); // Buckler
        assert!(shld_types.contains(&"shld"));
        assert!(shld_types.contains(&"armo"));
        assert!(shld_types.contains(&"seco"));
    }

    #[test]
    fn test_runeword_sequence_violation() {
        let mut item = Item::empty_for_tests();
        item.is_runeword = true;
        item.runeword_id = Some(32); // Enigma (Jah Ith Ber = r31 r06 r30)
        item.code = "uap ".to_string(); // Archon Plate
        item.sockets = Some(3);
        
        let mut r31 = Item::empty_for_tests(); r31.code = "r31 ".to_string();
        let mut r06 = Item::empty_for_tests(); r06.code = "r06 ".to_string();
        let mut r30 = Item::empty_for_tests(); r30.code = "r30 ".to_string();
        
        // Correct order is r31, r06, r30. Let's use an incorrect one.
        item.socketed_items = vec![r31, r30, r06];
        
        let warnings = check_runeword_legitimacy(&item);
        assert!(warnings.iter().any(|w| w.contains("incorrect rune sequence")));
    }

    #[test]
    fn test_superior_ethereal_defense_validation() {
        let mut item = Item::empty_for_tests();
        item.code = "utp ".to_string(); // Archon Plate
        item.is_ethereal = true;
        item.quality = Some(crate::item::ItemQuality::High);
        
        // 15% Enhanced Defense
        item.properties.push(crate::item::ItemProperty::new(
            16,
            "item_armor_percent".to_string(),
            0,
            0,
            15,
            ItemBitRange::default(),
        ));
        
        // Expected: floor((524+1) * 1.5) * 1.15 = floor(787.5) * 1.15 = 787 * 1.15 = 905.05 -> 905?
        // Actually: floor(floor(525 * 1.5) * 1.15) = floor(787 * 1.15) = floor(905.05) = 905.
        // Wait, my check_base_stat_legitimacy uses floats.
        // Let's set it to 905.
        item.defense = Some(905);
        
        let warnings = check_base_stat_legitimacy(&item);
        assert!(warnings.is_empty(), "Expected no warnings for 905 defense, got: {:?}", warnings);
        
        // Violation case: 900 defense
        item.defense = Some(900);
        let warnings = check_base_stat_legitimacy(&item);
        assert!(warnings.iter().any(|w| w.contains("Defense 900 does not match expected 905")));
    }
}
