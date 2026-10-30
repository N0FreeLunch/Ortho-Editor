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
