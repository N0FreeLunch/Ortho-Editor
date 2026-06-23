use anyhow::{Context, Result, anyhow};
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::map_core_sections;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct PrefixTraceReport {
    file: String,
    version: u32,
    alpha_mode: bool,
    total_jm_sections: usize,
    traced_sections: usize,
    total_items: usize,
    local_match_count: usize,
    sections_with_divergence: usize,
    verdict: String,
    sections: Vec<SectionTrace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct SectionTrace {
    section_index: usize,
    jm_offset_bit: u64,
    next_jm_offset_bit: u64,
    section_bit_offset: u64,
    section_header_bits: u64,
    payload_start_bit: u64,
    payload_end_bit: u64,
    payload_original_len_bits: usize,
    payload_serialized_len_bits: usize,
    top_level_count: usize,
    parsed_item_count: usize,
    local_match_count: usize,
    prefix_match: bool,
    matching_prefix_bits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_matching_prefix_bit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_matching_prefix_byte: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_divergence: Option<PrefixDivergence>,
    items: Vec<ItemTrace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ItemTrace {
    item_index: usize,
    code: String,
    raw_start_bit: u64,
    raw_end_bit: u64,
    raw_len_bits: usize,
    serialized_len_bits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_check: Option<BitCheck>,
    prefix_checkpoint: PrefixCheckpoint,
    socketed_child_count: usize,
    emitted_child_count: usize,
    child_emission_skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct BitCheck {
    matches: bool,
    expected_bits: usize,
    actual_bits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_mismatch: Option<BitMismatch>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct PrefixCheckpoint {
    emitted_bits: usize,
    compared_bits: usize,
    matches: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_mismatch: Option<BitMismatch>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct BitMismatch {
    kind: String,
    bit_offset: u64,
    byte_offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_bit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_bit: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct PrefixDivergence {
    item_index: Option<usize>,
    item_code: Option<String>,
    mismatch: BitMismatch,
}

struct ItemEmission {
    bits: Vec<bool>,
    emitted_child_count: usize,
    child_emission_skipped: bool,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_section_prefix_trace");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));
    parser.add_spec(ArgSpec::flag(
        "alpha",
        Some('a'),
        Some("alpha"),
        "Force Alpha v105 mode",
    ));
    parser.add_spec(ArgSpec::flag(
        "verbose",
        Some('v'),
        Some("verbose"),
        "Emit per-item checkpoint rows",
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

    let mut out = OutputManager::new("d2item_section_prefix_trace", &parsed);
    let path = parsed.get("save_file").unwrap();
    let bytes = fs::read(path).with_context(|| format!("Failed to read file: {}", path))?;
    if bytes.len() < 8 {
        return Err(anyhow!("File too small to read version header: {}", path));
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let alpha_mode = parsed.is_set("alpha") || version == 105 || version == 6;
    let huffman = HuffmanTree::new();
    let section_map = map_core_sections(&bytes)
        .with_context(|| format!("Failed to map JM sections in {}", path))?;
    let jm_positions = section_map.jm_positions;

    let payload = build_prefix_trace_report(
        path,
        version,
        alpha_mode,
        &bytes,
        &jm_positions,
        &huffman,
    )?;

    let has_divergence = payload.sections.iter().any(|section| section.first_divergence.is_some());
    let status = if has_divergence {
        ReportStatus::Warn
    } else {
        ReportStatus::Ok
    };

    let report = Report::new(
        ReportMetadata::new("d2item_section_prefix_trace", path, env!("CARGO_PKG_VERSION")),
        status,
    )
    .with_results(payload);

    if parsed.is_json() {
        out.json(&serde_json::to_string_pretty(&report)?);
    } else {
        print_text_report(&report, parsed.is_set("verbose"), &mut out);
    }

    Ok(())
}

fn build_prefix_trace_report(
    file: &str,
    version: u32,
    alpha_mode: bool,
    bytes: &[u8],
    jm_positions: &[usize],
    huffman: &HuffmanTree,
) -> Result<PrefixTraceReport> {
    let mut sections = Vec::new();
    let mut total_items = 0usize;
    let mut local_match_count = 0usize;
    let mut sections_with_divergence = 0usize;

    for (section_index, &jm_offset) in jm_positions.iter().enumerate() {
        let next_jm_offset = jm_positions
            .get(section_index + 1)
            .copied()
            .unwrap_or(bytes.len());
        let section_bytes = &bytes[jm_offset..next_jm_offset];
        let section_bit_offset = (jm_offset as u64) * 8;
        let next_jm_offset_bit = (next_jm_offset as u64) * 8;
        let top_level_count = read_section_count(section_bytes);

        let items = Item::read_section(
            section_bytes,
            section_bit_offset,
            top_level_count,
            huffman,
            alpha_mode,
            false,
        )
        .with_context(|| {
            format!(
                "Failed to parse JM section {} at byte {}",
                section_index + 1,
                jm_offset
            )
        })?;

        let payload_start_bit = items
            .first()
            .map(|item| item.range.start)
            .unwrap_or(section_bit_offset + 32);
        let section_header_bits = payload_start_bit.saturating_sub(section_bit_offset);
        let payload_end_bit = next_jm_offset_bit;
        let original_payload_bits = bits_from_range(bytes, payload_start_bit, payload_end_bit);

        let mut emitted_prefix = Vec::new();
        let mut item_traces = Vec::new();
        let mut section_local_match_count = 0usize;
        let mut first_divergence: Option<PrefixDivergence> = None;

        for (item_index, item) in items.iter().enumerate() {
            let raw_bits = bits_from_range(bytes, item.range.start, item.range.end);
            let serialized_local = item
                .to_bits(item_index, huffman, alpha_mode)
                .with_context(|| {
                    format!(
                        "Failed to serialize item {} ({}) locally in section {}",
                        item_index,
                        item.code.trim(),
                        section_index + 1
                    )
                })?;

            let local_check = compare_exact_bits(&raw_bits, &serialized_local, item.range.start);
            if local_check.matches {
                section_local_match_count += 1;
            }

            let emission = emit_item_section_bits(item, item_index, huffman, alpha_mode)
                .with_context(|| {
                    format!(
                        "Failed to emit cumulative section bits for item {} ({}) in section {}",
                        item_index,
                        item.code.trim(),
                        section_index + 1
                    )
                })?;

            emitted_prefix.extend(emission.bits.iter().copied());

            let mut checkpoint_mismatch = None;
            if first_divergence.is_none() {
                if let Some(mismatch) =
                    compare_prefix_progress(&original_payload_bits, &emitted_prefix, payload_start_bit)
                {
                    checkpoint_mismatch = Some(mismatch.clone());
                    first_divergence = Some(PrefixDivergence {
                        item_index: Some(item_index),
                        item_code: Some(item.code.trim().to_string()),
                        mismatch,
                    });
                }
            }

            item_traces.push(ItemTrace {
                item_index,
                code: item.code.trim().to_string(),
                raw_start_bit: item.range.start,
                raw_end_bit: item.range.end,
                raw_len_bits: raw_bits.len(),
                serialized_len_bits: emission.bits.len(),
                local_check: Some(local_check),
                prefix_checkpoint: PrefixCheckpoint {
                    emitted_bits: emitted_prefix.len(),
                    compared_bits: emitted_prefix.len().min(original_payload_bits.len()),
                    matches: first_divergence.is_none(),
                    first_mismatch: checkpoint_mismatch,
                },
                socketed_child_count: item.socketed_items.len(),
                emitted_child_count: emission.emitted_child_count,
                child_emission_skipped: emission.child_emission_skipped,
            });

            total_items += 1;
            if item_traces
                .last()
                .and_then(|trace| trace.local_check.as_ref())
                .map(|check| check.matches)
                .unwrap_or(false)
            {
                local_match_count += 1;
            }
        }

        if first_divergence.is_none() && emitted_prefix.len() != original_payload_bits.len() {
            let mismatch_bit = payload_start_bit + emitted_prefix.len().min(original_payload_bits.len()) as u64;
            let (expected_bit, actual_bit) = if emitted_prefix.len() < original_payload_bits.len() {
                (
                    original_payload_bits.get(emitted_prefix.len()).copied(),
                    None,
                )
            } else {
                (None, emitted_prefix.get(original_payload_bits.len()).copied())
            };
            if let Some(last_item) = item_traces.last_mut() {
                last_item.prefix_checkpoint.matches = false;
                last_item.prefix_checkpoint.first_mismatch = Some(BitMismatch {
                    kind: "length".to_string(),
                    bit_offset: mismatch_bit,
                    byte_offset: mismatch_bit / 8,
                    expected_bit,
                    actual_bit,
                });
            }
            first_divergence = Some(PrefixDivergence {
                item_index: items.len().checked_sub(1),
                item_code: items.last().map(|item| item.code.trim().to_string()),
                mismatch: BitMismatch {
                    kind: "length".to_string(),
                    bit_offset: mismatch_bit,
                    byte_offset: mismatch_bit / 8,
                    expected_bit,
                    actual_bit,
                },
            });
        }

        let prefix_match = first_divergence.is_none();
        if first_divergence.is_some() {
            sections_with_divergence += 1;
        }

        let matching_prefix_bits = if let Some(divergence) = &first_divergence {
            divergence
                .mismatch
                .bit_offset
                .saturating_sub(payload_start_bit) as usize
        } else {
            emitted_prefix.len().min(original_payload_bits.len())
        };
        let last_matching_prefix_bit = if matching_prefix_bits == 0 {
            None
        } else {
            Some(payload_start_bit + matching_prefix_bits as u64 - 1)
        };
        let last_matching_prefix_byte = last_matching_prefix_bit.map(|bit| bit / 8);

        sections.push(SectionTrace {
            section_index: section_index + 1,
            jm_offset_bit: section_bit_offset,
            next_jm_offset_bit,
            section_bit_offset,
            section_header_bits,
            payload_start_bit,
            payload_end_bit,
            payload_original_len_bits: original_payload_bits.len(),
            payload_serialized_len_bits: emitted_prefix.len(),
            top_level_count: top_level_count as usize,
            parsed_item_count: item_traces.len(),
            local_match_count: section_local_match_count,
            prefix_match,
            matching_prefix_bits,
            last_matching_prefix_bit,
            last_matching_prefix_byte,
            first_divergence,
            items: item_traces,
        });
    }

    let verdict = if sections_with_divergence == 0 {
        "prefix_match".to_string()
    } else {
        "prefix_divergence_detected".to_string()
    };

    Ok(PrefixTraceReport {
        file: file.to_string(),
        version,
        alpha_mode,
        total_jm_sections: jm_positions.len(),
        traced_sections: sections.len(),
        total_items,
        local_match_count,
        sections_with_divergence,
        verdict,
        sections,
    })
}

fn print_text_report(report: &Report<PrefixTraceReport>, verbose: bool, out: &mut OutputManager) {
    let payload = report
        .scan_results
        .as_ref()
        .expect("prefix trace report payload missing");
    let status = match &report.status {
        ReportStatus::Ok => "ok",
        ReportStatus::Warn => "warn",
        ReportStatus::Fail => "fail",
    };

    out.summary(&format!("Prefix trace: {}", payload.file));
    out.summary(&format!(
        "Version: {} | Alpha mode: {} | JM sections: {} | Items: {}",
        payload.version, payload.alpha_mode, payload.total_jm_sections, payload.total_items
    ));
    out.summary(&format!(
        "Verdict: {} | Status: {} | Local matches: {}/{} | Divergent sections: {}",
        payload.verdict, status, payload.local_match_count, payload.total_items, payload.sections_with_divergence
    ));

    for section in &payload.sections {
        if let Some(divergence) = &section.first_divergence {
            let code = divergence
                .item_code
                .as_deref()
                .unwrap_or("<unknown>");
            out.summary(&format!(
                "Section {} @ JM byte {}: prefix diverges at bit {} (byte {}), item {} {}",
                section.section_index,
                section.jm_offset_bit / 8,
                divergence.mismatch.bit_offset,
                divergence.mismatch.byte_offset,
                divergence
                    .item_index
                    .map(|idx| idx.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                code
            ));
            out.summary(&format!(
                "  Matching prefix: {} bits | Local item matches: {}/{}",
                section.matching_prefix_bits, section.local_match_count, section.parsed_item_count
            ));
        } else {
            out.summary(&format!(
                "Section {} @ JM byte {}: full prefix match through {} bits",
                section.section_index,
                section.jm_offset_bit / 8,
                section.payload_original_len_bits
            ));
        }

        if verbose {
            out.summary("  idx | code       | raw range           | local | prefix | children");
            out.summary("  ----+------------+---------------------+-------+--------+---------");
            for item in &section.items {
                out.summary(&format!(
                    "  {:>3} | {:<10} | {:>8}..{:>8} | {:>5} | {:>6} | {:>3}/{}{}",
                    item.item_index,
                    truncate_code(&item.code, 10),
                    item.raw_start_bit,
                    item.raw_end_bit,
                    if item
                        .local_check
                        .as_ref()
                        .map(|check| check.matches)
                        .unwrap_or(false)
                    {
                        "ok"
                    } else {
                        "fail"
                    },
                    if item.prefix_checkpoint.matches { "ok" } else { "fail" },
                    item.emitted_child_count,
                    item.socketed_child_count,
                    if item.child_emission_skipped {
                        " skipped"
                    } else {
                        ""
                    }
                ));
            }
        }
    }
}

fn emit_item_section_bits(
    item: &Item,
    idx: usize,
    huffman: &HuffmanTree,
    alpha_mode: bool,
) -> Result<ItemEmission> {
    let mut bits = if alpha_mode {
        item.to_bits(idx, huffman, alpha_mode)?
    } else {
        let mut bits = item.gap_bits.clone();
        bits.extend(bytes_to_bits(&item.to_bytes(idx, huffman, alpha_mode)?));
        bits
    };

    let mut emitted_child_count = 0usize;
    let child_emission_skipped = alpha_mode
        && item.header.is_runeword
        && is_authority_overlap_code(item, alpha_mode);

    if !child_emission_skipped {
        for child in &item.socketed_items {
            let child_bits = if alpha_mode {
                child.to_bits(0, huffman, alpha_mode)?
            } else {
                bytes_to_bits(&child.to_bytes(0, huffman, alpha_mode)?)
            };
            bits.extend(child_bits);
            emitted_child_count += 1;
        }
    }

    Ok(ItemEmission {
        bits,
        emitted_child_count,
        child_emission_skipped,
    })
}

fn is_authority_overlap_code(item: &Item, alpha_mode: bool) -> bool {
    let trimmed = item.code.trim();
    let mut is_authority_overlap_code =
        alpha_mode && matches!(trimmed, "xrs" | "c8xr" | "rhd" | "wa2" | "ww" | "gcw");

    if alpha_mode {
        let registry = get_registry();
        if let Some(overrides) = &registry.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&val) = map.get("is_authority_overlap") {
                    is_authority_overlap_code = val != 0 || is_authority_overlap_code;
                }
            }
        }
    }

    is_authority_overlap_code
}

fn compare_exact_bits(expected: &[bool], actual: &[bool], base_bit: u64) -> BitCheck {
    let compare_len = expected.len().min(actual.len());
    if let Some(idx) = expected[..compare_len]
        .iter()
        .zip(&actual[..compare_len])
        .position(|(left, right)| left != right)
    {
        let bit_offset = base_bit + idx as u64;
        return BitCheck {
            matches: false,
            expected_bits: expected.len(),
            actual_bits: actual.len(),
            first_mismatch: Some(BitMismatch {
                kind: "content".to_string(),
                bit_offset,
                byte_offset: bit_offset / 8,
                expected_bit: expected.get(idx).copied(),
                actual_bit: actual.get(idx).copied(),
            }),
        };
    }

    if expected.len() != actual.len() {
        let bit_offset = base_bit + compare_len as u64;
        return BitCheck {
            matches: false,
            expected_bits: expected.len(),
            actual_bits: actual.len(),
            first_mismatch: Some(BitMismatch {
                kind: "length".to_string(),
                bit_offset,
                byte_offset: bit_offset / 8,
                expected_bit: expected.get(compare_len).copied(),
                actual_bit: actual.get(compare_len).copied(),
            }),
        };
    }

    BitCheck {
        matches: true,
        expected_bits: expected.len(),
        actual_bits: actual.len(),
        first_mismatch: None,
    }
}

fn compare_prefix_progress(
    expected_full: &[bool],
    actual_prefix: &[bool],
    base_bit: u64,
) -> Option<BitMismatch> {
    let compare_len = expected_full.len().min(actual_prefix.len());
    if let Some(idx) = expected_full[..compare_len]
        .iter()
        .zip(&actual_prefix[..compare_len])
        .position(|(left, right)| left != right)
    {
        let bit_offset = base_bit + idx as u64;
        return Some(BitMismatch {
            kind: "content".to_string(),
            bit_offset,
            byte_offset: bit_offset / 8,
            expected_bit: expected_full.get(idx).copied(),
            actual_bit: actual_prefix.get(idx).copied(),
        });
    }

    if actual_prefix.len() > expected_full.len() {
        let bit_offset = base_bit + expected_full.len() as u64;
        return Some(BitMismatch {
            kind: "length".to_string(),
            bit_offset,
            byte_offset: bit_offset / 8,
            expected_bit: None,
            actual_bit: actual_prefix.get(expected_full.len()).copied(),
        });
    }

    None
}

fn bits_from_range(bytes: &[u8], start_bit: u64, end_bit: u64) -> Vec<bool> {
    let mut out = Vec::with_capacity(end_bit.saturating_sub(start_bit) as usize);
    for bit_pos in start_bit..end_bit {
        let byte_idx = (bit_pos / 8) as usize;
        if byte_idx >= bytes.len() {
            break;
        }
        let bit_idx = (bit_pos % 8) as u8;
        out.push(((bytes[byte_idx] >> bit_idx) & 1) == 1);
    }
    out
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut out = Vec::with_capacity(bytes.len() * 8);
    for byte in bytes {
        for bit_idx in 0..8 {
            out.push(((byte >> bit_idx) & 1) == 1);
        }
    }
    out
}

fn read_section_count(section_bytes: &[u8]) -> u16 {
    if section_bytes.len() >= 4 {
        u16::from_le_bytes([section_bytes[2], section_bytes[3]])
    } else {
        0
    }
}

fn truncate_code(code: &str, max_len: usize) -> String {
    let trimmed = code.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else if max_len <= 3 {
        trimmed.chars().take(max_len).collect()
    } else {
        let mut truncated = trimmed.chars().take(max_len - 3).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}
