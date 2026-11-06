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
fn automatic_reports_preserve_existing_files_and_concurrent_outputs() {
    struct OwnedDir(PathBuf);
    impl Drop for OwnedDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = core.join("tests/fixtures/savegames/original/amazon_initial.d2s");
    let before = fs::read(&fixture).expect("fixture");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let root = core.join("tmp");
    fs::create_dir_all(&root).expect("temporary root");
    let path = root.join(format!(
        "output-collision-{}-{}",
        std::process::id(),
        now.as_nanos()
    ));
    fs::create_dir(&path).expect("unique directory");
    let work = OwnedDir(path);
    let mut sentinels = Vec::new();
    for second in now.as_secs().saturating_sub(5)..=now.as_secs() + 120 {
        let path = work.0.join(format!("d2item_inspect_{second}.txt"));
        fs::write(&path, b"existing report").unwrap();
        sentinels.push(path);
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|index| {
            let root = work.0.clone();
            let fixture = fixture.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Command::new(env!("CARGO_BIN_EXE_d2item_inspect"))
                    .args([
                        "--dialect",
                        "legacy",
                        "--json",
                        "--token-efficient",
                        "--section-item",
                        &index.to_string(),
                        "--trace-segments",
                    ])
                    .arg(fixture)
                    .env("AGENT_ARTIFACTS_PATH", root)
                    .output()
                    .unwrap()
            })
        })
        .collect();
    let mut paths = Vec::new();
    for handle in handles {
        let result = handle.join().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stderr = String::from_utf8(result.stderr).unwrap();
        let path = stderr
            .lines()
            .find_map(|line| line.strip_prefix("[TOKEN-EFFICIENT] Detail output path: "))
            .expect("destination notice");
        let path = PathBuf::from(path);
        assert_eq!(fs::read(&path).expect("report"), result.stdout);
        paths.push(path);
    }
    assert_ne!(paths[0], paths[1]);
    for path in sentinels {
        assert_eq!(fs::read(path).unwrap(), b"existing report");
    }
    assert_eq!(fs::read(fixture).unwrap(), before);
}

#[test]
fn legacy_inspector_parse_and_comparison_reports_reach_destination() {
    struct OwnedDir(PathBuf);
    impl Drop for OwnedDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = core.join("tests/fixtures/savegames/original/amazon_initial.d2s");
    let raw = core.join("tests/fixtures/items/portal_scroll.d2i");
    let before = fs::read(&fixture).expect("fixture");
    let raw_before = fs::read(&raw).expect("raw fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = core.join("tmp");
    fs::create_dir_all(&root).expect("temporary root");
    let path = root.join(format!("inspector-parse-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).expect("create unique test directory");
    let work = OwnedDir(path);
    let invalid = work.0.join("invalid.d2s");
    fs::write(&invalid, [0u8; 16]).expect("synthetic comparison");
    let empty = work.0.join("empty.d2s");
    let mut bytes = before.clone();
    assert_eq!(&bytes[903..905], b"JM");
    bytes[905..907].copy_from_slice(&[0, 0]);
    fs::write(&empty, bytes).expect("zero-count comparison");
    let trace_failure = work.0.join("trace-failure.d2s");
    let mut bytes = before.clone();
    bytes[913] ^= 0x80;
    fs::write(&trace_failure, bytes).expect("synthetic trace failure");
    let raw_single = work.0.join("raw-single.d2i");
    let mut bytes = vec![0; 4];
    bytes.extend_from_slice(&105u32.to_le_bytes());
    bytes.extend_from_slice(b"JM");
    bytes.extend_from_slice(&4096u16.to_le_bytes());
    bytes.extend_from_slice(&before[907..917]);
    fs::write(&raw_single, bytes).expect("synthetic single raw item");
    let missing = work.0.join("missing.d2s");
    for name in [
        "marker-ordinal",
        "bit-trace-error",
        "bit-parse-error",
        "candidate-error",
        "compare-missing",
        "compare-invalid",
        "compare-empty",
        "bounded-selected",
        "selected-trace",
        "selected-trace-failure",
        "raw-item",
        "raw-single",
    ] {
        for efficient in [false, true] {
            let dest = work.0.join(format!("{name}-{efficient}.json"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_d2item_inspect"));
            cmd.args(["--dialect", "legacy", "--json", "--output"])
                .arg(&dest);
            if efficient {
                cmd.arg("--token-efficient");
            }
            match name {
                "marker-ordinal" => {
                    cmd.args(["--marker-section", "1", "--marker-ordinal", "999"]);
                }
                "bit-trace-error" => {
                    cmd.args(["--bit-offset", "999999", "--trace-segments"]);
                }
                "bit-parse-error" => {
                    cmd.args(["--bit-offset", "999999"]);
                }
                "candidate-error" => {
                    cmd.args([
                        "--section-item",
                        "0",
                        "--section-context",
                        "--candidate-evidence-records",
                    ])
                    .arg(&missing);
                }
                "compare-missing" | "compare-invalid" | "compare-empty" => {
                    let comparison = match name {
                        "compare-missing" => &missing,
                        "compare-invalid" => &invalid,
                        _ => &empty,
                    };
                    cmd.args(["--section-item", "0", "--section-context", "--compare-file"])
                        .arg(comparison);
                }
                "bounded-selected" => {
                    cmd.args([
                        "--section-item",
                        "0",
                        "--trace-segments",
                        "--reparse-limit-bits",
                        "74",
                    ]);
                }
                "selected-trace" | "selected-trace-failure" => {
                    cmd.args(["--section-item", "0", "--trace-segments"]);
                }
                _ => {}
            }
            let output = cmd
                .arg(match name {
                    "raw-item" => &raw,
                    "raw-single" => &raw_single,
                    "selected-trace-failure" => &trace_failure,
                    _ => &fixture,
                })
                .output()
                .expect("run inspector");
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let file =
                fs::read(&dest).unwrap_or_else(|error| panic!("{name}: missing report: {error}"));
            assert_eq!(file, output.stdout, "{name}, token={efficient}");
            let value: serde_json::Value = serde_json::from_slice(&file).expect("JSON report");
            if name == "selected-trace-failure" {
                assert_eq!(value["raw_len_bits"], 2);
                assert!(value["parser_consumed_bits"].is_null());
            }
            if name == "raw-single" {
                assert!(value["item"].is_object());
                assert!(value["errors"].as_array().unwrap().is_empty());
            } else if name != "bounded-selected" && name != "selected-trace" {
                assert!(
                    !value["errors"].as_array().expect("error list").is_empty(),
                    "{name}"
                );
            }
        }
    }
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
    assert_eq!(fs::read(&raw).expect("raw fixture after"), raw_before);
}

#[test]
fn legacy_inspector_option_errors_reach_destination() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/original/amazon_initial.d2s");
    let before = fs::read(&fixture).expect("fixture");
    let f = fixture.to_str().expect("fixture path");
    let cases = vec![
        (
            "selector-conflict",
            vec![
                "--marker-section",
                "1",
                "--marker-ordinal",
                "0",
                "--section-item",
                "0",
            ],
            0,
        ),
        ("preview-requires", vec!["--marker-header-preview"], 0),
        ("bad-reparse", vec!["--reparse-limit-bits", "x"], 0),
        (
            "bad-marker-reparse",
            vec!["--marker-reparse-limit-bits", "x"],
            0,
        ),
        (
            "candidate-requires",
            vec!["--candidate-evidence-records", "unused.json"],
            0,
        ),
        (
            "candidate-conflict",
            vec![
                "--candidate-evidence-records",
                "unused.json",
                "--section-context",
                "--section-item",
                "0",
                "--compare-file",
                f,
            ],
            0,
        ),
        ("compact-requires", vec!["--compact-transition-probe"], 1),
        (
            "compact-conflict",
            vec![
                "--compact-transition-probe",
                "--section-context",
                "--section-item",
                "0",
                "--compare-file",
                f,
            ],
            1,
        ),
        ("replay-requires", vec!["--recorded-parser-replay"], 1),
        (
            "replay-conflict",
            vec![
                "--recorded-parser-replay",
                "--section-context",
                "--section-item",
                "0",
                "--compare-file",
                f,
            ],
            1,
        ),
        ("body-requires", vec!["--recorded-body-probe"], 1),
        (
            "body-conflict",
            vec![
                "--recorded-body-probe",
                "--section-context",
                "--section-item",
                "0",
                "--compare-file",
                f,
            ],
            1,
        ),
        ("dual-requires", vec!["--dual-limit-body-probe"], 1),
        (
            "dual-conflict",
            vec![
                "--dual-limit-body-probe",
                "--section-context",
                "--section-item",
                "0",
                "--compare-file",
                f,
            ],
            1,
        ),
        (
            "marker-limit-requires",
            vec!["--marker-reparse-limit-bits", "74"],
            0,
        ),
        (
            "marker-limit-conflict",
            vec![
                "--marker-section",
                "1",
                "--marker-ordinal",
                "0",
                "--marker-reparse-limit-bits",
                "74",
                "--reparse-limit-bits",
                "74",
            ],
            0,
        ),
        (
            "preview-conflict",
            vec![
                "--marker-section",
                "1",
                "--marker-ordinal",
                "0",
                "--marker-header-preview",
                "--marker-reparse-limit-bits",
                "74",
            ],
            0,
        ),
        ("reparse-requires", vec!["--reparse-limit-bits", "74"], 0),
    ];
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for (name, flags, expected) in cases {
        for efficient in [false, true] {
            let dest = std::env::temp_dir().join(format!(
                "inspector-option-{}-{nonce}-{name}-{efficient}.json",
                std::process::id()
            ));
            assert!(!dest.exists());
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_d2item_inspect"));
            cmd.args(["--dialect", "legacy", "--json", "--output"])
                .arg(&dest);
            if efficient {
                cmd.arg("--token-efficient");
            }
            let result = cmd
                .args(flags.iter())
                .arg(&fixture)
                .output()
                .expect("run inspector");
            assert_eq!(result.status.code(), Some(expected), "{name}");
            let file =
                fs::read(&dest).unwrap_or_else(|error| panic!("{name}: missing report: {error}"));
            assert_eq!(file, result.stdout, "{name}, token={efficient}");
            let value: serde_json::Value = serde_json::from_slice(&file).expect("JSON error");
            assert!(
                !value["errors"].as_array().expect("error list").is_empty(),
                "{name}"
            );
            fs::remove_file(&dest).expect("remove owned test output");
        }
    }
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
}

#[test]
fn legacy_inspector_json_reports_reach_explicit_destination() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/original/amazon_initial.d2s");
    let before = fs::read(&fixture).expect("fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for name in ["all", "comparison", "marker", "offset", "witness"] {
        for efficient in [false, true] {
            let dest = std::env::temp_dir().join(format!(
                "inspector-route-{}-{nonce}-{name}-{efficient}.json",
                std::process::id()
            ));
            assert!(!dest.exists());
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_d2item_inspect"));
            cmd.args(["--dialect", "legacy", "--json", "--output"])
                .arg(&dest);
            if efficient {
                cmd.arg("--token-efficient");
            }
            match name {
                "comparison" => {
                    cmd.args(["--section-item", "0", "--section-context", "--compare-file"])
                        .arg(&fixture);
                }
                "marker" => {
                    cmd.args(["--marker-section", "1", "--marker-ordinal", "0"]);
                }
                "offset" => {
                    cmd.args(["--bit-offset", "7256"]);
                }
                "witness" => {
                    cmd.arg("--section-segment-witnesses");
                }
                _ => {}
            }
            let result = cmd.arg(&fixture).output().expect("run inspector");
            assert!(
                result.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let file =
                fs::read(&dest).unwrap_or_else(|error| panic!("{name}: missing report: {error}"));
            assert_eq!(file, result.stdout, "{name}, token={efficient}");
            let _: serde_json::Value = serde_json::from_slice(&file).expect("JSON report");
            fs::remove_file(&dest).expect("remove owned test output");
        }
    }
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
}

#[test]
fn legacy_inspector_variant_and_error_reports_reach_destination() {
    struct OwnedDir(PathBuf);
    impl Drop for OwnedDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = core.join("tests/fixtures/savegames/original/amazon_initial.d2s");
    let before = fs::read(&fixture).expect("fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = OwnedDir(
        core.join("tmp")
            .join(format!("inspector-variant-{}-{nonce}", std::process::id())),
    );
    fs::create_dir(&work.0).expect("create unique test directory");
    let directory = work.0.join("fixtures");
    fs::create_dir(&directory).expect("create fixture directory");
    fs::copy(&fixture, directory.join("input.d2s")).expect("copy fixture");
    for name in [
        "marker-reparse",
        "bit-trace",
        "coordinate",
        "witness-dir",
        "corpus",
        "invalid-index",
        "invalid-marker",
        "invalid-pair",
        "missing",
    ] {
        for efficient in [false, true] {
            let dest = work.0.join(format!("{name}-{efficient}.json"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_d2item_inspect"));
            cmd.args(["--dialect", "legacy", "--json", "--output"])
                .arg(&dest);
            if efficient {
                cmd.arg("--token-efficient");
            }
            match name {
                "marker-reparse" => {
                    cmd.args([
                        "--marker-section",
                        "1",
                        "--marker-ordinal",
                        "0",
                        "--marker-reparse-limit-bits",
                        "74",
                    ]);
                }
                "bit-trace" => {
                    cmd.args(["--bit-offset", "7256", "--trace-segments"]);
                }
                "coordinate" => {
                    cmd.args(["--coordinate-bit", "7256"]);
                }
                "witness-dir" => {
                    cmd.arg("--section-segment-witnesses");
                }
                "corpus" => {
                    cmd.arg("--corpus-dir").arg(&directory);
                }
                "invalid-index" => {
                    cmd.args(["--section-item", "999"]);
                }
                "invalid-marker" => {
                    cmd.args(["--marker-section", "999", "--marker-ordinal", "0"]);
                }
                "invalid-pair" => {
                    cmd.args(["--marker-section", "1"]);
                }
                _ => {}
            }
            if name == "witness-dir" {
                cmd.arg(&directory);
            } else if name == "missing" {
                cmd.arg(work.0.join("missing.d2s"));
            } else if name != "corpus" {
                cmd.arg(&fixture);
            }
            let output = cmd.output().expect("run inspector");
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let file =
                fs::read(&dest).unwrap_or_else(|error| panic!("{name}: missing report: {error}"));
            assert_eq!(file, output.stdout, "{name}, token={efficient}");
            let value: serde_json::Value = serde_json::from_slice(&file).expect("JSON report");
            if ["invalid-index", "invalid-marker", "invalid-pair", "missing"].contains(&name) {
                assert!(
                    !value["errors"].as_array().expect("error list").is_empty(),
                    "{name}"
                );
            }
        }
    }
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
}

#[test]
fn serialization_visual_directory_destination_is_an_error() {
    let fixture = fixture();
    let before = fs::read(&fixture).expect("fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_d2item_serialization_audit"))
        .arg("--visual")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg(&fixture)
        .output()
        .expect("run serialization audit");
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Visual diff report generated"));
    let blocked_parent = fixture.join("report.html");
    let output = Command::new(env!("CARGO_BIN_EXE_d2item_serialization_audit"))
        .arg("--visual")
        .arg(&blocked_parent)
        .arg(&fixture)
        .output()
        .expect("run serialization audit with invalid parent");
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Visual diff report generated"));
    assert!(!blocked_parent.exists());
    assert_eq!(fs::read(&fixture).expect("fixture after"), before);
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
