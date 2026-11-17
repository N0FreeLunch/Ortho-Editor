//! Match explicit property ranges and grade their existing rolls.

use super::model::{StatValidation, StatValidationStatus, ValidationResult};
use crate::data::item_specs::ItemStatRange;
use crate::item::{ItemBitRange, ItemProperty};

pub(super) fn validate_item_properties(
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
            let (score, in_range, is_perfect) =
                calculate_score(prop.value, spec_stat.min, spec_stat.max);

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
        detailed_warnings: Vec::new(),
    }
}

fn find_matching_spec_index(
    prop: &ItemProperty,
    spec_stats: &[ItemStatRange],
    used_specs: &[bool],
) -> Option<usize> {
    spec_stats.iter().enumerate().find_map(|(idx, spec_stat)| {
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
