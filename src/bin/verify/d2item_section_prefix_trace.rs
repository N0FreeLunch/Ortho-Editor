use anyhow::{Context, Result, anyhow};
use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::map_core_sections;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::Cursor;

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
struct IsolatedInspect {
    code: String,
    bit_length: usize,
    scanner_hint: String,
    normalized_code: String,
    final_code: String,
    gap_len: usize,
    gap_source: String,
    emitter_bypass: bool,
    ownership_hint: String,
    ownership_reason: String,
    contradiction_class: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    isolated_inspect: Option<IsolatedInspect>,
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

            let isolated_inspect = get_isolated_inspect(
                bytes,
                item.range.start as usize,
                huffman,
                alpha_mode,
                &item.code,
                raw_bits.len(),
            );

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
                isolated_inspect,
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

fn classify_trace_ownership(
    item: &Item,
    scanner_hint: &str,
    normalized_code: &str,
    final_code: &str,
    gap_len: usize,
    gap_source: &str,
    emitter_bypass: bool,
) -> (String, String) {
    let padding_signals = emitter_bypass
        || item.is_opaque()
        || item.is_semi_opaque()
        || gap_source == "normalization:opaque_fallback";
        
    let is_kk_seam_drift = (scanner_hint.starts_with("wc") || scanner_hint.contains("wc"))
        && (final_code == "wwsl" || final_code == "wwu8")
        && gap_source == "normalization:drift_realigned";

    let replay_signals = gap_source == "header_gap_lookup"
        || is_kk_seam_drift
        || (!scanner_hint.is_empty()
            && scanner_hint == normalized_code
            && normalized_code == final_code
            && gap_len > 0);

    let ownership_hint = match (replay_signals, padding_signals) {
        (true, false) => "capture_replay",
        (false, true) => "emission_padding",
        _ => "ambiguous",
    };

    let ownership_reason = match ownership_hint {
        "capture_replay" => {
            if is_kk_seam_drift {
                format!(
                    "k  k seam drift identified: scanner_hint='{}' misaligned to final_code='{}' under drift_realigned. This is a capture_replay parsing geometry mismatch.",
                    scanner_hint, final_code
                )
            } else {
                format!(
                    "Header-derived replay signals dominate here: scanner_hint='{}', normalized_code='{}', final_code='{}', gap_len={}, gap_source='{}'.",
                    scanner_hint, normalized_code, final_code, gap_len, gap_source
                )
            }
        }
        "emission_padding" => format!(
            "Padding-preserving emission signals dominate here: emitter_bypass={}, gap_source='{}', final_code='{}'.",
            emitter_bypass, gap_source, final_code
        ),
        _ => format!(
            "Signals remain split between replay and padding: scanner_hint='{}', normalized_code='{}', final_code='{}', gap_len={}, gap_source='{}', emitter_bypass={}.",
            scanner_hint, normalized_code, final_code, gap_len, gap_source, emitter_bypass
        ),
    };

    (ownership_hint.to_string(), ownership_reason)
}

fn get_isolated_inspect(
    bytes: &[u8],
    offset: usize,
    huffman: &HuffmanTree,
    is_alpha: bool,
    section_code: &str,
    section_len: usize,
) -> Option<IsolatedInspect> {
    if !is_alpha {
        return None;
    }
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(offset as u32).unwrap_or(());
    
    if let Ok(item) = Item::from_reader(&mut reader, huffman, is_alpha) {
        let bit_end = reader.position_in_bits().unwrap_or(0) as usize;
        let bit_length = bit_end.saturating_sub(offset);

        let scanner_hint = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
            bytes,
            offset as u64,
            Some(offset as u64),
            huffman,
            true,
            0,
        )
        .map(|p| p.3.trim().to_string())
        .unwrap_or_default();

        let (normalized_code, gap_len, gap_source) = {
            let mut reader2 = BitReader::endian(Cursor::new(bytes), LittleEndian);
            let _ = reader2.skip(offset as u32).unwrap_or(());
            let mut cursor = d2r_core::data::bit_cursor::BitCursor::new(&mut reader2);
            
            let gap_override = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                bytes,
                offset as u64,
                Some(offset as u64),
                huffman,
                true,
                0,
            ).map(|p| {
                let mut gap = p.8 as usize;
                if p.5 == 7 && !p.6 {
                    gap = gap.saturating_sub(45);
                }
                gap
            });
            
            let has_checksum_peek = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                bytes,
                offset as u64,
                Some(offset as u64),
                huffman,
                true,
                0,
            ).map(|p| p.9);

            if let Ok((header, _, _)) = d2r_core::domain::item::entity::parse_item_header(
                &mut cursor,
                true,
                Some(scanner_hint.as_str()),
                gap_override,
                true,
                None,
                has_checksum_peek,
                Some(offset as u64),
            ) {
                if header.is_compact {
                    cursor.base_pos = offset as u64;
                }
                let s_axiom = d2r_core::domain::stats::axiom::StatsAxiom::new(
                    header.version,
                    header.quality.unwrap_or(d2r_core::domain::item::quality::ItemQuality::Normal),
                    true,
                );
                let is_ho = s_axiom.is_header_only(header.flags, Some(scanner_hint.as_str()).unwrap_or(""));

                if is_ho {
                    (scanner_hint.clone(), 0usize, "header_only".to_string())
                } else {
                    let gap_len = if scanner_hint.trim() == "buc" || matches!(header.version, 1) {
                        0
                    } else {
                        s_axiom.header_gap(&scanner_hint, header.flags)
                    };
                    if gap_len > 0 {
                        let _ = cursor.skip(gap_len as u64);
                    }
                    let mut decoded = String::new();
                    let mut ok = true;
                    for _ in 0..4 {
                        if let Ok(c) = huffman.decode(&mut reader2) {
                            decoded.push(c);
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        let decoded_trimmed = decoded.trim().to_string();
                        let gap_source = if gap_len > 0 {
                            "header_gap_lookup".to_string()
                        } else {
                            if item.is_opaque() || item.is_semi_opaque() {
                                "normalization:opaque_fallback".to_string()
                            } else if decoded_trimmed == item.code.trim() {
                                "normalization:match_target".to_string()
                            } else {
                                "normalization:drift_realigned".to_string()
                            }
                        };
                        (decoded_trimmed, gap_len as usize, gap_source)
                    } else {
                        let gap_source = if item.is_opaque() || item.is_semi_opaque() {
                            "normalization:opaque_fallback".to_string()
                        } else {
                            "normalization:drift_realigned".to_string()
                        };
                        ("".to_string(), gap_len as usize, gap_source)
                    }
                }
            } else {
                ("".to_string(), 0usize, "unresolved".to_string())
            }
        };

        let final_code = item.code.trim().to_string();
        
        let emitter_bypass = {
            let trimmed_code = item.code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
            let is_target_blank = is_alpha && trimmed_code.is_empty();
            item.is_opaque() || item.is_semi_opaque() || is_target_blank
        };

        let (ownership_hint, ownership_reason) = classify_trace_ownership(
            &item,
            &scanner_hint,
            &normalized_code,
            &final_code,
            gap_len,
            &gap_source,
            emitter_bypass,
        );

        let code_mismatch = section_code.trim() != final_code.trim();
        let len_mismatch = section_len != bit_length;

        let contradiction_class = match (code_mismatch, len_mismatch) {
            (true, true) => "both_mismatch".to_string(),
            (true, false) => "code_mismatch".to_string(),
            (false, true) => "length_mismatch".to_string(),
            (false, false) => "none".to_string(),
        };

        Some(IsolatedInspect {
            code: final_code.clone(),
            bit_length,
            scanner_hint,
            normalized_code,
            final_code,
            gap_len,
            gap_source,
            emitter_bypass,
            ownership_hint,
            ownership_reason,
            contradiction_class,
        })
    } else {
        None
    }
}
