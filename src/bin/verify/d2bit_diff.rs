// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{env, fs, process};

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffCategory {
    #[serde(rename = "Intended")]
    Intended,
    #[serde(rename = "Expected Collateral")]
    ExpectedCollateral,
    #[serde(rename = "Unintended Corruption")]
    UnintendedCorruption,
}

#[derive(Debug, Serialize)]
pub struct BitDiff {
    pub bit_offset: usize,
    pub byte_offset: usize,
    pub bit_in_byte: usize,
    pub original_value: u8,
    pub mutated_value: u8,
    pub category: DiffCategory,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiffReport {
    pub original_file: String,
    pub mutated_file: String,
    pub total_diffs: usize,
    pub categories: std::collections::HashMap<DiffCategory, usize>,
    pub diffs: Vec<BitDiff>,
}

// SBA Baseline Structures
#[derive(Debug, Deserialize)]
struct SbaRange {
    start: usize,
    end: usize,
}

#[derive(Debug, Deserialize)]
struct SbaSegment {
    start: usize,
    end: usize,
    label: String,
}

#[derive(Debug, Deserialize)]
struct SbaItem {
    path: String,
    code: String,
    range: SbaRange,
    segments: Vec<SbaSegment>,
}

#[derive(Debug, Deserialize)]
struct SbaBaseline {
    items: Vec<SbaItem>,
}

fn main() {
    let mut parser = ArgParser::new("d2bit_diff")
        .description("Mutation-Aware Bit-Diff Auditor (MABA) for D2R save files");

    parser.add_spec(ArgSpec::positional(
        "original_d2s",
        "path to the original D2R save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "mutated_d2s",
        "path to the mutated D2R save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::repeated_positional(
        "intended_bits",
        "optional explicit intended bit offsets",
    ));
    parser.add_spec(ArgSpec::option(
        "sba-baseline",
        None,
        Some("sba-baseline"),
        "path to SBA baseline JSON",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}\n\n{}", e, parser.usage());
            process::exit(1);
        }
    };

    let original_path = Path::new(parsed.get("original_d2s").unwrap());
    let mutated_path = Path::new(parsed.get("mutated_d2s").unwrap());
    let is_json = parsed.is_json();
    let sba_path = parsed.get("sba-baseline");

    let intended_bits: Vec<usize> = parsed
        .get_vec("intended_bits")
        .unwrap_or(&vec![])
        .iter()
        .map(|s| {
            s.parse::<usize>()
                .expect("intended_bits must be positive integers")
        })
        .collect();

    // Load SBA baseline if provided
    let sba_baseline: Option<SbaBaseline> = sba_path.map(|path| {
        let content = fs::read_to_string(path).expect("Could not read SBA baseline file");
        serde_json::from_str(&content).expect("Could not parse SBA baseline JSON")
    });

    if !original_path.exists() {
        eprintln!("Error: Original file '{:?}' does not exist.", original_path);
        process::exit(1);
    }
    if !mutated_path.exists() {
        eprintln!("Error: Mutated file '{:?}' does not exist.", mutated_path);
        process::exit(1);
    }

    let original_bytes = fs::read(original_path).expect("Could not read original file");
    let mutated_bytes = fs::read(mutated_path).expect("Could not read mutated file");

    let max_len = original_bytes.len().max(mutated_bytes.len());
    let mut diffs = Vec::new();

    for i in 0..max_len {
        let orig = original_bytes.get(i).copied().unwrap_or(0);
        let muta = mutated_bytes.get(i).copied().unwrap_or(0);

        if orig != muta {
            for b in 0..8 {
                let orig_bit = (orig >> b) & 1;
                let muta_bit = (muta >> b) & 1;

                if orig_bit != muta_bit {
                    let bit_offset = i * 8 + b;
                    let mut category = DiffCategory::UnintendedCorruption;
                    let mut reason = None;

                    if intended_bits.contains(&bit_offset) {
                        category = DiffCategory::Intended;
                    } else if (64..96).contains(&bit_offset) {
                        category = DiffCategory::ExpectedCollateral;
                        reason = Some("File Size Header".to_string());
                    } else if (96..128).contains(&bit_offset) {
                        category = DiffCategory::ExpectedCollateral;
                        reason = Some("Checksum Header".to_string());
                    } else if let Some(baseline) = &sba_baseline {
                        // Check if it falls within an SBA item range
                        for item in &baseline.items {
                            if (item.range.start..item.range.end).contains(&bit_offset) {
                                category = DiffCategory::Intended; // Baseline-derived is trusted in Phase 1
                                let rel_bit = bit_offset - item.range.start;
                                let mut segment_label = "Unknown Segment".to_string();
                                for seg in &item.segments {
                                    if (seg.start..seg.end).contains(&rel_bit) {
                                        segment_label = seg.label.clone();
                                        break;
                                    }
                                }
                                reason = Some(format!("SbaItem[{}]: {}", item.path, segment_label));
                                break;
                            }
                        }
                    }

                    diffs.push(BitDiff {
                        bit_offset,
                        byte_offset: i,
                        bit_in_byte: b,
                        original_value: orig_bit,
                        mutated_value: muta_bit,
                        category,
                        reason,
                    });
                }
            }
        }
    }

    let mut categories_count = std::collections::HashMap::new();
    for diff in &diffs {
        *categories_count.entry(diff.category).or_insert(0) += 1;
    }

    let report = DiffReport {
        original_file: original_path.to_string_lossy().to_string(),
        mutated_file: mutated_path.to_string_lossy().to_string(),
        total_diffs: diffs.len(),
        categories: categories_count,
        diffs,
    };

    if is_json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("MABA Diff Report");
        println!("Original: {:?}", original_path);
        println!("Mutated:  {:?}", mutated_path);
        if let Some(path) = sba_path {
            println!("Baseline: {:?}", path);
        }
        println!("--------------------------------------------------");
        println!("Total differences (bits): {}", report.total_diffs);

        let unintended = report
            .categories
            .get(&DiffCategory::UnintendedCorruption)
            .copied()
            .unwrap_or(0);
        let expected = report
            .categories
            .get(&DiffCategory::ExpectedCollateral)
            .copied()
            .unwrap_or(0);
        let intended = report
            .categories
            .get(&DiffCategory::Intended)
            .copied()
            .unwrap_or(0);

        println!("- Intended:            {}", intended);
        println!("- Expected Collateral: {}", expected);
        println!("- Unintended Corruption: {}", unintended);
        println!("--------------------------------------------------");

        if report.total_diffs > 0 {
            println!(
                "{:<10} {:<10} {:<5} {:<5} -> {:<5} {:<25} {}",
                "Bit", "Byte", "InB", "Orig", "Muta", "Category", "Reason"
            );
            for diff in &report.diffs {
                let category_str = match diff.category {
                    DiffCategory::Intended => "Intended",
                    DiffCategory::ExpectedCollateral => "Expected Collateral",
                    DiffCategory::UnintendedCorruption => "Unintended Corruption",
                };
                println!(
                    "{:<10} {:<10} {:<5} {:<5} -> {:<5} {:<25} {}",
                    diff.bit_offset,
                    diff.byte_offset,
                    diff.bit_in_byte,
                    diff.original_value,
                    diff.mutated_value,
                    category_str,
                    diff.reason.as_deref().unwrap_or("")
                );
            }
        }
    }

    let unintended = report
        .categories
        .get(&DiffCategory::UnintendedCorruption)
        .copied()
        .unwrap_or(0);
    if unintended > 0 {
        process::exit(1);
    } else {
        process::exit(0);
    }
}
