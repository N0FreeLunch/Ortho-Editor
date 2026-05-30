// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::Save;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::mutation::{
    MutationMode, mutate_absolute_bit_and_finalize, resolve_logical_address,
};
use d2r_core::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};
use std::path::Path;
use std::{env, fs};

#[derive(serde::Serialize)]
struct MutationDiagnostic {
    bit_offset: usize,
    dest_path: String,
    parse_check: String,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("bsmt_mutate")
        .description("Safely mutates D2R save files in an isolated environment (tmp/forensics/) for testing and exploration");

    parser.add_spec(ArgSpec::positional(
        "save_file",
        "path to the D2R save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "target",
        "bit offset (absolute) or logical address (e.g. Header.Checksum)",
    ));
    parser.add_spec(
        ArgSpec::option(
            "mode",
            Some('m'),
            Some("mode"),
            "mutation mode: 'absolute' (default) or 'logical'",
        )
        .with_default("absolute"),
    );

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let is_json = parsed.is_json();
    let input_path_str = parsed.get("save_file").unwrap().trim_matches('"');
    let input_path = Path::new(input_path_str);
    let target = parsed.get("target").unwrap();
    let mode_str = parsed.get("mode").unwrap();

    let mode = match mode_str.as_str() {
        "absolute" => MutationMode::Absolute,
        "logical" => MutationMode::Logical,
        _ => anyhow::bail!("Unknown mode '{}'. Use 'absolute' or 'logical'.", mode_str),
    };

    let bit_offset = resolve_bit_offset(target, mode)?;

    if !input_path.exists() {
        anyhow::bail!("Input file '{}' does not exist.", input_path_str);
    }

    // 1. Isolation: Create tmp/forensics/ if it doesn't exist
    let forensics_dir = Path::new("tmp/forensics");
    fs::create_dir_all(forensics_dir)?;

    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let dest_filename = format!("{}_mutated_{}.d2s", file_stem, bit_offset);
    let dest_path = forensics_dir.join(dest_filename);
    let dest_path_str = dest_path.to_string_lossy().to_string();

    if !is_json {
        println!("Isolated to: {:?}", dest_path);
    }

    // 2. Mutation
    let original_bytes = fs::read(input_path)?;

    if !is_json {
        println!("Mutation applied at bit offset: {}", bit_offset);
    }

    let mutated_bytes = match mutate_absolute_bit_and_finalize(&original_bytes, bit_offset) {
        Ok(bytes) => bytes,
        Err(e) => anyhow::bail!("Error during mutation/finalization: {}", e),
    };

    // 3. Validation and Diagnostic Reporting
    let issue = run_diagnostic_logic(&mutated_bytes)?;
    let parse_check = if issue.is_some() { "fail" } else { "ok" };

    if is_json {
        let status = if issue.is_some() {
            ReportStatus::Fail
        } else {
            ReportStatus::Ok
        };
        let metadata = ReportMetadata::new("bsmt_mutate", input_path_str, "Alpha v105 | Legacy");
        let results = MutationDiagnostic {
            bit_offset,
            dest_path: dest_path_str.clone(),
            parse_check: parse_check.to_string(),
        };
        let mut report = Report::new(metadata, status).with_results(results);
        if let Some(i) = issue {
            report = report.with_issues(vec![i]);
        }
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if let Some(i) = &issue {
            println!("\n--- Diagnostic Report ---");
            println!("{}", i.message);
            println!("-------------------------\n");
        } else {
            println!("Parse check: ok");
        }
    }

    if let Err(e) = fs::write(&dest_path, &mutated_bytes) {
        anyhow::bail!("Error writing mutated file: {}", e);
    }

    if !is_json {
        println!("Finalized and saved to: {}", dest_path_str);
    }

    Ok(())
}

fn resolve_bit_offset(target: &str, mode: MutationMode) -> anyhow::Result<usize> {
    match mode {
        MutationMode::Absolute => target
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("target must be a positive integer in absolute mode.")),
        MutationMode::Logical => resolve_logical_address(target)
            .map_err(|e| anyhow::anyhow!("Error resolving logical address: {}", e)),
    }
}

fn run_diagnostic_logic(mutated_bytes: &[u8]) -> anyhow::Result<Option<ReportIssue>> {
    match Save::from_bytes(mutated_bytes) {
        Ok(save) => {
            let huffman = HuffmanTree::new();
            // Alpha v105 detection
            let is_alpha = save.header.version == 6 || save.header.version == 105;

            match Item::read_player_items(mutated_bytes, &huffman, is_alpha) {
                Ok(items) => {
                    // Find JM #0 to get the expected count from the header
                    if let Some(jm_pos) = mutated_bytes.windows(2).position(|w| w == b"JM") {
                        let expected_count = u16::from_le_bytes([
                            mutated_bytes[jm_pos + 2],
                            mutated_bytes[jm_pos + 3],
                        ]) as usize;

                        if items.len() != expected_count {
                            return Ok(Some(ReportIssue {
                                kind: "count_mismatch".to_string(),
                                message: format!(
                                    "[Bit {}] [Rel +16] [read_player_items] Item count mismatch: expected {}, found {}",
                                    jm_pos * 8 + 16,
                                    expected_count,
                                    items.len()
                                ),
                                bit_offset: Some((jm_pos * 8 + 16) as u64),
                            }));
                        }
                    }
                }
                Err(e) => {
                    return Ok(Some(ReportIssue {
                        kind: "item_parse_error".to_string(),
                        message: format!("{}", e),
                        bit_offset: None,
                    }));
                }
            }
        }
        Err(e) => {
            return Ok(Some(ReportIssue {
                kind: "save_validation_error".to_string(),
                message: format!("Basic save validation failed: {}", e),
                bit_offset: None,
            }));
        }
    }

    Ok(None)
}
