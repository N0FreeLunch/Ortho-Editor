//! Stat input labels, parameter previews, and choice catalogs.

use crate::data::char_stats::CHAR_STATS;
use crate::domain::stats::stat_cost;
use crate::domain::stats::semantics::{
    decode_skill_param, decode_skill_tab_param, monster_type_name, skill_name,
    skill_tab_template as skill_tab_name,
};
use crate::localization::{get_loc, strip_d2_color_codes};

pub const STAT_ID_ELEM_SKILL: u32 = 126;

/// Returns `(value_label, param_label, param_kind_tag)` for UI input guidance.
pub fn describe_stat_input_labels(stat_id: u32) -> (&'static str, &'static str, &'static str) {
    let Some(cost) = stat_cost(stat_id) else {
        return ("Value", "Param", "none");
    };
    if cost.save_param_bits == 0 {
        return ("Value", "Param", "none");
    }
    let descfunc = cost.descfunc.unwrap_or(0);
    if stat_id == STAT_ID_ELEM_SKILL {
        return ("+Element Skills", "Element", "element");
    }
    match descfunc {
        13 => ("+Class Skills", "Class", "class"),
        14 => ("+Tab Skills", "Skill Tab", "skill_tab"),
        15 => ("Chance (%)", "Skill", "packed_skill"),
        16 => ("Aura Level", "Aura Skill", "skill"),
        22 => ("Stat Value", "Monster Type ID", "raw"),
        23 => ("Chance (%)", "Monster ID", "raw"),
        24 => ("Charges ((max<<8)|cur)", "Skill", "packed_skill"),
        27 | 28 => ("Skill Level", "Skill", "skill"),
        _ => ("Value", "Param", "raw"),
    }
}

/// Formats a human-readable preview of `param` for the given `stat_id`.
pub fn describe_stat_param_value(stat_id: u32, param: u32, language: &str) -> Option<String> {
    let cost = stat_cost(stat_id)?;
    if cost.save_param_bits == 0 {
        return None;
    }
    if stat_id == STAT_ID_ELEM_SKILL {
        return crate::data::skills::ELEMENT_NAMES
            .iter()
            .find(|(id, _, _)| *id == param)
            .map(|(_, en, ko)| {
                if language == "ko" {
                    (*ko).to_string()
                } else {
                    (*en).to_string()
                }
            });
    }
    let descfunc = cost.descfunc.unwrap_or(0);
    match descfunc {
        13 => CHAR_STATS
            .get(param as usize)
            .map(|cs| get_loc(cs.class, language).to_string()),
        14 => {
            let (class_id, tab_index) = decode_skill_tab_param(param);
            skill_tab_name(class_id, tab_index, language).map(|raw| {
                strip_d2_color_codes(&raw.replace("%+d", "").replace("%d", ""))
                    .trim()
                    .to_string()
            })
        }
        15 | 24 => {
            let (skill_id, skill_level) = decode_skill_param(param);
            let skill = skill_name(skill_id, language);
            Some(format!("Lv.{skill_level} {skill} (#{skill_id})"))
        }
        16 | 27 | 28 => Some(skill_name(param, language)),
        22 => Some(monster_type_name(param, language)),
        23 => Some(crate::domain::stats::semantics::monster_name(
            param, language,
        )),
        _ => None,
    }
}

/// Lists `(param_value, label)` choices for `(classes, elements, skill_tabs, skills)`.
pub fn list_param_choices(
    language: &str,
) -> (
    Vec<(i32, String)>,
    Vec<(i32, String)>,
    Vec<(i32, String)>,
    Vec<(i32, String)>,
) {
    let classes: Vec<(i32, String)> = CHAR_STATS
        .iter()
        .enumerate()
        .map(|(cid, cs)| {
            (
                cid as i32,
                format!("#{cid} {}", get_loc(cs.class, language)),
            )
        })
        .collect();

    let elements: Vec<(i32, String)> = crate::data::skills::ELEMENT_NAMES
        .iter()
        .map(|(id, en, ko)| {
            let lbl = if language == "ko" { *ko } else { *en };
            (*id as i32, format!("#{id} {lbl}"))
        })
        .collect();

    let mut skill_tabs = Vec::new();
    for class_id in 0..CHAR_STATS.len() {
        for tab_idx in 0..3usize {
            let param_val = ((class_id << 3) | tab_idx) as i32;
            let label = skill_tab_name(class_id, tab_idx, language)
                .map(|raw| {
                    strip_d2_color_codes(&raw.replace("%+d", "").replace("%d", ""))
                        .trim()
                        .to_string()
                })
                .unwrap_or_else(|| format!("Class #{class_id} Tab #{tab_idx}"));
            skill_tabs.push((param_val, format!("#{param_val} {label}")));
        }
    }

    let mut skills: Vec<(i32, String)> = crate::data::skills::SKILLS
        .iter()
        .map(|s| {
            (
                s.id as i32,
                format!("#{} {}", s.id, skill_name(s.id, language)),
            )
        })
        .collect();
    skills.sort_by_key(|(id, _)| *id);

    (classes, elements, skill_tabs, skills)
}
