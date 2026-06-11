use anyhow::{Context, Result};
use d2r_core::item::{HuffmanTree, is_plausible_item_header, peek_item_header_at_specific_gap};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct MarkerProbeCandidate {
    anchor_offset_bits: u64,
    candidate_offset_bits: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_offset_bits: Option<u64>,
    gap_bits: u64,
    header_bits: u64,
    mode: String,
    location: String,
    header_code: String,
    flags: u32,
    version: u8,
    nudge_bits: i8,
    compact: bool,
    has_checksum: bool,
    plausibility: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct MarkerProbeReport {
    file: String,
    anchor_offset_bits: u64,
    alpha_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sweep_start_offset_bits: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sweep_end_offset_bits: Option<u64>,
    candidate_count: usize,
    plausible_count: usize,
    verdict: String,
    candidates: Vec<MarkerProbeCandidate>,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_marker_probe")
        .description("Single-offset forensic probe for item marker plausibility");

    parser.add_spec(
        ArgSpec::option("file", Some('f'), Some("file"), "Path to D2S save file").required(),
    );
    parser.add_spec(
        ArgSpec::option("offset", Some('o'), Some("offset"), "Bit offset to probe").required(),
    );
    parser.add_spec(ArgSpec::flag(
        "alpha",
        Some('a'),
        Some("alpha"),
        "Enable Alpha v105 mode",
    ));
    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let file_path = parsed.get("file").unwrap();
    let offset_str = parsed.get("offset").unwrap();
    let offset: u64 = offset_str
        .parse()
        .with_context(|| format!("Invalid bit offset: {}", offset_str))?;
    let alpha_mode = parsed.is_set("alpha");
    let use_json = parsed.is_json();

    let bytes =
        fs::read(file_path).with_context(|| format!("Failed to read file: {}", file_path))?;
    let huffman = HuffmanTree::new();
    let report = if use_json {
        build_sweep_report(file_path, offset, alpha_mode, &bytes, &huffman)
    } else {
        build_legacy_report(file_path, offset, alpha_mode, &bytes, &huffman)
    };

    if use_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_legacy_report(&report);
    }

    Ok(())
}

fn build_legacy_report(
    file_path: &str,
    offset: u64,
    alpha_mode: bool,
    bytes: &[u8],
    huffman: &HuffmanTree,
) -> MarkerProbeReport {
    let mut candidates = Vec::new();

    for gap in 0..64_u64 {
        if let Some((
            mode,
            location,
            _x,
            code,
            flags,
            version,
            is_compact,
            header_bits,
            nudge_val,
            has_checksum,
        )) = peek_item_header_at_specific_gap(bytes, offset, huffman, alpha_mode, gap)
        {
            let plausibility = is_plausible_item_header(
                mode,
                location,
                code.as_bytes(),
                flags,
                version,
                alpha_mode,
            );

            candidates.push(MarkerProbeCandidate {
                anchor_offset_bits: offset,
                candidate_offset_bits: offset.saturating_add(header_bits),
                body_offset_bits: Some(offset.saturating_add(header_bits)),
                gap_bits: gap,
                header_bits,
                mode: mode_name(mode).to_string(),
                location: location_name(location).to_string(),
                header_code: code.trim().to_string(),
                flags,
                version,
                nudge_bits: nudge_val,
                compact: is_compact,
                has_checksum,
                plausibility,
            });
        }
    }

    let plausible_count = candidates
        .iter()
        .filter(|candidate| candidate.plausibility)
        .count();
    let verdict = if plausible_count > 0 {
        "plausible_candidates_found"
    } else {
        "no_plausible_item_at_offset"
    }
    .to_string();

    MarkerProbeReport {
        file: file_path.to_string(),
        anchor_offset_bits: offset,
        alpha_mode,
        sweep_start_offset_bits: None,
        sweep_end_offset_bits: None,
        candidate_count: candidates.len(),
        plausible_count,
        verdict,
        candidates,
    }
}

fn build_sweep_report(
    file_path: &str,
    anchor_offset: u64,
    alpha_mode: bool,
    bytes: &[u8],
    huffman: &HuffmanTree,
) -> MarkerProbeReport {
    let mut candidates = Vec::new();
    let sweep_start = anchor_offset.saturating_sub(64);
    let sweep_end = anchor_offset.saturating_add(64);

    for candidate_offset in sweep_start..=sweep_end {
        for gap in 0..64_u64 {
            if let Some((
                mode,
                location,
                _x,
                code,
                flags,
                version,
                is_compact,
                header_bits,
                nudge_val,
                has_checksum,
            )) =
                peek_item_header_at_specific_gap(bytes, candidate_offset, huffman, alpha_mode, gap)
            {
                let plausibility = is_plausible_item_header(
                    mode,
                    location,
                    code.as_bytes(),
                    flags,
                    version,
                    alpha_mode,
                );

                candidates.push(MarkerProbeCandidate {
                    anchor_offset_bits: anchor_offset,
                    candidate_offset_bits: candidate_offset,
                    body_offset_bits: Some(candidate_offset.saturating_add(header_bits)),
                    gap_bits: gap,
                    header_bits,
                    mode: mode_name(mode).to_string(),
                    location: location_name(location).to_string(),
                    header_code: code.trim().to_string(),
                    flags,
                    version,
                    nudge_bits: nudge_val,
                    compact: is_compact,
                    has_checksum,
                    plausibility,
                });
            }
        }
    }

    let plausible_count = candidates
        .iter()
        .filter(|candidate| candidate.plausibility)
        .count();
    let verdict = if plausible_count > 0 {
        "plausible_candidates_found"
    } else {
        "no_plausible_item_in_sweep_window"
    }
    .to_string();

    MarkerProbeReport {
        file: file_path.to_string(),
        anchor_offset_bits: anchor_offset,
        alpha_mode,
        sweep_start_offset_bits: Some(sweep_start),
        sweep_end_offset_bits: Some(sweep_end),
        candidate_count: candidates.len(),
        plausible_count,
        verdict,
        candidates,
    }
}

fn print_legacy_report(report: &MarkerProbeReport) {
    println!("Probing file: {}", report.file);
    println!("Bit offset: {}", report.anchor_offset_bits);
    println!("Alpha mode: {}", report.alpha_mode);
    println!("{:-<40}", "");

    let mut found = false;
    for candidate in report
        .candidates
        .iter()
        .filter(|candidate| candidate.plausibility)
    {
        println!(
            "Candidate at bit {} (Gap {}):",
            candidate.candidate_offset_bits, candidate.gap_bits
        );
        println!("  Flags:    0x{:08X}", candidate.flags);
        println!("  Version:  {}", candidate.version);
        println!("  Code:     '{}'", candidate.header_code);
        println!("  Nudge:    {}", candidate.nudge_bits);
        println!("{:-<20}", "");
        found = true;
    }

    if !found {
        println!("Verdict: [EXTRACTION FAILURE / NO PLAUSIBLE ITEM AT OFFSET]");
    }
}

fn mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "Stored",
        1 => "Equipped",
        2 => "Belt",
        4 => "Cursor",
        6 => "Socketed",
        _ => "Unknown",
    }
}

fn location_name(loc: u8) -> &'static str {
    match loc {
        0 => "None",
        1 => "Inventory",
        4 => "Stash",
        5 => "Cube",
        _ => "Other",
    }
}
