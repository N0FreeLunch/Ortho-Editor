#[cfg(target_os = "linux")]
#[test]
fn full_device_write_failure_never_reports_success() {
    use std::fs;
    use std::os::unix::fs::FileTypeExt;
    use std::path::PathBuf;
    use std::process::Command;

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    let before = fs::read(&fixture).expect("read fixture");
    assert!(
        fs::metadata("/dev/full")
            .expect("/dev/full")
            .file_type()
            .is_char_device()
    );
    let mut failures = Vec::new();
    for (name, flags) in [
        ("standard", vec!["--json"]),
        ("dump_bits", vec!["--dump-bits", "0", "16"]),
        ("residue_map", vec!["--json", "--dump-residue-map"]),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_d2save_verify"))
            .args(flags)
            .args(["--token-efficient", "--output", "/dev/full"])
            .arg(&fixture)
            .env_remove("D2R_ITEM_TRACE")
            .output()
            .expect("run verifier");
        let stderr = String::from_utf8_lossy(&result.stderr);
        if result.status.success()
            || !stderr.contains("os error 28")
            || stderr.contains("Verification complete: OK")
        {
            failures.push(format!("{name}: status={}, stderr={stderr}", result.status));
        }
    }
    assert_eq!(fs::read(&fixture).expect("read fixture after"), before);
    assert!(
        failures.is_empty(),
        "{} failing routes: {failures:#?}",
        failures.len()
    );
}

#[test]
fn diff_summary_follows_delta_output() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    let before = fs::read(&fixture).expect("read fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = std::env::temp_dir().join(format!("verifier-delta-{}-{nonce}", std::process::id()));
    fs::create_dir(&work).expect("create owned test directory");
    let baseline = work.join("baseline.json");
    let report = work.join("report.txt");
    let generated = Command::new(env!("CARGO_BIN_EXE_d2save_verify"))
        .args(["--json", "--token-efficient", "--output"])
        .arg(&baseline)
        .arg(&fixture)
        .output()
        .expect("generate baseline");
    assert!(generated.status.success());
    let result = Command::new(env!("CARGO_BIN_EXE_d2save_verify"))
        .args(["--token-efficient", "--output"])
        .arg(&report)
        .arg("--diff-baseline")
        .arg(&baseline)
        .arg(&fixture)
        .output()
        .expect("run comparison");
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 stdout");
    assert!(
        stdout.find("FIDELITY DELTA").unwrap() < stdout.find("Verification complete: OK").unwrap()
    );
    let text = fs::read_to_string(&report).expect("read report");
    assert!(text.find("FIDELITY DELTA").unwrap() < text.find("Verification complete: OK").unwrap());
    assert_eq!(fs::read(&fixture).expect("read fixture after"), before);
    fs::remove_file(baseline).expect("remove owned baseline");
    fs::remove_file(report).expect("remove owned report");
    fs::remove_dir(work).expect("remove owned directory");
}
