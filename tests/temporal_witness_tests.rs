use serde_json::Value;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn temporal_witness_emits_a_bounded_json_index() {
    let output = std::env::temp_dir().join(format!(
        "d2save_temporal_witness_{}_{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = Command::new(env!("CARGO_BIN_EXE_d2save_temporal_witness"))
        .args([
            "tests/fixtures/savegames/gameplay/normal/act1",
            "--max-files",
            "2",
            "--json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("temporal witness binary must run");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    let _ = fs::remove_file(&output);
    assert_eq!(report["metadata"]["tool"], "d2save_temporal_witness");
    assert_eq!(report["scan_results"]["nodes"].as_array().unwrap().len(), 2);
    let edge = report["scan_results"]["edges"]
        .as_array()
        .unwrap()
        .first()
        .unwrap();
    assert!(edge["before"]
        .as_str()
        .unwrap()
        .contains("TESTASSASSIN_Act1_FreshStart"));
    assert!(edge["after"]
        .as_str()
        .unwrap()
        .contains("TESTASSASSIN_Act1_BloodRavenKilled_PreKashya"));
    assert_eq!(edge["relation"], "forward_partial_order");
    let runeword_delta = edge["item_deltas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|delta| {
            delta["key"]
                .as_str()
                .unwrap_or_default()
                .starts_with("oesw|Runeword")
        })
        .expect("bounded fixture edge must expose the runeword header candidate");
    assert_eq!(runeword_delta["template_candidate"], true);
    assert_eq!(
        runeword_delta["template_candidate_basis"],
        "runeword_header_flag"
    );
    assert!(report["scan_results"]["hint"]
        .as_str()
        .unwrap()
        .contains("partial order"));
}

#[test]
fn temporal_witness_skips_the_lexical_prefix_before_limiting() {
    let output = std::env::temp_dir().join(format!(
        "d2save_temporal_witness_skip_{}_{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = Command::new(env!("CARGO_BIN_EXE_d2save_temporal_witness"))
        .args([
            "tests/fixtures/savegames/gameplay/normal/act1",
            "--skip-files",
            "2",
            "--max-files",
            "1",
            "--json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("temporal witness binary must run");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    let _ = fs::remove_file(&output);
    let node = report["scan_results"]["nodes"]
        .as_array()
        .unwrap()
        .first()
        .unwrap();
    assert!(node["path"]
        .as_str()
        .unwrap()
        .contains("TESTASSASSIN_Act1_KashyaTalked_AlizaHired"));
}
