//! Render individual and grouped properties independently of item assembly.

use crate::data::char_stats::CHAR_STATS;
use crate::data::stat_costs::STAT_COSTS;
use crate::domain::stats::semantics::{
    decode_skill_param, decode_skill_tab_param, monster_type_name, skill_name,
    skill_tab_template as skill_tab_name,
};
use crate::item::{ItemBitRange, ItemProperty};
use crate::localization::{format_template, get_loc, get_loc_optional, strip_d2_color_codes};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatDisplayMode {
    #[default]
    Fused,
    Discrete,
}

pub fn format_properties_list(
    props: &[ItemProperty],
    char_level: u8,
    language: &str,
) -> Vec<String> {
    format_properties_list_with_mode(props, char_level, language, StatDisplayMode::Fused)
}

pub fn format_properties_list_with_mode(
    props: &[ItemProperty],
    char_level: u8,
    language: &str,
    mode: StatDisplayMode,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_indices = std::collections::HashSet::new();

    // If Fused mode is active, check and apply composite stat rules (e.g. res-all, all-stats, res-all-max)
    if mode == StatDisplayMode::Fused {
        apply_composite_rules(props, language, &mut out, &mut skip_indices);
    }

    let mut i = 0;
    while i < props.len() {
        if skip_indices.contains(&i) {
            i += 1;
            continue;
        }

        let p = &props[i];
        // Only the exact internal socket placeholder has no tooltip meaning.
        let socket_placeholder = p.stat_id == 317
            && p.name == "item_socket_child"
            && p.param == 0
            && p.raw_value == 0
            && p.value == 0
            && p.range == ItemBitRange::default()
            && !p.is_opaque
            && p.opaque_bits.is_none();
        if p.stat_id == 511 || socket_placeholder {
            // Omit the terminator and internal socket placeholder from tooltips.
            i += 1;
            continue;
        }

        // Matching damage channels are one displayed modifier; retain every stored field.
        if mode == StatDisplayMode::Fused
            && matches!(p.stat_id, 21 | 22)
            && let Some(indices) = damage_companion_indices(props, i)
        {
            skip_indices.extend(indices);
            out.push(format_property(p, char_level, language));
            i += 1;
            continue;
        }

        // 1. Enhanced Damage pairing (Stat 17 & 18)
        if mode == StatDisplayMode::Fused && (p.stat_id == 17 || p.stat_id == 18) {
            let other_stat_id = if p.stat_id == 17 { 18 } else { 17 };
            if let Some((other_idx, other_p)) = props.iter().enumerate().find(|(idx, o)| {
                *idx > i && o.stat_id == other_stat_id && !skip_indices.contains(idx)
            }) {
                if p.value == other_p.value {
                    skip_indices.insert(other_idx);
                    let val = p.value;
                    let line = if language == "ko" {
                        format!("+{}% 증가된 데미지", val)
                    } else {
                        format!("+{}% Enhanced Damage", val)
                    };
                    out.push(line);
                    i += 1;
                    continue;
                }
            }
        }

        // Fire, lightning and magic share the same forward-pair consumption rule.
        if mode == StatDisplayMode::Fused
            && let Some((max_idx, line)) = format_elemental_pair(props, i, language, &skip_indices)
        {
            skip_indices.insert(max_idx);
            out.push(line);
            i += 1;
            continue;
        }

        // 5. Cold Damage pairing (Stat 54 & 55 & 56)
        if mode == StatDisplayMode::Fused && p.stat_id == 54 {
            if let Some((max_idx, max_p)) = props
                .iter()
                .enumerate()
                .find(|(idx, o)| *idx > i && o.stat_id == 55 && !skip_indices.contains(idx))
            {
                skip_indices.insert(max_idx);
                let len_p = props
                    .iter()
                    .enumerate()
                    .find(|(idx, o)| *idx > i && o.stat_id == 56 && !skip_indices.contains(idx));
                let duration_sec = if let Some((len_idx, lp)) = len_p {
                    skip_indices.insert(len_idx);
                    lp.value / 25
                } else {
                    0
                };
                let min = p.value;
                let max = max_p.value;
                let base_range = format_elemental_range(min, max, "cold", "콜드", language);
                let line = if duration_sec > 0 {
                    if language == "ko" {
                        format!("{} ({}초 동안 지속)", base_range, duration_sec)
                    } else {
                        format!("{} ({} sec duration)", base_range, duration_sec)
                    }
                } else {
                    base_range
                };
                out.push(line);
                i += 1;
                continue;
            }
        }

        // Combine only a complete qualified poison rate/duration group when in Fused mode.
        if mode == StatDisplayMode::Fused
            && p.stat_id == 57
            && let Some(line) = format_poison_triplet(&props[i..], language)
        {
            out.push(line);
            i += 3;
            continue;
        }

        // 6. Skip companion max stats if already consumed or orphan coldlength in Fused mode
        if mode == StatDisplayMode::Fused
            && (p.stat_id == 49
                || p.stat_id == 51
                || p.stat_id == 53
                || p.stat_id == 55
                || p.stat_id == 56)
        {
            i += 1;
            continue;
        }

        // Standard single property formatting
        out.push(format_property(p, char_level, language));
        i += 1;
    }

    out
}

fn format_elemental_pair(
    props: &[ItemProperty],
    primary: usize,
    language: &str,
    skip_indices: &std::collections::HashSet<usize>,
) -> Option<(usize, String)> {
    use crate::engine::property_groups::{ElementalKind, matching_elemental_pair};

    let pair = matching_elemental_pair(props, primary, skip_indices)?;
    let (english, korean) = match pair.kind {
        ElementalKind::Fire => ("fire", "파이어"),
        ElementalKind::Lightning => ("lightning", "라이트닝"),
        ElementalKind::Magic => ("magic", "마법"),
    };
    let min = pair.min;
    let max = pair.max;
    let max_idx = pair.max_index;
    let line = format_elemental_range(min, max, english, korean, language);
    Some((max_idx, line))
}

fn format_elemental_range(
    min: i32,
    max: i32,
    english: &str,
    korean: &str,
    language: &str,
) -> String {
    if min == max {
        if language == "ko" {
            format!("+{min} {korean} 데미지 추가")
        } else {
            format!("Adds {min} {english} damage")
        }
    } else if language == "ko" {
        format!("{min} - {max} {korean} 데미지 추가")
    } else {
        format!("Adds {min}-{max} {english} damage")
    }
}

fn apply_composite_rules(
    props: &[ItemProperty],
    language: &str,
    out: &mut Vec<String>,
    skip_indices: &mut std::collections::HashSet<usize>,
) {
    let rules = crate::data::properties::get_composite_rules();
    for rule in rules {
        if let Some(matched) = crate::engine::property_groups::matching_composite_rule(
            props,
            rule.stat_ids,
            skip_indices,
        ) {
            let val = matched.value;
            let matched_indices = matched.indices;
            for &idx in &matched_indices {
                skip_indices.insert(idx);
            }

            let template = get_loc(rule.loc_key, language);
            let formatted = if template.contains('%') {
                format_template(template, &[val.to_string()])
            } else if rule.code == "all-stats" {
                if language == "ko" {
                    format!("+{} 모든 능력치", val)
                } else {
                    format!("+{} to all Attributes", val)
                }
            } else if rule.code == "res-all-max" {
                if language == "ko" {
                    format!("모든 최대 저항력 +{}%", val)
                } else {
                    format!("+{}% to All Maximum Resistances", val)
                }
            } else {
                format!("{} +{}", template, val)
            };

            out.push(formatted);
        }
    }
}

use crate::engine::property_groups::damage_companion_indices;

fn format_poison_triplet(props: &[ItemProperty], language: &str) -> Option<String> {
    let display = crate::engine::poison::displayed_poison(props)?;
    let min = display.min_damage;
    let max = display.max_damage;
    let seconds = display.duration_seconds.to_string();
    let (key, args) = if min == max {
        ("strModPoisonDamage", vec![min.to_string(), seconds])
    } else {
        (
            "strModPoisonDamageRange",
            vec![min.to_string(), max.to_string(), seconds],
        )
    };
    Some(format_template(get_loc(key, language), &args))
}

pub fn format_property(prop: &ItemProperty, char_level: u8, language: &str) -> String {
    if prop.is_opaque {
        return if language == "ko" {
            format!("[미결 옵션] 미분류 스탯 (ID: {})", prop.stat_id)
        } else {
            format!("[Unresolved Option] Unmapped Stat (ID: {})", prop.stat_id)
        };
    }

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

    // Slice 1: DescStr2 handling (e.g. Based on Character Level with per-level rate)
    let mut phrase2 = if let Some(d2) = cost.descstr2 {
        Some(get_loc(d2, language).to_string())
    } else {
        None
    };

    // Slice 2: Level Scaling logic utilizing op and op_param
    let display =
        crate::engine::level_scaling::displayed_level_scaling(prop.value, char_level, cost);
    let display_value = display.value;
    // Append precise decimal rate per level, e.g. (+1.5 per Character Level) or (+1.5 `캐릭터 레벨에 비례해서`)
    if let Some(factor) = display.per_level_factor {
        let factor_str = if factor.fract() == 0.0 {
            format!("{:+}", factor as i64)
        } else {
            // Format up to 3 decimal places without trailing zeros
            let s = format!("{:+.*}", 3, factor);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        };

        if let Some(ref orig_phrase) = phrase2 {
            if language == "ko" {
                phrase2 = Some(format!(
                    "({} {})",
                    factor_str,
                    orig_phrase.trim_matches(|c| c == '(' || c == ')')
                ));
            } else {
                phrase2 = Some(format!(
                    "({} {})",
                    factor_str,
                    orig_phrase.trim_matches(|c| c == '(' || c == ')')
                ));
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
        } else if descfunc == 19 && (key_str == "ModStre9s" || key_str == "ModStre9v") {
            // descfunc 19 with ModStre9s ("Indestructible" / `파괴안됨`) or ModStre9v ("Replenishes quantity" / `자동수량회복`)
            // outputs the fixed phrase without numeric value prefix.
            format_with_phrase2(loc_str.to_string())
        } else {
            format_with_phrase2(base_fallback)
        }
    };

    match descfunc {
        1 | 19 => {
            if stat_id == 158 && display_value > 0 {
                if language == "ko" {
                    format!("{} [레벨 {}]", loc_str, display_value)
                } else {
                    format!("{} [Level {}]", loc_str, display_value)
                }
            } else if stat_id == 126 {
                // item_elemskill (prop.param: 0=Fire, 1=Lightning, 2=Magic, 3=Cold, 4=Poison)
                let elem_name = crate::data::skills::ELEMENT_NAMES
                    .iter()
                    .find(|(id, _, _)| *id == prop.param)
                    .map(|(_, en, ko)| if language == "ko" { *ko } else { *en })
                    .unwrap_or(if language == "ko" {
                        "원소"
                    } else {
                        "Elemental"
                    });
                if loc_str.contains('%') {
                    format_template(loc_str, &[signed_value, elem_name.to_string()])
                } else if language == "ko" {
                    format!("{} {} 스킬 레벨 상승", signed_value, elem_name)
                } else {
                    format!("{} to {} Skills", signed_value, elem_name)
                }
            } else {
                f(format!("{} {}", signed_value, loc_str))
            }
        }
        2 => f(format!("{}% {}", display_value, loc_str)),
        3 => f(format!("{} {}", display_value, loc_str)),
        4 | 8 => f(format!("{}% {}", signed_value, loc_str)),
        5 => f(format!("{} {}%", loc_str, display_value)),
        6 => f(format!("{}% {}", signed_value, loc_str)),
        7 => f(format!("{}% {}", display_value, loc_str)),
        11 => {
            // descfunc 11 is used for durability replenishment (e.g. rep-dur, stat 252).
            // In D2 engine, value represents the rate: seconds = 100 / value.
            // If value < 100, format using ModStre9u ("Repairs %d durability in %d seconds" / `내구력 회복`)
            // with durability amount = 1 and calculated seconds.
            // If value >= 100, format using ModStre9t ("Repairs %d durability per second" / `내구력 회복`)
            // with durability rate per second = value / 100.
            if display_value > 0 && display_value < 100 {
                let seconds = 100 / display_value;
                let template = get_loc("ModStre9u", language);
                format_template(template, &["1".to_string(), seconds.to_string()])
            } else if display_value >= 100 {
                let rate = display_value / 100;
                let template = get_loc("ModStre9t", language);
                format_template(template, &[rate.to_string()])
            } else {
                f(format!("{} {}", display_value, loc_str))
            }
        }
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
            if let Some(tab_name) = skill_tab_name(class_id, tab_index, language) {
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

                let template = get_loc_optional("ItemExpansiveChancX", language)
                    .unwrap_or("%d%% Chance to cast level %d %s on attack");
                format_template(
                    template,
                    &[
                        display_value.to_string(),
                        skill_level.to_string(),
                        skill,
                        phrase2.to_string(),
                    ],
                )
            }
        }
        16 => {
            let skill = skill_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[display_value.to_string(), skill.to_string()])
            } else {
                let template = get_loc_optional("ModitemAura", language)
                    .unwrap_or("Level %d %s Aura When Equipped");
                format_template(template, &[display_value.to_string(), skill])
            }
        }
        17 | 18 => {
            let label = if descfunc == 17 {
                loc_str
            } else {
                &format!("{}%", display_value)
            };
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
            let monster = crate::domain::stats::semantics::monster_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[display_value.to_string(), monster])
            } else if language == "ko" {
                format!("{}% {} ({})", display_value, loc_str, monster)
            } else {
                format!("{}% {} as {}", display_value, loc_str, monster)
            }
        }
        24 => {
            let (skill_id, skill_level) = decode_skill_param(prop.param);
            let skill = skill_name(skill_id, language);
            let (current_charges, max_charges) =
                crate::domain::stats::semantics::decode_skill_charges_value(prop.value);
            format_skill_charges(skill_level, &skill, current_charges, max_charges, language)
        }
        27 => {
            let skill = skill_name(prop.param, language);
            if loc_str.contains('%') {
                format_template(loc_str, &[signed_value, skill])
            } else {
                let class_id = CHAR_STATS
                    .iter()
                    .position(|s| cost.name.starts_with(&s.class.to_lowercase()));

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
                let key = if prop.value >= 0 {
                    "ModStr2uPercent"
                } else {
                    "ModStr2uPercentNegative"
                };
                let label = get_loc(key, language);
                format_template(label, &[prop.value.abs().to_string()])
            } else if loc_str.contains('%') {
                format_template(loc_str, &[display_value.abs().to_string()])
            } else {
                format!("{} {}", signed_value, loc_str)
            }
        }
        _ => strip_d2_color_codes(&format!(
            "{} (func {}): {}",
            loc_str, descfunc, display_value
        )),
    }
}

fn format_skill_charges(level: u32, skill: &str, cur: u32, max: u32, language: &str) -> String {
    let template =
        get_loc_optional("ModStre10d", language).unwrap_or("Level %d %s (%d/%d Charges)");
    if template.contains("%s") {
        format_template(
            template,
            &[
                level.to_string(),
                skill.to_string(),
                cur.to_string(),
                max.to_string(),
            ],
        )
    } else {
        // Korean pattern like "(%d/%d `회`)" -> needs skill/level prefix
        let level_label = get_loc("ModStre10b", language);
        let charges_formatted = format_template(template, &[cur.to_string(), max.to_string()]);
        format!("{} {} {} {}", level_label, level, skill, charges_formatted)
    }
}
