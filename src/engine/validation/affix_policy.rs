//! Affix level, staffmod, and group warning policies.

use super::grading::{consolidate_affix_stats, validate_item_properties};
use super::model::{ValidationResult, ValidationWarning, WarningConfidenceTier};
use crate::data::item_specs::get_base_item_spec;
use crate::data::item_types::ITEM_TYPES;
use crate::data::legitimacy::{STAFFMOD_ENTRIES, calc_alvl};
use crate::domain::item::affix_catalog::is_affix_eligible_for;
use crate::domain::item::specification::{ItemSpec, lookup_spec};
use crate::domain::item::type_hierarchy::get_all_item_types;
use crate::item::{Item, ItemQuality};

pub fn check_staffmod_legitimacy_detailed(item: &Item) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    let item_types = get_all_item_types(&item.code);
    let prefixes = item.prefixes();
    let suffixes = item.suffixes();
    let unique_template = match lookup_spec(item) {
        Some(ItemSpec::Unique(template)) if template.code == item.code.trim() => Some(template),
        _ => None,
    };

    for prop in &item.properties {
        if unique_template.is_some_and(|template| {
            template.stats.iter().any(|stat| {
                stat.stat_id == prop.stat_id
                    && stat.param == prop.param
                    && prop.value >= stat.min
                    && prop.value <= stat.max
            })
        }) {
            continue;
        }
        // If this property can be explained by any existing affix rolled on this item,
        // it is an affix stat, NOT an automagic / staffmod violation!
        let explained_by_affix = prefixes.iter().chain(suffixes.iter()).any(|affix| {
            affix
                .stats
                .iter()
                .any(|s| s.stat_id == prop.stat_id && s.param == prop.param)
        });
        if explained_by_affix {
            continue;
        }

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
            let stat_name = crate::domain::stats::stat_cost(prop.stat_id)
                .map(|s| s.name)
                .unwrap_or("unknown");

            // Automagic rules on complex items are evolving heuristics
            let tier = if matches!(
                item.quality,
                Some(ItemQuality::Rare) | Some(ItemQuality::Magic) | Some(ItemQuality::Crafted)
            ) {
                WarningConfidenceTier::EvolvingHypothesis
            } else {
                WarningConfidenceTier::SettledContract
            };

            warnings.push(ValidationWarning::new(
                format!(
                    "Property '{}' value {} requires ilvl {}, but item is ilvl {} (at bits {}..{})",
                    stat_name, prop.value, req_lvl, ilvl, prop.range.start, prop.range.end
                ),
                tier,
                "staffmod_ilvl",
            ));
        }
    }

    warnings
}

pub fn check_staffmod_legitimacy(item: &Item) -> Vec<String> {
    check_staffmod_legitimacy_detailed(item)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

pub fn check_alvl_legitimacy_detailed(item: &Item) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();

    let quality = match item.quality {
        Some(q) => q,
        None => return warnings,
    };

    if !matches!(
        quality,
        ItemQuality::Magic | ItemQuality::Rare | ItemQuality::Crafted
    ) {
        return warnings;
    }

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    // Lookup qlvl
    let trimmed = item.code.trim();
    let qlvl = match get_base_item_spec(trimmed) {
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
            warnings.push(ValidationWarning::new(
                format!(
                    "Affix '{}' (level {}) exceeds item aLvl {}",
                    affix.name, affix.level, alvl
                ),
                WarningConfidenceTier::EvolvingHypothesis,
                "affix_alvl",
            ));
        }
    }

    warnings
}

pub fn check_alvl_legitimacy(item: &Item) -> Vec<String> {
    check_alvl_legitimacy_detailed(item)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

pub fn check_affix_group_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut groups = std::collections::HashSet::new();

    let prefixes = item.prefixes();
    let suffixes = item.suffixes();

    for affix in prefixes.iter().chain(suffixes.iter()) {
        if affix.group > 0 && !groups.insert(affix.group) {
            warnings.push(format!(
                "Duplicate affix group found: {} (Affix: {})",
                affix.group, affix.name
            ));
        }
    }

    warnings
}

pub(super) fn grade_item_affixes(item: &Item) -> ValidationResult {
    let prefixes = item.prefixes();
    let suffixes = item.suffixes();

    if prefixes.is_empty() && suffixes.is_empty() {
        ValidationResult {
            spec_name: format!("{:?}", item.quality.unwrap_or(ItemQuality::Normal)),
            is_perfect: true,
            score: 1.0,
            stats: Vec::new(),
            warnings: Vec::new(),
            detailed_warnings: Vec::new(),
        }
    } else {
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
        res
    }
}
