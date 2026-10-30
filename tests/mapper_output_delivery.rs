use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn json_reports_reach_the_selected_output_in_all_modes() {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = core.join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    let before = fs::read(&fixture).expect("read fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = std::env::temp_dir().join(format!("mapper-delivery-{}-{nonce}", std::process::id()));
    fs::create_dir(&work).expect("create unique test directory");
    let mut failures = Vec::new();
    for flag in ["--list-anchors", "--auto-map"] {
        for explicit in [false, true] {
            for token in [false, true] {
                let folder = work.join(format!("{}-{explicit}-{token}", &flag[2..]));
                fs::create_dir(&folder).expect("create case directory");
                let target = folder.join("report.json");
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_d2item_oracle_mapper"));
                cmd.arg(&fixture)
                    .args([flag, "--json"])
                    .env("AGENT_ARTIFACTS_PATH", &folder);
                if token {
                    cmd.arg("--token-efficient");
                }
                if explicit {
                    cmd.arg("--output").arg(&target);
                }
                let result = cmd.output().expect("run mapper");
                let files: Vec<_> = fs::read_dir(&folder)
                    .expect("list case directory")
                    .map(|e| e.unwrap().path())
                    .collect();
                let report = if explicit {
                    Some(target.clone())
                } else if token && files.len() == 1 {
                    Some(files[0].clone())
                } else {
                    None
                };
                let report_json = report
                    .as_ref()
                    .and_then(|path| fs::read(path).ok())
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                let stdout_json = serde_json::from_slice::<serde_json::Value>(&result.stdout).ok();
                let value_ok = |value: &serde_json::Value| {
                    value["file"] == "amazon_authority_runeword.d2s"
                        && value["scan_results"]
                            .as_array()
                            .is_some_and(|rows| !rows.is_empty())
                };
                let file_ok = if explicit || token {
                    report_json.as_ref().is_some_and(value_ok)
                } else {
                    files.is_empty()
                };
                let stdout_ok = if token {
                    result.stdout.is_empty()
                } else {
                    stdout_json.as_ref().is_some_and(value_ok)
                };
                let channels_equal = if !token && explicit {
                    stdout_json == report_json
                } else {
                    true
                };
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stderr_ok =
                    stderr.ends_with("Scan complete.\n") && !stderr.contains("scan_results");
                if !result.status.success()
                    || !file_ok
                    || !stdout_ok
                    || !stderr_ok
                    || !channels_equal
                {
                    failures.push(format!("{flag}, explicit={explicit}, token={token}: exit={}, file={file_ok}, stdout={stdout_ok}, stderr={stderr_ok}, same={channels_equal}", result.status));
                }
                for file in files {
                    fs::remove_file(file).expect("remove owned report");
                }
                fs::remove_dir(&folder).expect("remove owned case directory");
            }
        }
    }
    fs::remove_dir(&work).expect("remove owned test directory");
    assert_eq!(fs::read(&fixture).expect("read fixture after"), before);
    assert!(
        failures.is_empty(),
        "{} failing routes: {failures:#?}",
        failures.len()
    );
}
