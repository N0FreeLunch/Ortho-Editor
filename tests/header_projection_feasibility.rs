//! Report-only comparison of independent header candidates with fully parsed items.
//! Harness success means a complete report, never authoritative placement parity.

mod common;

use d2r_core::domain::inventory::InventoryGrid;
use d2r_core::domain::item::scanner::{MarkerStatus, scan_item_markers_with_receipt};
use d2r_core::domain::item::serialization::{
    is_likely_jm_section_header, peek_item_header_at, peek_item_header_at_specific_gap,
};
use d2r_core::item::{HuffmanTree, Item};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Serialize)]
struct HeaderFields {
    mode: u8,
    location: u8,
    x: u8,
    code: String,
    flags: u32,
    version: u8,
    compact: bool,
}

#[derive(Debug, Serialize)]
struct Candidate {
    section_marker_byte_offset: u64,
    absolute_start_bit: u64,
    marker_code: String,
    status: MarkerStatus,
    selected_trial_gap: Option<u64>,
    selected_corrected_gap: Option<i8>,
    fields: Option<HeaderFields>,
}

#[derive(Debug, Serialize)]
struct Scan {
    section_count: usize,
    declared_item_count: usize,
    probes: u64,
    lookahead_checks: u64,
    timed_out: bool,
    candidates: Vec<Candidate>,
}

// No full-parser values, offsets, or body fields enter this path.
fn scan(bytes: &[u8], huffman: &HuffmanTree) -> Scan {
    let sections: Vec<_> = d2r_core::save::find_jm_markers(bytes)
        .into_iter()
        .filter(|&pos| is_likely_jm_section_header(bytes, pos, true, huffman))
        .collect();
    let mut output = Scan {
        section_count: sections.len(),
        declared_item_count: 0,
        probes: 0,
        lookahead_checks: 0,
        timed_out: false,
        candidates: Vec::new(),
    };
    for (i, &pos) in sections.iter().enumerate() {
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        output.declared_item_count += usize::from(count);
        if count == 0 {
            continue;
        }
        let end = sections.get(i + 1).copied().unwrap_or(bytes.len());
        let section = &bytes[pos..end];
        let receipt = scan_item_markers_with_receipt(
            section,
            huffman,
            true,
            pos as u64 * 8,
            Some(count),
            false,
        );
        output.probes += receipt.work.probes;
        output.lookahead_checks += receipt.work.lookahead_checks;
        output.timed_out |= receipt.timed_out;
        for (ordinal, marker) in receipt.markers.into_iter().enumerate() {
            let peek = match marker.selected_trial_gap {
                Some(gap) => {
                    peek_item_header_at_specific_gap(section, marker.offset, huffman, true, gap)
                }
                None => peek_item_header_at(section, marker.offset, huffman, true, ordinal),
            };
            let fields = peek.map(
                |(mode, location, x, code, flags, version, compact, _, _, _)| HeaderFields {
                    mode,
                    location,
                    x,
                    code,
                    flags,
                    version,
                    compact,
                },
            );
            output.candidates.push(Candidate {
                section_marker_byte_offset: pos as u64,
                absolute_start_bit: pos as u64 * 8 + marker.offset,
                marker_code: marker.code,
                status: marker.status,
                selected_trial_gap: marker.selected_trial_gap,
                selected_corrected_gap: marker.selected_corrected_gap,
                fields,
            });
        }
    }
    output
}

fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn compare(path: &Path, bytes: &[u8], scan: &Scan, full: &[Item]) -> Value {
    let mut candidate_keys = BTreeMap::<u64, Vec<&Candidate>>::new();
    for c in &scan.candidates {
        candidate_keys
            .entry(c.absolute_start_bit)
            .or_default()
            .push(c);
    }
    let mut full_keys = BTreeMap::<u64, Vec<&Item>>::new();
    for item in full {
        full_keys.entry(item.range.start).or_default().push(item);
    }
    let missing: Vec<_> = full_keys
        .keys()
        .filter(|key| !candidate_keys.contains_key(key))
        .copied()
        .collect();
    let extra: Vec<_> = candidate_keys
        .keys()
        .filter(|key| !full_keys.contains_key(key))
        .copied()
        .collect();
    let candidate_duplicates: Vec<_> = candidate_keys
        .iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(key, rows)| json!({"start": key, "count": rows.len()}))
        .collect();
    let full_duplicates: Vec<_> = full_keys
        .iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(key, rows)| json!({"start": key, "count": rows.len()}))
        .collect();
    let mut differences = Vec::new();
    let mut matched = 0;
    for (start, rows) in &full_keys {
        if let Some(candidates) = candidate_keys.get(start) {
            if rows.len() != 1 || candidates.len() != 1 {
                continue;
            }
            if let Some(header) = &candidates[0].fields {
                matched += 1;
                let item = rows[0];
                let baseline = json!({"mode": item.header.mode, "location": item.header.location,
                    "x": item.header.x, "code": item.code.trim(), "flags": item.header.flags,
                    "version": item.header.version, "compact": item.is_compact});
                let candidate = json!({"mode": header.mode, "location": header.location,
                    "x": header.x, "code": header.code.trim(), "flags": header.flags,
                    "version": header.version, "compact": header.compact});
                if baseline != candidate {
                    differences.push(
                        json!({"start": start, "baseline": baseline, "candidate": candidate}),
                    );
                }
            }
        }
    }
    let candidate_order: Vec<_> = scan
        .candidates
        .iter()
        .map(|c| c.absolute_start_bit)
        .collect();
    let full_order: Vec<_> = full.iter().map(|item| item.range.start).collect();
    json!({
        "fixture": path.to_string_lossy(), "bytes": bytes.len(), "fnv1a64": digest(bytes),
        "baseline_items": full.len(), "baseline_residues": full.iter().filter(|item| item.is_residue()).count(),
        "baseline_direct_socket_children": full.iter().map(|item| item.socketed_items.len()).sum::<usize>(),
        "candidate_items": scan.candidates.len(), "unique_matched_with_header": matched,
        "missing_full_start_bits": missing, "extra_candidate_start_bits": extra,
        "duplicate_candidate_keys": candidate_duplicates, "duplicate_full_keys": full_duplicates,
        "known_field_differences": differences, "same_ordered_identities": candidate_order == full_order,
        "failed_header_peeks": scan.candidates.iter().filter(|c| c.fields.is_none()).count(),
        "unresolved_candidate_count": scan.candidates.len(),
        "unresolved_reasons": ["y/page unavailable from public peek", "body-dependent membership and skip not established", "final item ownership and span unverified"],
        "membership_drift": null, "validator_result_drift": null,
        "authoritative_projection": false, "decision": "NO_GO_AUTHORITATIVE_PROJECTION",
        "baseline_results": {
            "cube": format!("{:?}", InventoryGrid::validate_cube_integrity(full)),
            "stash": format!("{:?}", InventoryGrid::validate_stash_integrity(full)),
            "inventory": format!("{:?}", InventoryGrid::validate_inventory_integrity(full)),
            "all": format!("{:?}", InventoryGrid::validate_all_containers_integrity(full)),
        },
        "scan": scan,
    })
}

fn run(paths: Vec<PathBuf>, report_name: &str, expected_files: usize) {
    assert_eq!(paths.len(), expected_files);
    let huffman = HuffmanTree::new();
    let inputs: Vec<_> = paths
        .iter()
        .map(|path| fs::read(path).expect("Fixture read failed"))
        .collect();
    d2r_core::init_rayon_thread_pool();
    let scan_start = Instant::now();
    let scans: Vec<_> = inputs
        .par_iter()
        .map(|bytes| scan(bytes, &huffman))
        .collect();
    let scan_ms = scan_start.elapsed().as_secs_f64() * 1000.0;
    let parse_start = Instant::now();
    let full: Vec<_> = inputs
        .par_iter()
        .map(|bytes| Item::read_player_items(bytes, &huffman, true).expect("Baseline parse failed"))
        .collect();
    let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;
    let compare_start = Instant::now();
    let rows: Vec<_> = paths
        .iter()
        .zip(&inputs)
        .zip(&scans)
        .zip(&full)
        .map(|(((path, bytes), scan), full)| {
            assert!(scan.section_count > 0, "No sections in {}", path.display());
            assert!(!scan.timed_out, "Scanner timed out");
            assert!(!full.is_empty(), "Empty baseline in {}", path.display());
            compare(path, bytes, scan, full)
        })
        .collect();
    let report = json!({
        "schema_version": 1, "file_count": rows.len(), "mode": "alpha-v105",
        "decision": "NO_GO_AUTHORITATIVE_PROJECTION", "authoritative_projection": false,
        "timing": {"scan_wall_ms": scan_ms, "full_parse_wall_ms": parse_ms,
            "comparison_wall_ms": compare_start.elapsed().as_secs_f64() * 1000.0,
            "rayon_threads": rayon::current_num_threads(), "warmup_runs": 0, "measurement_runs": 1,
            "label": "diagnostic single run; excludes file IO, tree setup, build; not a speedup benchmark"},
        "full_parser_calls_in_scan": 0, "baseline_full_parser_calls": paths.len(),
        "digest_note": "FNV-1a 64-bit non-cryptographic identifier; both phases share identical immutable byte buffers",
        "files": rows,
    });
    let report_dir = PathBuf::from(
        std::env::var_os("D2R_PROJECTION_REPORT_DIR").expect("Set D2R_PROJECTION_REPORT_DIR"),
    );
    assert!(report_dir.is_absolute() && report_dir.is_dir());
    let output = report_dir.join(report_name);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("Report already exists or cannot be created");
    serde_json::to_writer_pretty(file, &report).expect("Report serialization failed");
    let readback: Value =
        serde_json::from_slice(&fs::read(&output).expect("Readback failed")).expect("Invalid JSON");
    assert_eq!(readback, report);
    println!(
        "{}: {} files, scan {:.2}ms, full {:.2}ms, NO_GO_AUTHORITATIVE_PROJECTION (not a parity PASS)",
        output.display(),
        paths.len(),
        scan_ms,
        parse_ms
    );
}

#[d2r_macros::hypothesis_test]
#[ignore = "Requires an explicit report directory; diagnostic only"]
fn candidate_samples() {
    run(
        vec![
            common::repo_path("tests/fixtures/savegames/original/amazon_10_scrolls.d2s"),
            common::repo_path(
                "tests/fixtures/savegames/gameplay/normal/act2/TESTDRUID_Q2_CubeFoundNoCainTalk.d2s",
            ),
        ],
        "candidate-samples.json",
        2,
    );
}

#[d2r_macros::radar_test]
fn candidate_corpus() {
    fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("Corpus directory missing") {
            let path = entry.expect("Corpus entry failed").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().and_then(|s| s.to_str()) == Some("d2s") {
                files.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    collect(
        &common::repo_path("tests/fixtures/savegames/gameplay/normal"),
        &mut paths,
    );
    paths.sort();
    run(paths, "candidate-corpus.json", 87);
}
