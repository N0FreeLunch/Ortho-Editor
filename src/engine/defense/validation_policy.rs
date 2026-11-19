//! Preserve stored-field and legacy defense warning contracts.

use crate::data::DataRepository;
use crate::data::item_specs::get_base_item_spec;
use crate::domain::stats::fits_unsigned_save_field;
use crate::engine::validation::model::{ValidationWarning, WarningConfidenceTier};
use crate::item::{Item, ItemQuality};

pub(crate) fn stored_defense_warnings(item: &Item) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    let Some(template) = DataRepository::get_item_template(item.code.trim()) else {
        return warnings;
    };
    let mut warn = |category, message| {
        warnings.push(ValidationWarning::new(
            message,
            WarningConfidenceTier::HardInvariant,
            category,
        ));
    };
    if item.body.defense != item.defense {
        warn(
            "defense_mirror",
            "Stored defense disagrees with its legacy mirror.".to_string(),
        );
    }
    if !template.is_armor {
        if item.body.defense.is_some() {
            warn(
                "defense_layout",
                "A non-armor item contains a stored armor defense field.".to_string(),
            );
        }
        return warnings;
    }
    let Some(base) = item.body.defense else {
        warn(
            "defense_missing",
            "The armor's stored defense field is missing.".to_string(),
        );
        return warnings;
    };
    let representable = fits_unsigned_save_field(31, base);
    if !representable {
        warn(
            "defense_encoding",
            format!("Stored defense {base} does not fit its save-field encoding."),
        );
    }
    warnings
}

pub fn check_base_stat_legitimacy_detailed(item: &Item) -> Vec<ValidationWarning> {
    if crate::engine::is_explicit_v105(item) {
        return crate::engine::defense::stored_defense_warnings(item);
    }
    let mut warnings = Vec::new();
    let trimmed = item.code.trim();
    let spec = if let Some(s) = get_base_item_spec(trimmed) {
        s
    } else {
        return warnings;
    };

    // Defense Verification
    if let Some(actual_def) = item.defense {
        let mut ed_percent = 0;
        let mut flat_def = 0;
        for prop in &item.properties {
            if prop.stat_id == 16 {
                ed_percent += prop.value;
            } else if prop.stat_id == 31 {
                flat_def += prop.value;
            }
        }

        let is_max_ac_plus_one = ed_percent > 0
            || item.is_runeword
            || matches!(
                item.quality,
                Some(ItemQuality::Unique) | Some(ItemQuality::Set)
            );
        let eth_mul = if item.is_ethereal { 1.5 } else { 1.0 };

        if is_max_ac_plus_one {
            let base_ac = spec.max_ac as f32 + 1.0;
            let calc = ((base_ac * eth_mul).floor() * (100.0 + ed_percent as f32) / 100.0).floor()
                as i64
                + flat_def as i64;
            let expected = calc.max(0) as u32;
            if actual_def != expected && (actual_def as i64 - expected as i64).abs() > 1 {
                // Fixed specs (Unique/Set/Runeword) are SettledContract; others are EvolvingHypothesis
                let tier = if matches!(
                    item.quality,
                    Some(ItemQuality::Unique) | Some(ItemQuality::Set)
                ) || item.is_runeword
                {
                    WarningConfidenceTier::SettledContract
                } else {
                    WarningConfidenceTier::EvolvingHypothesis
                };
                warnings.push(ValidationWarning::new(
                    format!(
                        "Defense {} does not match expected {} (Superior/Unique Rule)",
                        actual_def, expected
                    ),
                    tier,
                    "defense_formula",
                ));
            }
        } else {
            let min_calc = (spec.min_ac as f32 * eth_mul).floor() as i64 + flat_def as i64;
            let max_calc = (spec.max_ac as f32 * eth_mul).floor() as i64 + flat_def as i64;
            let min_expected = min_calc.max(0) as u32;
            let max_expected = max_calc.max(0) as u32;

            if actual_def < min_expected || actual_def > max_expected {
                // In D2, Rare/Magic/Crafted items can roll base defense with affixes or level bonuses
                // that may not be fully captured without complete affix resolution.
                // We classify this as EvolvingHypothesis rather than a hard violation.
                let is_complex_quality = matches!(
                    item.quality,
                    Some(ItemQuality::Rare) | Some(ItemQuality::Magic) | Some(ItemQuality::Crafted)
                );
                let tier = if is_complex_quality {
                    WarningConfidenceTier::EvolvingHypothesis
                } else {
                    WarningConfidenceTier::SettledContract
                };

                // Possibility Envelope for Rare/Magic: allow maximum possible prefix flat def (+60) before warning
                let rare_envelope_max = if is_complex_quality {
                    max_expected + 60
                } else {
                    max_expected
                };

                if actual_def < min_expected || actual_def > rare_envelope_max {
                    warnings.push(ValidationWarning::new(
                        format!(
                            "Defense {} out of range {}-{}",
                            actual_def, min_expected, max_expected
                        ),
                        tier,
                        "defense_range",
                    ));
                }
            }
        }
    }

    warnings
}

pub fn check_base_stat_legitimacy(item: &Item) -> Vec<String> {
    check_base_stat_legitimacy_detailed(item)
        .into_iter()
        .map(|w| w.message)
        .collect()
}
