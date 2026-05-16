use d2r_core::domain::header::entity::calculate_alpha_v105_checksum;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, SymmetryOptions};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct FuzzResult {
    bit_offset: usize,
    fidelity: f32,
    is_match: bool,
    mismatch_offset: Option<u64>,
    drift_bits: Option<i64>,
    is_swallowed: bool,
    error: Option<String>,
}

fn get_all_item_markers(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> Vec<u64> {
    let mut all_markers = Vec::new();
    let jm_positions = d2r_core::save::find_jm_markers(bytes);
    for &pos in &jm_positions {
        if bytes.len() < pos + 4 {
            continue;
        }
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        if count == 0 {
            continue;
        }
        let next_pos = jm_positions
            .iter()
            .find(|&&p| p > pos)
            .cloned()
            .unwrap_or(bytes.len());
        let section_bytes = &bytes[pos + 4..next_pos];
        let section_bit_offset = (pos as u64 + 4) * 8;
        let markers = d2r_core::domain::item::scanner::scan_item_markers(section_bytes, huffman, alpha, section_bit_offset, None, false);
        for m in markers {
            all_markers.push((pos as u64 + 4) * 8 + m.offset);
        }
    }
    all_markers.sort();
    all_markers
}

fn main() {
    let mut parser = ArgParser::new("d2item_property_fuzzer");
    parser.add_spec(ArgSpec::positional("fixture", "Path to save file"));
    parser.add_spec(ArgSpec::option(
        "target",
        Some('t'),
        Some("target"),
        "Index of target item (0-based)",
    ));
    parser.add_spec(ArgSpec::option(
        "bit-range",
        Some('r'),
        Some("bit-range"),
        "Range of bits to flip (start..end)",
    ));
    parser.add_spec(ArgSpec::flag(
        "force-save-failed",
        None,
        Some("force-save-failed"),
        "Save even if parsing fails",
    ));
    parser.add_spec(ArgSpec::flag(
        "parse-verify",
        Some('v'),
        Some("parse-verify"),
        "Verify if mutated item can still be parsed",
    ));
    parser.add_spec(ArgSpec::flag(
        "detect-drift",
        Some('d'),
        Some("detect-drift"),
        "Detect bitstream drift and item swallowing",
    ));
    parser.add_spec(ArgSpec::option(
        "output-json",
        Some('o'),
        Some("output-json"),
        "Export results to JSON file",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let file_path = parsed.get("fixture").unwrap();
    let target_idx: usize = parsed
        .get("target")
        .map(|v| v.as_str())
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let bit_range_str = parsed.get("bit-range").map(|v| v.as_str()).unwrap_or("0..1");
    let parse_verify = parsed.is_set("parse-verify");
    let detect_drift = parsed.is_set("detect-drift");
    let output_json = parsed.get("output-json").cloned();

    let (bit_start, bit_end) = parse_range(bit_range_str).unwrap_or((0, 1));

    let original_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            std::process::exit(1);
        }
    };

    let huffman = HuffmanTree::new();
    let is_alpha_mode = is_alpha(&original_bytes);
    let original_items = Item::read_player_items(&original_bytes, &huffman, is_alpha_mode).unwrap_or_default();
    
    if target_idx >= original_items.len() {
        eprintln!(
            "[ERROR] Target item index {} out of range (found {} items).",
            target_idx,
            original_items.len()
        );
        std::process::exit(1);
    }

    let target_item = &original_items[target_idx];
    let item_bit_offset = target_item.range.start as usize;

    println!("--- Alpha v105 Item Property Poke-Test Fuzzer ---");
    println!("Fixture: {}", file_path);
    println!(
        "Target Item: #{} ({} at bit {})",
        target_idx, target_item.code.trim(), item_bit_offset
    );
    println!("Fuzzing Bit Range: {}..{}", bit_start, bit_end);

    let out_dir = Path::new("tmp/fuzz_outputs");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir).unwrap();
    }

    let original_markers = get_all_item_markers(&original_bytes, &huffman, is_alpha_mode);
    let mut results = Vec::new();

    for bit_offset in bit_start..bit_end {
        let mut mutated = original_bytes.clone();

        // Flip the bit
        let absolute_bit = item_bit_offset + bit_offset;
        let byte_idx = absolute_bit / 8;
        let bit_in_byte = absolute_bit % 8;

        if byte_idx >= mutated.len() {
            continue;
        }

        mutated[byte_idx] ^= 1 << bit_in_byte;

        // Re-calculate checksum
        let flags = read_u32_at_bit(&mutated, item_bit_offset);
        let version = read_u8_at_bit(&mutated, item_bit_offset + 40, 3);
        let new_checksum = calculate_alpha_v105_checksum(flags, version);
        write_u8_at_bit(&mut mutated, item_bit_offset + 32, 8, new_checksum);

        let mut fidelity = 0.0;
        let mut mismatch_info = String::new();
        let mut drift_info = String::new();
        let mut is_match = false;
        let mut mismatch_offset = None;
        let mut drift_bits = None;
        let mut is_swallowed = false;
        let error_msg = None;

        if parse_verify {
            let options = SymmetryOptions {
                roundtrip: true,
                target_index: Some(target_idx),
                fail_fast: false,
            };

            if let Ok(report) = calculate_symmetry_diff(&mutated, None, options) {
                if let Some(item_diff) = report.items.iter().find(|i| i.label == format!("Item {}", target_idx)) {
                    fidelity = item_diff.fidelity_score * 100.0;
                    is_match = item_diff.is_match;
                    if !item_diff.is_match {
                        mismatch_offset = item_diff.first_mismatch_offset;
                        mismatch_info = format!(" | MISMATCH@{}", mismatch_offset.unwrap_or(0));
                    }
                }
            }
        }

        if detect_drift {
            let mutated_markers = get_all_item_markers(&mutated, &huffman, is_alpha_mode);
            if mutated_markers.len() < original_markers.len() {
                is_swallowed = true;
                drift_info = format!(" | SWALLOWED={}", original_markers.len() - mutated_markers.len());
            } else if target_idx + 1 < original_markers.len() && target_idx + 1 < mutated_markers.len() {
                let orig_next = original_markers[target_idx + 1];
                let mut_next = mutated_markers[target_idx + 1];
                if orig_next != mut_next {
                    let diff = (mut_next as i64) - (orig_next as i64);
                    drift_bits = Some(diff);
                    drift_info = format!(" | DRIFT={}", diff);
                }
            }
        }

        let status = if parse_verify {
            if is_match { "PASS" } else { "FAIL" }
        } else {
            "MUTATED"
        };

        println!("  Bit {:>4}: FIDELITY={:>5.1}% [{}] {}{}", bit_offset, fidelity, status, mismatch_info, drift_info);

        results.push(FuzzResult {
            bit_offset,
            fidelity,
            is_match,
            mismatch_offset,
            drift_bits,
            is_swallowed,
            error: error_msg,
        });

        if !parse_verify || parsed.is_set("force-save-failed") || is_match {
            let out_name = format!("fuzz_item{}_bit{}.d2s", target_idx, bit_offset);
            fs::write(out_dir.join(out_name), &mutated).unwrap();
        }
    }

    if let Some(json_path) = output_json {
        let json_path = resolve_output_json_path(&json_path);
        let json_str = serde_json::to_string_pretty(&results).unwrap();
        if let Some(parent) = json_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&json_path, json_str).unwrap();
        println!("Results exported to {}", json_path.display());
    }

    println!("Fuzzing complete. Results in tmp/fuzz_outputs/");
}

fn is_alpha(bytes: &[u8]) -> bool {
    if bytes.len() < 8 { return false; }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    version == 105 || version == 6
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        let start = parts[0].parse().ok()?;
        let end = parts[1].parse().ok()?;
        Some((start, end))
    } else {
        None
    }
}

fn resolve_output_json_path(raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() || p.parent().is_some_and(|parent| parent != Path::new("")) {
        return p.to_path_buf();
    }

    if let Ok(spec_root) = env::var("D2R_SPEC_PATH") {
        return Path::new(&spec_root)
            .join("research")
            .join("forensics")
            .join(raw);
    }

    p.to_path_buf()
}

fn read_u32_at_bit(bytes: &[u8], bit_offset: usize) -> u32 {
    let mut val: u32 = 0;
    for i in 0..32 {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (bytes[byte_idx] & (1 << bit_in_byte)) != 0 {
                val |= 1 << i;
            }
        }
    }
    val
}

fn read_u8_at_bit(bytes: &[u8], bit_offset: usize, count: usize) -> u8 {
    let mut val: u8 = 0;
    for i in 0..count {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (bytes[byte_idx] & (1 << bit_in_byte)) != 0 {
                val |= 1 << i;
            }
        }
    }
    val
}

fn write_u8_at_bit(bytes: &mut [u8], bit_offset: usize, count: usize, val: u8) {
    for i in 0..count {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (val & (1 << i)) != 0 {
                bytes[byte_idx] |= 1 << bit_in_byte;
            } else {
                bytes[byte_idx] &= !(1 << bit_in_byte);
            }
        }
    }
}
