use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s")
}

fn assert_failure(output: std::process::Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "destination failure must not report success: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "destination failure must be diagnosable"
    );
    assert!(!stderr.contains("Verification complete: OK"));
}

#[test]
fn explicit_missing_parent_is_an_error() {
    let fixture = fixture();
    let before = fs::read(&fixture).expect("fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tmp")
        .join(format!("output-init-test-{}-{nonce}", std::process::id()))
        .join("absent-parent/report.json");
    assert!(!output.parent().unwrap().exists());
    let result = Command::new(env!("CARGO_BIN_EXE_d2save_verify"))
        .args(["--json", "--token-efficient", "--output"])
        .arg(&output)
        .arg(&fixture)
        .output()
        .expect("run verifier");
    assert!(!output.exists());
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
    assert_failure(result);
}

#[test]
fn automatic_directory_failure_is_an_error() {
    let fixture = fixture();
    let before = fs::read(&fixture).expect("fixture");
    let result = Command::new(env!("CARGO_BIN_EXE_d2save_verify"))
        .args(["--json", "--token-efficient"])
        .arg(&fixture)
        .env("AGENT_ARTIFACTS_PATH", &fixture)
        .output()
        .expect("run verifier");
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
    assert_failure(result);
}

#[test]
fn all_remaining_consumers_report_output_initialization_failure() {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = core.join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    let before = fs::read(&fixture).expect("fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = core
        .join("tmp")
        .join(format!("output-migration-{}-{nonce}", std::process::id()));
    let cases = [
        (
            "d2item_bit_align",
            env!("CARGO_BIN_EXE_d2item_bit_align"),
            "single",
        ),
        (
            "d2item_chunk_verify",
            env!("CARGO_BIN_EXE_d2item_chunk_verify"),
            "single",
        ),
        (
            "d2item_code_fuzzer",
            env!("CARGO_BIN_EXE_d2item_code_fuzzer"),
            "single",
        ),
        (
            "d2item_oracle_mapper",
            env!("CARGO_BIN_EXE_d2item_oracle_mapper"),
            "single",
        ),
        (
            "d2item_property_fuzzer",
            env!("CARGO_BIN_EXE_d2item_property_fuzzer"),
            "single",
        ),
        (
            "d2item_section_prefix_trace",
            env!("CARGO_BIN_EXE_d2item_section_prefix_trace"),
            "single",
        ),
        (
            "d2item_serialization_audit",
            env!("CARGO_BIN_EXE_d2item_serialization_audit"),
            "single",
        ),
        (
            "d2save_mercenary_audit",
            env!("CARGO_BIN_EXE_d2save_mercenary_audit"),
            "single",
        ),
        ("d2save_diff", env!("CARGO_BIN_EXE_d2save_diff"), "double"),
        ("d2save_dsa", env!("CARGO_BIN_EXE_d2save_dsa"), "double"),
        (
            "d2save_item_diff",
            env!("CARGO_BIN_EXE_d2save_item_diff"),
            "double",
        ),
        ("d2save_ssa", env!("CARGO_BIN_EXE_d2save_ssa"), "double"),
        (
            "d2save_structural_diff",
            env!("CARGO_BIN_EXE_d2save_structural_diff"),
            "double",
        ),
        (
            "d2item_boundary_oracle",
            env!("CARGO_BIN_EXE_d2item_boundary_oracle"),
            "boundary",
        ),
        ("sba", env!("CARGO_BIN_EXE_sba"), "sba"),
        (
            "d2save_temporal_witness",
            env!("CARGO_BIN_EXE_d2save_temporal_witness"),
            "temporal",
        ),
        (
            "d2item_inspect",
            env!("CARGO_BIN_EXE_d2item_inspect"),
            "inspect",
        ),
    ];
    let mut failures = Vec::new();
    for (name, binary, kind) in cases {
        let dir = work.join(name);
        let missing = dir.join("missing-input.d2s");
        let report = dir.join("absent-output-parent/report.json");
        let mut cmd = Command::new(binary);
        match kind {
            "single" => {
                cmd.arg(&missing);
            }
            "double" => {
                cmd.arg(&missing).arg(&missing);
            }
            "boundary" => {
                cmd.arg("--fixture").arg(&missing);
            }
            "sba" => {
                cmd.arg("--fixture")
                    .arg(&missing)
                    .arg("--baseline")
                    .arg(dir.join("missing.json"));
            }
            "temporal" => {
                cmd.arg(dir.join("missing-directory"));
            }
            "inspect" => {
                cmd.arg(&fixture)
                    .args(["--dialect", "legacy", "--section-item", "0"]);
            }
            _ => unreachable!(),
        }
        let result = cmd
            .args(["--json", "--token-efficient", "--output"])
            .arg(&report)
            .current_dir(&core)
            .env("AGENT_ARTIFACTS_PATH", &dir)
            .env_remove("D2R_ITEM_TRACE")
            .output()
            .expect("run consumer");
        let stderr = String::from_utf8_lossy(&result.stderr);
        if result.status.success()
            || !stderr.contains("Failed to initialize output destination")
            || report.exists()
        {
            failures.push(format!("{name}: status={}, stderr={stderr}", result.status));
        }
    }
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
    assert!(
        failures.is_empty(),
        "{} consumer failures: {failures:#?}",
        failures.len()
    );
}
