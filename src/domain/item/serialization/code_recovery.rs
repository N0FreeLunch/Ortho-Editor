use std::collections::HashMap;

/// Normalize one body code with the caller-supplied effective-code map.
/// The caller retains format admission and trusted hint handling.
pub(super) fn normalize_alpha_body_code(
    code: &str,
    effective_codes: &HashMap<String, String>,
) -> String {
    let trimmed_code = code.trim();
    match trimmed_code {
        // Capture-replay aliases for the live `jav` witness.
        "g71" | "wl l" | "us g" | "k g" => "jav".to_string(),
        "wucb" => "ucb8".to_string(),
        other => {
            let mut code = other.to_string();
            if let Some(eff) = effective_codes.get(other) {
                code = eff.clone();
            }
            let normalized_body_code = crate::item::normalize_alpha_code_hint(&code).to_string();
            if normalized_body_code != code {
                code = normalized_body_code;
            }
            code
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
