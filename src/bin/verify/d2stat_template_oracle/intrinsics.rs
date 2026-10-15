//! Source compatibility for explicit-v105 primary intrinsic properties.
//! Optional staffmods are observations, never required-roll coverage.
use d2r_core::data::generated::intrinsics::{AUTO_MAGIC_SOURCES, ITEM_INTRINSIC_SOURCES};
use d2r_core::data::item_specs::ItemStatRange;
use d2r_core::data::skills::SKILLS;
use d2r_core::data::stat_costs::STAT_COSTS;
use d2r_core::engine::validation::get_all_item_types;
use d2r_core::item::{Item, ItemQuality};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub kind: &'static str,
    pub message: String,
    pub hint: &'static str,
}
#[derive(Debug, Serialize)]
pub struct ObservedProperty {
    pub property_index: usize,
    pub param: u32,
    pub value: i32,
}
#[derive(Debug, Serialize)]
pub struct PropertyCheck {
    pub source_kind: &'static str,
    pub stat_id: u32,
    pub param: u32,
    pub minimum: i32,
    pub maximum: i32,
    pub observed: Vec<ObservedProperty>,
    pub admitted: bool,
}
#[derive(Debug, Serialize)]
pub struct IntrinsicEvaluation {
    pub code: String,
    pub source_range_start: u64,
    pub source_range_end: u64,
    pub status: &'static str,
    pub auto_saved_id: Option<u16>,
    pub auto_source_row_id: Option<u32>,
    pub auto_source_name: Option<&'static str>,
    pub host_auto_group: Option<u32>,
    pub host_staffmods_class: Option<&'static str>,
    pub automatic_checks: Vec<PropertyCheck>,
    pub staffmod_checks: Vec<PropertyCheck>,
    pub unadmitted_contexts: Vec<&'static str>,
    pub diagnostics: Vec<Diagnostic>,
}
pub struct Outcome {
    pub evaluation: IntrinsicEvaluation,
    pub admitted_property_indices: Vec<usize>,
}

fn diagnostic(row: &mut IntrinsicEvaluation, kind: &'static str, message: impl Into<String>) {
    row.diagnostics.push(Diagnostic { kind, message: message.into(), hint: "Check source ID/group/host class and the original parameter/value/occurrence. Do not hydrate hidden or optional rolls." });
}
fn overlaps(item: &Item, known: &[ItemStatRange], id: u32, param: u32) -> bool {
    known.iter().any(|s| s.stat_id == id)
        || item
            .prefixes()
            .into_iter()
            .chain(item.suffixes())
            .any(|a| a.stats.iter().any(|s| s.stat_id == id && s.param == param))
}

pub fn evaluate(
    item: &Item,
    alpha_mode: bool,
    known_primary_stats: &[ItemStatRange],
) -> Option<Outcome> {
    if alpha_mode
        || item.header.save_is_alpha
        || item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .is_none_or(|a| a.save_version != 105)
    {
        return None;
    }
    let staff_id = STAT_COSTS.iter().find(|s| s.name == "item_singleskill")?.id;
    let staff: Vec<_> = item
        .properties
        .iter()
        .enumerate()
        .filter(|(_, p)| p.stat_id == staff_id)
        .collect();
    if !item.has_class_specific_data && item.class_specific_bits.is_none() && staff.is_empty() {
        return None;
    }
    let host = ITEM_INTRINSIC_SOURCES
        .iter()
        .find(|h| h.code == item.code.trim());
    let mut row = IntrinsicEvaluation {
        code: item.code.trim().into(),
        source_range_start: item.range.start,
        source_range_end: item.range.end,
        status: "qualified",
        auto_saved_id: item.class_specific_bits,
        auto_source_row_id: None,
        auto_source_name: None,
        host_auto_group: host.and_then(|h| h.auto_group),
        host_staffmods_class: host.and_then(|h| h.staffmods_class),
        automatic_checks: Vec::new(),
        staffmod_checks: Vec::new(),
        unadmitted_contexts: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut admitted = Vec::new();
    if item.is_opaque() || item.is_semi_opaque() || !item.properties_complete {
        row.status = "unadmitted";
        row.unadmitted_contexts.push("incomplete_item");
        return Some(Outcome {
            evaluation: row,
            admitted_property_indices: admitted,
        });
    }
    if item.has_class_specific_data != item.class_specific_bits.is_some() {
        diagnostic(
            &mut row,
            "automatic_header_presence",
            "Automatic header flag and saved ID disagree",
        );
    }
    let source = if item.has_class_specific_data {
        match item
            .class_specific_bits
            .and_then(|id| AUTO_MAGIC_SOURCES.iter().find(|s| s.saved_id == id))
        {
            Some(source) => {
                row.auto_source_row_id = Some(source.source_row_id);
                row.auto_source_name = Some(source.affix.name);
                let all_types = get_all_item_types(&item.code);
                let included = source
                    .affix
                    .include_types
                    .iter()
                    .any(|t| all_types.iter().any(|actual| actual == t));
                let excluded = source
                    .affix
                    .exclude_types
                    .iter()
                    .any(|t| all_types.iter().any(|actual| actual == t));
                if row.host_auto_group != Some(source.affix.group) || !included || excluded {
                    diagnostic(
                        &mut row,
                        "automatic_host_context",
                        "Automatic source group/type does not match the source host",
                    );
                    None
                } else {
                    Some(source)
                }
            }
            None => {
                diagnostic(
                    &mut row,
                    "unknown_automatic_source",
                    "Saved automatic ID has no generated source row",
                );
                None
            }
        }
    } else {
        None
    };
    if !item.header.is_identified {
        if item.properties.iter().any(|p| p.stat_id != 511) {
            diagnostic(
                &mut row,
                "unidentified_property_leak",
                "Unidentified intrinsic host exposes visible properties",
            );
        }
        row.status = if row.diagnostics.is_empty() {
            "unidentified"
        } else {
            "red"
        };
        return Some(Outcome {
            evaluation: row,
            admitted_property_indices: admitted,
        });
    }
    if let Some(source) = source {
        if !matches!(
            item.quality,
            Some(ItemQuality::Low | ItemQuality::Normal | ItemQuality::High)
        ) || source.class_code.is_some()
        {
            row.unadmitted_contexts
                .push("automatic_overlapping_or_class_context");
        } else {
            for expected in source.affix.stats {
                if overlaps(item, known_primary_stats, expected.stat_id, expected.param) {
                    row.unadmitted_contexts
                        .push("automatic_template_or_affix_overlap");
                    continue;
                }
                let observations: Vec<_> = item
                    .properties
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.stat_id == expected.stat_id && p.param == expected.param)
                    .map(|(index, p)| ObservedProperty {
                        property_index: index,
                        param: p.param,
                        value: p.value,
                    })
                    .collect();
                let valid = observations.len() == 1
                    && observations[0].value >= expected.min.min(expected.max)
                    && observations[0].value <= expected.max.max(expected.min);
                if valid {
                    admitted.push(observations[0].property_index);
                } else {
                    diagnostic(
                        &mut row,
                        "automatic_property_mismatch",
                        format!(
                            "Source option ({}, {}) requires one value in {}..{}; observed {}",
                            expected.stat_id,
                            expected.param,
                            expected.min,
                            expected.max,
                            observations.len()
                        ),
                    );
                }
                row.automatic_checks.push(PropertyCheck {
                    source_kind: "automatic_required",
                    stat_id: expected.stat_id,
                    param: expected.param,
                    minimum: expected.min.min(expected.max),
                    maximum: expected.max.max(expected.min),
                    observed: observations,
                    admitted: valid,
                });
            }
        }
    }
    let observed_staff_count = staff.len();
    let eligible_staff: Vec<_> = staff
        .into_iter()
        .filter(|(_, p)| !overlaps(item, known_primary_stats, p.stat_id, p.param))
        .collect();
    if eligible_staff.len() != observed_staff_count {
        row.unadmitted_contexts
            .push("staffmod_template_or_affix_overlap");
    }
    if !eligible_staff.is_empty() {
        if let Some(class) = row.host_staffmods_class {
            let mut class_skills: Vec<_> = SKILLS.iter().filter(|s| s.charclass == class).collect();
            class_skills.sort_by_key(|s| s.id);
            let qualified_layout =
                class_skills.len() == 30 && class_skills.windows(2).all(|p| p[1].id == p[0].id + 1);
            if let (true, Some(level)) = (qualified_layout, item.level) {
                let first = class_skills[0].id;
                let base = if level > 36 {
                    5
                } else if level > 24 {
                    4
                } else if level > 18 {
                    3
                } else if level > 11 {
                    2
                } else {
                    1
                };
                let lower = (base - 2i32).max(1);
                let upper = if item.quality == Some(ItemQuality::Low) {
                    (base + 1).min(4)
                } else {
                    base + 1
                };
                let mut grouped: BTreeMap<u32, Vec<_>> = BTreeMap::new();
                for (index, p) in eligible_staff {
                    grouped.entry(p.param).or_default().push((index, p));
                }
                let count: usize = grouped.values().map(Vec::len).sum();
                if count > 3 {
                    diagnostic(
                        &mut row,
                        "staffmod_count",
                        format!(
                            "Observed {count} unexplained staffmod properties; source maximum is3"
                        ),
                    );
                }
                for (param, entries) in grouped {
                    let skill = class_skills.iter().find(|s| s.id == param);
                    let tier = skill.map(|s| ((s.id - first) / 5 + 1) as i32);
                    let valid = count <= 3
                        && entries.len() == 1
                        && entries[0].1.value >= 1
                        && entries[0].1.value <= 3
                        && tier.is_some_and(|t| t >= lower && t <= upper);
                    let observations: Vec<_> = entries
                        .iter()
                        .map(|(index, p)| ObservedProperty {
                            property_index: *index,
                            param: p.param,
                            value: p.value,
                        })
                        .collect();
                    if valid {
                        admitted.push(observations[0].property_index);
                    } else {
                        diagnostic(
                            &mut row,
                            "staffmod_property_mismatch",
                            format!(
                                "Skill parameter {param} is not one source-compatible {class} bonus1..3 in tiers{lower}..{upper}"
                            ),
                        );
                    }
                    row.staffmod_checks.push(PropertyCheck {
                        source_kind: "staffmod_optional",
                        stat_id: staff_id,
                        param,
                        minimum: 1,
                        maximum: 3,
                        observed: observations,
                        admitted: valid,
                    });
                }
            } else {
                row.unadmitted_contexts
                    .push("staffmod_class_layout_or_level");
            }
        } else {
            row.unadmitted_contexts.push("no_staffmod_host_source");
        }
    }
    row.status = if !row.diagnostics.is_empty() {
        "red"
    } else if !row.unadmitted_contexts.is_empty() {
        "partial"
    } else {
        "qualified"
    };
    Some(Outcome {
        evaluation: row,
        admitted_property_indices: admitted,
    })
}
