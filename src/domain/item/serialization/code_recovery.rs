use crate::data::generated::layout_policy::{
    legacy_alpha_body_capture_code_alias, legacy_alpha_empty_body_hint_fallback,
    legacy_alpha_trusted_body_repair_observation,
};
use crate::domain::forensic::registry::AlphaForensics;
use std::collections::HashMap;

/// Normalize one body code with the caller-supplied effective-code map.
/// The caller retains format admission and trusted hint handling.
fn normalize_alpha_body_code(code: &str, effective_codes: &HashMap<String, String>) -> String {
    let trimmed_code = code.trim();
    if let Some(alias) = legacy_alpha_body_capture_code_alias(trimmed_code) {
        return alias.to_string();
    }
    let mut code = trimmed_code.to_string();
    if let Some(eff) = effective_codes.get(trimmed_code) {
        code = eff.clone();
    }
    let normalized_body_code = crate::item::normalize_alpha_code_hint(&code).to_string();
    if normalized_body_code != code {
        code = normalized_body_code;
    }
    code
}

/// Recover an Alpha body code within the caller's format admission.
/// Classification retains its original guarded invocation through the callback.
pub(super) fn recover_alpha_body_code(
    code: &mut String,
    code_hint: Option<&str>,
    code_peek: Option<&str>,
    reg: &AlphaForensics,
    classify_summary: impl FnOnce(&str) -> bool,
) {
    promote_alpha_body_code_hint(code, code_hint, code_peek, reg, classify_summary);
    *code = normalize_alpha_body_code(code, &reg.effective_codes);
    repair_alpha_body_code_with_trusted_hint(code, code_hint, code_peek);
}

fn promote_alpha_body_code_hint(
    code: &mut String,
    code_hint: Option<&str>,
    code_peek: Option<&str>,
    reg: &AlphaForensics,
    classify_summary: impl FnOnce(&str) -> bool,
) {
    if code.trim().is_empty() {
        if let Some(hint) = code_hint.or(code_peek) {
            let trimmed_hint = hint.trim();
            let anchored_hint = !trimmed_hint.is_empty()
                && (classify_summary(trimmed_hint)
                    || reg
                        .forced_compact_codes
                        .as_ref()
                        .map(|codes| codes.iter().any(|c| c == trimmed_hint))
                        .unwrap_or(false)
                    || reg
                        .forced_runeword_codes
                        .as_ref()
                        .map(|codes| codes.iter().any(|c| c == trimmed_hint))
                        .unwrap_or(false)
                    || reg
                        .item_overrides
                        .as_ref()
                        .and_then(|overrides| overrides.get(trimmed_hint))
                        .and_then(|map| map.get("fixed_width"))
                        .copied()
                        .unwrap_or(0)
                        > 0);
            if anchored_hint {
                *code = trimmed_hint.to_string();
            }
        }
    }
    if let Some(hint) = code_hint.or(code_peek) {
        let trimmed_hint = hint.trim();
        if let Some(fallback) =
            legacy_alpha_empty_body_hint_fallback(trimmed_hint).filter(|_| code.trim().is_empty())
        {
            *code = fallback.to_string();
        }
    }
}

fn repair_alpha_body_code_with_trusted_hint(
    code: &mut String,
    code_hint: Option<&str>,
    code_peek: Option<&str>,
) {
    if let Some(hint) = code_hint.or(code_peek) {
        let trimmed_hint = hint.trim();
        // Copy the selected trusted hint only after its body observation matches.
        if legacy_alpha_trusted_body_repair_observation(trimmed_hint)
            .is_some_and(|observed_body| code.trim() == observed_body)
        {
            *code = trimmed_hint.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_alpha_body_code;
    use d2r_macros::hypothesis_test;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    #[hypothesis_test]
    fn legacy_alpha_body_normalization_preserves_phase_precedence() {
        // Freeze this phase independently of future Data aliases and registry defaults.
        let inputs = [
            ("g71", "jav", Some("jav")),
            ("wl l", "jav", Some("jav")),
            ("us g", "jav", Some("jav")),
            ("k g", "jav", Some("jav")),
            ("wucb", "ucb8", Some("ucb8")),
            ("7 p", "ks d", None),
            ("80sc", "ks d", None),
            ("lbl", "b7ts", None),
            ("whp1", "hp1", None),
            ("wmp1", "mp1", None),
            ("mp1", "mp1", None),
            ("", "", None),
            ("gcw", "gcw", None),
            ("ww", "ww", None),
            ("US G", "US G", None),
            ("unknown", "unknown", None),
            ("a\0", "a\0", None),
            ("us g\0", "us g\0", None),
            ("\u{cf}O", "hp1", None),
            ("\u{1cf}\u{14d}", "mp1", None),
            ("wa2", "wa2", None),
            ("rhd", "rhd", None),
        ];
        let effective_values = [
            ("g71", "g71"),
            ("wucb", "wucb"),
            (" us g ", "jav"),
            (" wmp1 ", "mp1"),
            ("mp1", "mp1"),
            (" raw ", "raw"),
            ("", ""),
            ("\t", ""),
            ("us g\0", "us g\0"),
            ("\u{cf}O", "hp1"),
            (" 7 p ", "ks d"),
            ("WA2", "WA2"),
            ("raw", "raw"),
            ("wl l", "wl l"),
            ("us  g", "us  g"),
            (" wucb ", "wucb"),
        ];
        assert_eq!((inputs.len(), effective_values.len()), (22, 16));
        let mut rows = Vec::new();
        for (base, no_registry_output, capture_output) in inputs {
            for input in [
                base.to_owned(),
                format!(" {base} "),
                format!("\t{base}\r\n"),
                format!("\u{2003}{base}\u{3000}"),
            ] {
                for scenario in 0..=effective_values.len() {
                    let mut registry = HashMap::new();
                    let expected_without_capture = if scenario == 0 {
                        no_registry_output
                    } else {
                        let (effective, normalized) = effective_values[scenario - 1];
                        // Poison chained lookups: the phase must read only the original key.
                        for key in ["g71", "wucb", "jav", "mp1"] {
                            registry.insert(key.to_owned(), "registry_retry_poison".to_owned());
                        }
                        registry.insert(base.to_owned(), effective.to_owned());
                        normalized
                    };
                    let expected = capture_output.unwrap_or(expected_without_capture);
                    let before = registry.clone();
                    let actual = normalize_alpha_body_code(&input, &registry);
                    assert_eq!(actual, expected, "input {input:?}, scenario {scenario}");
                    assert_eq!(registry, before, "registry must remain unchanged");
                    let sorted_registry: BTreeMap<_, _> = registry.iter().collect();
                    rows.push(json!({
                        "input": input, "scenario": scenario, "registry": sorted_registry,
                        "expected": expected, "actual": actual,
                    }));
                }
            }
        }
        assert_eq!(rows.len(), 1496);
        let registry = HashMap::from([(" raw ".to_owned(), "padded_key_poison".to_owned())]);
        let before = registry.clone();
        let actual = normalize_alpha_body_code("\t raw \n", &registry);
        assert_eq!(actual, "raw");
        assert_eq!(registry, before);
        rows.push(json!({"input": "\t raw \n", "scenario": "padded-key-only", "registry": BTreeMap::from([(" raw ", "padded_key_poison")]), "expected": "raw", "actual": actual}));
        assert_eq!(rows.len(), 1497);
        let report = json!({
            "schema": 1, "cases": rows.len(), "input_modes": 4,
            "evidence": "source-observed phase preservation, not universal format truth",
            "protocol": "capture alias before one trimmed-key registry lookup; global hint normalization only after lookup; registry unchanged",
            "rows": rows,
        });
        if let Some(output) = std::env::var_os("D2R_REFACTOR_BODY_OUTPUT") {
            let path = std::path::PathBuf::from(output);
            assert!(path.parent().unwrap().is_dir());
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            serde_json::to_writer_pretty(&mut file, &report).unwrap();
        }
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::{promote_alpha_body_code_hint, recover_alpha_body_code};
    use crate::domain::forensic::registry::AlphaForensics;
    use d2r_macros::hypothesis_test;
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, HashMap};

    #[derive(Clone, Copy, Debug)]
    enum RegistryCase {
        Empty,
        Compact,
        PaddedCompact,
        Runeword,
        WidthZero,
        WidthOne,
        WidthMax,
        WrongWidthField,
        WrongOverrideCode,
        CompactXrs,
        CompactC8xr,
        EffectiveWucb,
        EffectiveG71,
        EffectiveUsG,
        CompactAndEffectiveWucb,
        WidthNul,
        EmptyEffective,
    }

    fn registry(recipe: RegistryCase) -> AlphaForensics {
        let mut reg = AlphaForensics::default();
        match recipe {
            RegistryCase::Empty => {}
            RegistryCase::Compact => reg.forced_compact_codes = Some(vec!["probe".to_owned()]),
            RegistryCase::PaddedCompact => {
                reg.forced_compact_codes = Some(vec![" probe ".to_owned()]);
            }
            RegistryCase::Runeword => reg.forced_runeword_codes = Some(vec!["probe".to_owned()]),
            RegistryCase::WidthZero
            | RegistryCase::WidthOne
            | RegistryCase::WidthMax
            | RegistryCase::WrongWidthField
            | RegistryCase::WrongOverrideCode
            | RegistryCase::WidthNul => {
                let (code, key, value) = match recipe {
                    RegistryCase::WidthZero => ("probe", "fixed_width", 0),
                    RegistryCase::WidthOne => ("probe", "fixed_width", 1),
                    RegistryCase::WidthMax => ("probe", "fixed_width", u32::MAX),
                    RegistryCase::WrongWidthField => ("probe", "width", 1),
                    RegistryCase::WrongOverrideCode => (" probe ", "fixed_width", 1),
                    RegistryCase::WidthNul => ("probe\0", "fixed_width", 1),
                    _ => unreachable!(),
                };
                reg.item_overrides = Some(HashMap::from([(
                    code.to_owned(),
                    HashMap::from([(key.to_owned(), value)]),
                )]));
            }
            RegistryCase::CompactXrs => reg.forced_compact_codes = Some(vec!["xrs".to_owned()]),
            RegistryCase::CompactC8xr => reg.forced_compact_codes = Some(vec!["c8xr".to_owned()]),
            RegistryCase::EffectiveWucb => {
                reg.effective_codes
                    .insert("unknown".to_owned(), " wucb ".to_owned());
            }
            RegistryCase::EffectiveG71 => {
                reg.effective_codes
                    .insert("unknown".to_owned(), "g71".to_owned());
                reg.effective_codes
                    .insert("wucb".to_owned(), "g71".to_owned());
            }
            RegistryCase::EffectiveUsG => {
                reg.effective_codes
                    .insert("unknown".to_owned(), " us g ".to_owned());
            }
            RegistryCase::CompactAndEffectiveWucb => {
                reg.forced_compact_codes = Some(vec!["probe".to_owned()]);
                reg.effective_codes
                    .insert("probe".to_owned(), " wucb ".to_owned());
            }
            RegistryCase::EmptyEffective => {
                reg.effective_codes
                    .insert(String::new(), " wucb ".to_owned());
            }
        }
        reg
    }

    fn fields(reg: &AlphaForensics) -> Value {
        let effective: BTreeMap<_, _> = reg.effective_codes.iter().collect();
        let overrides = reg.item_overrides.as_ref().map(|items| {
            items
                .iter()
                .map(|(code, values)| (code, values.iter().collect::<BTreeMap<_, _>>()))
                .collect::<BTreeMap<_, _>>()
        });
        json!({
            "effective_codes": effective, "item_overrides": overrides,
            "forced_compact_codes": reg.forced_compact_codes,
            "forced_runeword_codes": reg.forced_runeword_codes,
        })
    }

    fn padded(value: &str, mode: usize) -> String {
        match mode {
            0 => value.to_owned(),
            1 => format!(" {value} "),
            2 => format!("\t{value}\r\n"),
            3 => format!("\u{2003}{value}\u{3000}"),
            _ => unreachable!(),
        }
    }

    #[hypothesis_test]
    fn legacy_alpha_body_recovery_preserves_hint_stages() {
        use RegistryCase::*;
        // Source-observed expectations, independent of candidate Data policy.
        // label, body, hint, peek, registry, summary reply, promotion, final, call.
        let cases = [
            ("no-hint", "", None, None, Empty, false, None, "", None),
            (
                "peek-summary",
                "",
                None,
                Some("probe"),
                Empty,
                true,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "summary-accept",
                "",
                Some("probe"),
                None,
                Empty,
                true,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "summary-deny",
                "",
                Some("probe"),
                None,
                Empty,
                false,
                None,
                "",
                Some("probe"),
            ),
            (
                "empty-shadows-peek",
                "",
                Some(""),
                Some("probe"),
                Compact,
                true,
                None,
                "",
                None,
            ),
            (
                "blank-shadows-peek",
                "",
                Some(" \t"),
                Some("probe"),
                Compact,
                true,
                None,
                "",
                None,
            ),
            (
                "body-suppresses-summary",
                "known",
                Some("probe"),
                None,
                Compact,
                true,
                None,
                "known",
                None,
            ),
            (
                "capture-suppresses-summary",
                "g71",
                Some("probe"),
                None,
                Compact,
                true,
                None,
                "jav",
                None,
            ),
            (
                "compact-exact",
                "",
                Some("probe"),
                None,
                Compact,
                false,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "compact-padded-key",
                "",
                Some("probe"),
                None,
                PaddedCompact,
                false,
                None,
                "",
                Some("probe"),
            ),
            (
                "runeword-exact",
                "",
                Some("probe"),
                None,
                Runeword,
                false,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "width-zero",
                "",
                Some("probe"),
                None,
                WidthZero,
                false,
                None,
                "",
                Some("probe"),
            ),
            (
                "width-one",
                "",
                Some("probe"),
                None,
                WidthOne,
                false,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "width-max",
                "",
                Some("probe"),
                None,
                WidthMax,
                false,
                Some("probe"),
                "probe",
                Some("probe"),
            ),
            (
                "wrong-width-field",
                "",
                Some("probe"),
                None,
                WrongWidthField,
                false,
                None,
                "",
                Some("probe"),
            ),
            (
                "wrong-override-code",
                "",
                Some("probe"),
                None,
                WrongOverrideCode,
                false,
                None,
                "",
                Some("probe"),
            ),
            (
                "xrs-fallback-space",
                "",
                Some("xrs"),
                None,
                Empty,
                false,
                Some("xrs "),
                "xrs",
                Some("xrs"),
            ),
            (
                "c8xr-fallback-space",
                "",
                Some("c8xr"),
                None,
                Empty,
                false,
                Some("xrs "),
                "xrs",
                Some("c8xr"),
            ),
            (
                "anchored-xrs-no-space",
                "",
                Some("xrs"),
                None,
                CompactXrs,
                false,
                Some("xrs"),
                "xrs",
                Some("xrs"),
            ),
            (
                "anchored-c8xr",
                "",
                Some("c8xr"),
                None,
                CompactC8xr,
                false,
                Some("c8xr"),
                "c8xr",
                Some("c8xr"),
            ),
            (
                "plain-capture",
                "us g",
                None,
                None,
                Empty,
                false,
                None,
                "jav",
                None,
            ),
            (
                "trusted-after-map",
                "unknown",
                Some("ucb8"),
                None,
                EffectiveWucb,
                false,
                None,
                "ucb8",
                None,
            ),
            (
                "untrusted-after-map",
                "unknown",
                Some("other"),
                None,
                EffectiveWucb,
                false,
                None,
                "wucb",
                None,
            ),
            (
                "repair-empty-shadows-peek",
                "unknown",
                Some(""),
                Some("ucb8"),
                EffectiveWucb,
                false,
                None,
                "wucb",
                None,
            ),
            (
                "repair-peek",
                "unknown",
                None,
                Some("ucb8"),
                EffectiveWucb,
                false,
                None,
                "ucb8",
                None,
            ),
            (
                "direct-capture-beats-map",
                "wucb",
                Some("other"),
                None,
                EffectiveG71,
                false,
                None,
                "ucb8",
                None,
            ),
            (
                "no-capture-retry",
                "unknown",
                None,
                None,
                EffectiveG71,
                false,
                None,
                "g71",
                None,
            ),
            (
                "global-alias-after-map",
                "unknown",
                None,
                None,
                EffectiveUsG,
                false,
                None,
                "jav",
                None,
            ),
            (
                "selected-hint-precedence",
                "",
                Some("probe"),
                Some("ucb8"),
                CompactAndEffectiveWucb,
                false,
                Some("probe"),
                "wucb",
                Some("probe"),
            ),
            (
                "nul-key-miss",
                "",
                Some("probe\0"),
                None,
                WidthOne,
                false,
                None,
                "",
                Some("probe\0"),
            ),
            (
                "nul-key-exact",
                "",
                Some("probe\0"),
                None,
                WidthNul,
                false,
                Some("probe\0"),
                "probe\0",
                Some("probe\0"),
            ),
            (
                "empty-code-registry",
                "",
                None,
                None,
                EmptyEffective,
                false,
                None,
                "wucb",
                None,
            ),
        ];
        assert_eq!(cases.len(), 32);
        let mut rows = Vec::new();
        for (label, body, hint, peek, recipe, summary, promotion, expected, call) in cases {
            for body_mode in 0..4 {
                for hint_mode in 0..4 {
                    for peek_mode in 0..4 {
                        let input = padded(body, body_mode);
                        let hint = hint.map(|value| padded(value, hint_mode));
                        let peek = peek.map(|value| padded(value, peek_mode));
                        let reg = registry(recipe);
                        let before = fields(&reg);
                        let expected_calls: Vec<_> = call.into_iter().map(str::to_owned).collect();
                        let mut promoted = input.clone();
                        let mut promotion_calls = Vec::new();
                        promote_alpha_body_code_hint(
                            &mut promoted,
                            hint.as_deref(),
                            peek.as_deref(),
                            &reg,
                            |code| {
                                promotion_calls.push(code.to_owned());
                                summary
                            },
                        );
                        assert_eq!(promoted, promotion.unwrap_or(&input), "promotion {label}");
                        assert_eq!(promotion_calls, expected_calls, "promotion calls {label}");
                        let mut actual = input.clone();
                        let mut calls = Vec::new();
                        recover_alpha_body_code(
                            &mut actual,
                            hint.as_deref(),
                            peek.as_deref(),
                            &reg,
                            |code| {
                                calls.push(code.to_owned());
                                summary
                            },
                        );
                        assert_eq!(actual, expected, "final {label}");
                        assert_eq!(calls, expected_calls, "full calls {label}");
                        assert_eq!(fields(&reg), before, "registry mutation {label}");
                        rows.push(json!({
                            "case": label, "body_mode": body_mode, "hint_mode": hint_mode,
                            "peek_mode": peek_mode, "body": input, "hint": hint, "peek": peek,
                            "registry_recipe": format!("{recipe:?}"), "registry": before,
                            "summary_reply": summary, "promotion": promoted,
                            "expected_promotion": promotion, "actual": actual, "expected": expected,
                            "summary_calls": calls, "promotion_calls": promotion_calls,
                        }));
                    }
                }
            }
        }
        assert_eq!(rows.len(), 2048);
        let report = json!({
            "schema": 1, "cases": rows.len(), "stage_cases": cases.len(),
            "whitespace_modes": 4, "whitespace_axes": 3,
            "evidence": "source-observed phase and callback contract; not universal item classification",
            "rows": rows,
        });
        if let Some(output) = std::env::var_os("D2R_REFACTOR_RECOVERY_OUTPUT") {
            let path = std::path::PathBuf::from(output);
            assert!(path.parent().unwrap().is_dir());
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            serde_json::to_writer_pretty(&mut file, &report).unwrap();
        }
    }
}
