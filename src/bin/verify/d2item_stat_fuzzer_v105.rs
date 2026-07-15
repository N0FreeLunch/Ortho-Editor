use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::domain::item::serialization::HuffmanTree;
use bitstream_io::{BitReader, LittleEndian};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
struct FuzzReport {
    offset: usize,
    target_code: Option<String>,
    results: Vec<FuzzResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FuzzResult {
    stat_id: u32,
    candidates: Vec<Candidate>,
    winner: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Candidate {
    bits: u32,
    audit_exit_code: i32,
    #[serde(rename = "match")]
    is_match: bool,
    fidelity_score: f32,
}

fn get_item_code(orig_bits_str: &str) -> String {
    let mut bytes = Vec::new();
    let mut current_byte = 0u8;
    let mut bit_count = 0;
    for c in orig_bits_str.chars() {
        if c == '1' {
            current_byte |= 1 << bit_count;
        }
        bit_count += 1;
        if bit_count == 8 {
            bytes.push(current_byte);
            current_byte = 0;
            bit_count = 0;
        }
    }
    if bit_count > 0 {
        bytes.push(current_byte);
    }

    // Try ASCII first (Alpha v105 summary/runeword codes like xrs, mp1, etc.)
    let mut ascii_code = String::new();
    if bytes.len() >= 3 {
        for i in 0..3 {
            ascii_code.push(bytes[i] as char);
        }
    }
    let trimmed = ascii_code.trim();
    if trimmed == "xrs" || trimmed == "mp1" || d2r_core::domain::forensic::v105::axioms::is_v105_summary_code(&ascii_code) {
        return ascii_code;
    }

    // Try Huffman
    let huff = HuffmanTree::new();
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let mut decoded = String::new();
    for _ in 0..4 {
        if let Ok(c) = huff.decode(&mut reader) {
            decoded.push(c);
        } else {
            break;
        }
    }
    decoded
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_stat_fuzzer_v105");
    parser.add_spec(ArgSpec::option(
        "fixture",
        Some('f'),
        Some("fixture"),
        "Path to the save game fixture",
    ));
    parser.add_spec(ArgSpec::option(
        "stat",
        Some('s'),
        Some("stat"),
        "Stat ID to fuzz",
    ));
    parser.add_spec(ArgSpec::option(
        "stats",
        None,
        Some("stats"),
        "Comma-separated list of Stat IDs to fuzz",
    ));
    parser.add_spec(ArgSpec::option(
        "range",
        Some('r'),
        Some("range"),
        "Bit width range (e.g. 8..32)",
    ));
    parser.add_spec(ArgSpec::option(
        "item-index",
        Some('i'),
        Some("item-index"),
        "Target specific item index",
    ));
    parser.add_spec(ArgSpec::option(
        "offset",
        Some('o'),
        Some("offset"),
        "Header gap offset in bits",
    ));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixture_path = parsed.get("fixture").expect("Missing --fixture");
    let range_str = parsed.get("range").map(|s| s.as_str()).unwrap_or("1..32");
    let item_index = parsed.get("item-index");
    let offset_val: usize = parsed.get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let use_json = parsed.is_set("json");

    let mut stat_ids = Vec::new();
    if let Some(s) = parsed.get("stats") {
        for id_str in s.split(',') {
            if let Ok(id) = id_str.parse::<u32>() {
                stat_ids.push(id);
            }
        }
    } else if let Some(s) = parsed.get("stat") {
        if let Ok(id) = s.parse::<u32>() {
            stat_ids.push(id);
        }
    }

    let range: Vec<u32> = if let Some((start_str, end_str)) = range_str.split_once("..") {
        let start: u32 = start_str.parse().expect("Invalid range start");
        let end: u32 = end_str.parse().expect("Invalid range end");
        (start..=end).collect()
    } else {
        vec![range_str.parse().expect("Invalid range value")]
    };

    let bytes = fs::read(fixture_path)?;

    // Pre-flight check to resolve actual item code if targeting an item
    let mut target_code = None;
    if let Some(idx_str) = item_index {
        let target_idx: usize = idx_str.parse().expect("Invalid item index");
        let options = d2r_core::verify::symmetry::SymmetryOptions {
            roundtrip: true,
            target_index: Some(target_idx),
            fail_fast: false,
        };
        if let Ok(report) = d2r_core::verify::symmetry::calculate_symmetry_diff(&bytes, None, options) {
            if let Some(item_diff) = report.items.iter().find(|it| {
                it.label.strip_prefix("Item ")
                    .and_then(|s| s.parse::<usize>().ok()) == Some(target_idx)
            }) {
                let code = if item_diff.code == "Opaque" {
                    if let Some(ref bits) = item_diff.orig_bits {
                        get_item_code(bits)
                    } else {
                        item_diff.code.clone()
                    }
                } else {
                    item_diff.code.clone()
                };
                if !code.trim().is_empty() {
                    target_code = Some(code);
                }
            }
        }
    }

    if !use_json {
        println!(
            "Fuzzing stats {:?} over range {:?} using fixture {}, offset {}, target_code {:?}",
            stat_ids, range, fixture_path, offset_val, target_code
        );
    }

    // Locate original registry
    let base_data_path = env::var("D2R_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../d2r-data"));
    let original_registry_path = base_data_path.join("constants/alpha_v105_forensics.json");
    let original_content = fs::read_to_string(&original_registry_path)?;
    let registry_json: Value = serde_json::from_str(&original_content)?;

    // Find pre-built audit executable
    let exe_path = env::current_exe()
        .ok()
        .map(|p| p.parent().unwrap().join("d2item_serialization_audit.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("../d2r-core/target/debug/d2item_serialization_audit.exe"));

    let mut fuzz_results = Vec::new();

    for &stat_id in &stat_ids {
        if !use_json {
            println!("Starting fuzz sweep for stat ID: {}", stat_id);
        }
        let mut results = FuzzResult {
            stat_id,
            candidates: Vec::new(),
            winner: None,
        };

        for &width in &range {
            if !use_json {
                print!("  Width {}... ", width);
                std::io::Write::flush(&mut std::io::stdout())?;
            }

            // Patch registry
            let mut temp_registry = registry_json.clone();
            patch_registry(&mut temp_registry, stat_id, width);
            if let Some(ref code) = target_code {
                patch_header_gap(&mut temp_registry, code, offset_val);
            }

            // Create temp data dir
            let temp_dir = PathBuf::from("tmp").join(format!("fuzz_stat_{}_{}", stat_id, width));
            let temp_constants_dir = temp_dir.join("constants");
            fs::create_dir_all(&temp_constants_dir)?;
            let temp_json_path = temp_constants_dir.join("alpha_v105_forensics.json");
            fs::write(
                &temp_json_path,
                serde_json::to_string_pretty(&temp_registry)?,
            )?;

            // Run audit directly using pre-built binary
            let mut cmd = Command::new(&exe_path);
            cmd.arg(fixture_path)
                .arg("--json");

            if let Some(idx) = item_index {
                cmd.arg("--target").arg(idx);
            }

            cmd.env("D2R_DATA_PATH", &temp_dir);

            let output = cmd.output()?;
            let exit_code = output.status.code().unwrap_or(-1);

            let mut is_match = false;
            let mut fidelity = 0.0;

            if output.status.success() || !output.stdout.is_empty() {
                if let Ok(report) = serde_json::from_slice::<Value>(&output.stdout) {
                    is_match = report["success"].as_bool().unwrap_or(false);

                    // If targeting an item, check if THAT item matches
                    if item_index.is_some() {
                        if let Some(items) = report["items"].as_array() {
                            if let Some(item) = items.first() {
                                is_match = item["is_match"].as_bool().unwrap_or(false);
                                fidelity = item["fidelity_score"].as_f64().unwrap_or(0.0) as f32;
                            }
                        }
                    }
                }
            }

            if !use_json {
                println!("Match: {}, Fidelity: {}", is_match, fidelity);
            }

            results.candidates.push(Candidate {
                bits: width,
                audit_exit_code: exit_code,
                is_match,
                fidelity_score: fidelity,
            });

            // Cleanup temp dir
            let _ = fs::remove_dir_all(&temp_dir);

            // Accept only results with fidelity_score = 1.0 as a winner
            if is_match && (fidelity - 1.0).abs() < f32::EPSILON {
                results.winner = Some(width);
                break;
            }
        }
        fuzz_results.push(results);
    }

    let report = FuzzReport {
        offset: offset_val,
        target_code,
        results: fuzz_results,
    };

    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}

fn patch_registry(registry: &mut Value, stat_id: u32, width: u32) {
    let stat_id_str = stat_id.to_string();

    // 1. Try to patch mappings first if it exists
    if let Some(mappings) = registry["mappings"].as_object_mut() {
        if let Some(mapping) = mappings.get_mut(&stat_id_str) {
            mapping["save_bits"] = Value::from(width);
            return;
        }
    }

    // 2. Fallback to stats
    if let Some(stats) = registry["stats"].as_object_mut() {
        if let Some(stat) = stats.get_mut(&stat_id_str) {
            stat["width"] = Value::from(width);
            return;
        }
    }

    // 3. If neither exists, add to stats as a new entry
    if let Some(stats) = registry["stats"].as_object_mut() {
        let mut new_stat = serde_json::Map::new();
        new_stat.insert(
            "name".to_string(),
            Value::from(format!("fuzzed_stat_{}", stat_id)),
        );
        new_stat.insert("width".to_string(), Value::from(width));
        stats.insert(stat_id_str, Value::Object(new_stat));
    }
}

fn patch_header_gap(registry: &mut Value, code: &str, gap: usize) {
    if let Some(item_overrides) = registry["item_overrides"].as_object_mut() {
        if let Some(item) = item_overrides.get_mut(code) {
            item["header_gap"] = Value::from(gap);
            item["is_compact"] = Value::from(1);
        } else {
            let mut new_override = serde_json::Map::new();
            new_override.insert("header_gap".to_string(), Value::from(gap));
            new_override.insert("is_compact".to_string(), Value::from(1));
            item_overrides.insert(code.to_string(), Value::Object(new_override));
        }
    }
}
