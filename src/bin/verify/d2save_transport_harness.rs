// Verification harness tool for in-memory Alpha v105 transport section injection.
// Evaluates all declared oracle targets in memory without writing save files to disk.

use anyhow::{bail, Context, Result};
use d2r_core::engine::checksum::recalculate_checksum;
use d2r_core::save::transport::{
    inject_section1, TransportInjectError, TransportOracleEntry,
};
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct OracleRoot {
    scan_results: Option<ScanResults>,
}

#[derive(Debug, Deserialize)]
struct ScanResults {
    multi_insertion_simulation: Option<MultiInsertionSimulation>,
}

#[derive(Debug, Deserialize)]
struct MultiInsertionSimulation {
    targets: Option<Vec<OracleTarget>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleTarget {
    base_file: String,
    target_file: String,
    simulation_status: String,
    base_payload_bits: usize,
    target_payload_bits: usize,
    projected_delta_bits: isize,
    base_next_jm_bit_offset: usize,
    projected_next_jm_bit_offset: usize,
}

#[derive(Debug, Serialize)]
struct TargetResultRow {
    target_file: String,
    base_file: String,
    projected_delta_bits: isize,
    simulation_status: String,
    expected_class: String,
    result_class: String,
    passed: bool,
    injected_bits: Option<usize>,
    projected_next_jm_bit_offset: Option<usize>,
    checksum_valid: Option<bool>,
    error_message: Option<String>,
}

#[derive(Debug, Serialize)]
struct BatchHarnessReport {
    oracle_path: String,
    total_targets: usize,
    successful_injections: usize,
    rejected_injections: usize,
    all_passed: bool,
    results: Vec<TargetResultRow>,
}

fn load_file_bytes(path_str: &str) -> std::io::Result<Vec<u8>> {
    let path = Path::new(path_str);
    if path.exists() {
        return fs::read(path);
    }
    let alt_path = Path::new("d2r-core").join(path_str);
    if alt_path.exists() {
        return fs::read(alt_path);
    }
    fs::read(path)
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2save_transport_harness").description(
        "In-memory batch harness for testing Section 1 transport injection across oracle targets",
    );

    parser
        .add_opt("oracle", "Path to oracle simulation JSON file")
        .short('o')
        .long("oracle")
        .default(r"..\d2r-spec\agent_artifacts\2026-07-21-3378\multi_insertion_simulation.json");

    parser
        .add_opt("output", "Path to write JSON summary report")
        .short('u')
        .long("output");

    let parsed = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let oracle_path_str = parsed
        .get("oracle")
        .cloned()
        .unwrap_or_else(|| r"..\d2r-spec\agent_artifacts\2026-07-21-3378\multi_insertion_simulation.json".to_string());

    let output_path_str = parsed.get("output").cloned();

    let oracle_bytes = load_file_bytes(&oracle_path_str)
        .with_context(|| format!("Failed to read oracle file at '{oracle_path_str}'"))?;

    let root: OracleRoot = serde_json::from_slice(&oracle_bytes)
        .with_context(|| format!("Failed to parse JSON from '{oracle_path_str}'"))?;

    let targets = root
        .scan_results
        .and_then(|sr| sr.multi_insertion_simulation)
        .and_then(|mis| mis.targets)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow::anyhow!("No targets found at scan_results.multi_insertion_simulation.targets in '{oracle_path_str}'"))?;

    let mut report_rows = Vec::with_capacity(targets.len());
    let mut successful_injections = 0usize;
    let mut rejected_injections = 0usize;

    for target in &targets {
        let expected_class = if target.projected_delta_bits > 0 {
            "success"
        } else {
            "rejected_ineligible_delta"
        };

        let entry = TransportOracleEntry {
            target_file: target.target_file.clone(),
            simulation_status: target.simulation_status.clone(),
            base_payload_bits: target.base_payload_bits,
            target_payload_bits: target.target_payload_bits,
            projected_delta_bits: target.projected_delta_bits,
            base_next_jm_bit_offset: target.base_next_jm_bit_offset,
            projected_next_jm_bit_offset: target.projected_next_jm_bit_offset,
        };

        let base_bytes = match load_file_bytes(&target.base_file) {
            Ok(b) => b,
            Err(e) => {
                report_rows.push(TargetResultRow {
                    target_file: target.target_file.clone(),
                    base_file: target.base_file.clone(),
                    projected_delta_bits: target.projected_delta_bits,
                    simulation_status: target.simulation_status.clone(),
                    expected_class: expected_class.to_string(),
                    result_class: "read_base_failed".to_string(),
                    passed: false,
                    injected_bits: None,
                    projected_next_jm_bit_offset: None,
                    checksum_valid: None,
                    error_message: Some(format!("Failed to read base file: {e}")),
                });
                continue;
            }
        };

        let target_bytes = match load_file_bytes(&target.target_file) {
            Ok(b) => b,
            Err(e) => {
                report_rows.push(TargetResultRow {
                    target_file: target.target_file.clone(),
                    base_file: target.base_file.clone(),
                    projected_delta_bits: target.projected_delta_bits,
                    simulation_status: target.simulation_status.clone(),
                    expected_class: expected_class.to_string(),
                    result_class: "read_target_failed".to_string(),
                    passed: false,
                    injected_bits: None,
                    projected_next_jm_bit_offset: None,
                    checksum_valid: None,
                    error_message: Some(format!("Failed to read target file: {e}")),
                });
                continue;
            }
        };

        match inject_section1(&base_bytes, &target_bytes, &entry) {
            Ok(res) => {
                let stored_checksum = if res.bytes.len() >= 16 {
                    u32::from_le_bytes(res.bytes[12..16].try_into().unwrap())
                } else {
                    0
                };
                let calc_checksum = recalculate_checksum(&res.bytes).unwrap_or(0);
                let checksum_valid = stored_checksum == calc_checksum;

                let row_passed = (expected_class == "success")
                    && checksum_valid
                    && (res.injected_bits == target.target_payload_bits)
                    && (res.projected_next_jm_bit_offset == target.projected_next_jm_bit_offset);

                if row_passed {
                    successful_injections += 1;
                }

                report_rows.push(TargetResultRow {
                    target_file: target.target_file.clone(),
                    base_file: target.base_file.clone(),
                    projected_delta_bits: target.projected_delta_bits,
                    simulation_status: target.simulation_status.clone(),
                    expected_class: expected_class.to_string(),
                    result_class: "success".to_string(),
                    passed: row_passed,
                    injected_bits: Some(res.injected_bits),
                    projected_next_jm_bit_offset: Some(res.projected_next_jm_bit_offset),
                    checksum_valid: Some(checksum_valid),
                    error_message: None,
                });
            }
            Err(err) => {
                let result_class = match err {
                    TransportInjectError::IneligibleDelta { .. } => {
                        "rejected_ineligible_delta".to_string()
                    }
                    TransportInjectError::IneligibleStatus { .. } => {
                        "rejected_ineligible_status".to_string()
                    }
                    _ => format!("error_{err:?}"),
                };

                let row_passed = expected_class == result_class;
                if row_passed {
                    rejected_injections += 1;
                }

                report_rows.push(TargetResultRow {
                    target_file: target.target_file.clone(),
                    base_file: target.base_file.clone(),
                    projected_delta_bits: target.projected_delta_bits,
                    simulation_status: target.simulation_status.clone(),
                    expected_class: expected_class.to_string(),
                    result_class,
                    passed: row_passed,
                    injected_bits: None,
                    projected_next_jm_bit_offset: None,
                    checksum_valid: None,
                    error_message: Some(err.to_string()),
                });
            }
        }
    }

    let total_targets = targets.len();
    let all_passed = (successful_injections + rejected_injections == total_targets)
        && report_rows.iter().all(|r| r.passed);

    let report = BatchHarnessReport {
        oracle_path: oracle_path_str,
        total_targets,
        successful_injections,
        rejected_injections,
        all_passed,
        results: report_rows,
    };

    let report_json = serde_json::to_string_pretty(&report)?;

    if let Some(out_path_str) = output_path_str {
        let out_path = PathBuf::from(&out_path_str);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, &report_json)?;
        println!("Batch harness report written to '{}'", out_path_str);
    } else {
        println!("{}", report_json);
    }

    println!(
        "Batch Harness Summary: {}/{} total targets passed ({} successful injections, {} typed rejections). All passed: {}",
        successful_injections + rejected_injections,
        total_targets,
        successful_injections,
        rejected_injections,
        all_passed
    );

    if !all_passed {
        bail!("Batch transport harness failed: not all targets passed expectations");
    }

    Ok(())
}

