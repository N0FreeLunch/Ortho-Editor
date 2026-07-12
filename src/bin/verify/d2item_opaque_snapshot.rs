use anyhow::{Context, Result};
use std::env;
use std::fs;

use d2r_core::item::{HuffmanTree, Item, ItemModule};
use d2r_core::save::{find_jm_markers, Save};
use d2r_core::verify::args::{ArgError, ArgParser};

#[derive(serde::Serialize)]
struct OpaqueItem {
    index: usize,
    section_item_index: usize,
    section: String,
    section_header_byte_offset: u64,
    code: String,
    bit_start: u64,
    bit_end: u64,
    total_bits: u64,
    hex_payload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_body_hex: Option<String>,
    parser_probe: Option<ParserProbe>,
}

#[derive(serde::Serialize, Debug)]
struct ParserProbe {
    probe_source: String,
    candidate_start_bit: u64,
    candidate_limit_bits: u64,
    code_hint: Option<String>,
    forced_compact: Option<bool>,
    retained_outcome: String,
    retained_module_kind: String,
    outcome: String,
    module_kind: Option<String>,
    failure_error: Option<String>,
    failure_context_stack: Option<Vec<String>>,
    failure_bit_offset_abs: Option<u64>,
    failure_bit_offset_rel: Option<u64>,
    failure_context_relative_offset: Option<u64>,
    failure_hint: Option<String>,
}

#[derive(serde::Serialize)]
struct OpaqueSnapshotReport {
    save_file: String,
    version: u32,
    opaque_items: Vec<OpaqueItem>,
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

fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((bits.len() + 7) / 8);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                byte |= 1 << i;
            }
        }
        bytes.push(byte);
    }
    bytes
}

fn probe_parser(
    item: &Item,
    bytes: &[u8],
    huffman: &HuffmanTree,
    is_alpha: bool,
    item_index: usize,
) -> ParserProbe {
    let candidate_start_bit = item.range.start;
    let candidate_limit_bits = item.total_bits as u64;
    let code_hint = match item.code.trim() {
        "" | "Opaque" => None,
        code => Some(code.to_string()),
    };

    let (retained_outcome, retained_module_kind) = item
        .modules
        .iter()
        .find_map(|module| match module {
            ItemModule::SemiOpaque { .. } => Some(("semi_opaque", "SemiOpaque")),
            ItemModule::Opaque { .. } => Some(("opaque", "Opaque")),
            ItemModule::Residue { .. } => Some(("opaque", "Residue")),
            _ => None,
        })
        .unwrap_or(("parsed", "Structured"));

    let candidate_bits = bits_from_range(
        bytes,
        candidate_start_bit,
        candidate_start_bit.saturating_add(candidate_limit_bits),
    );
    let candidate_bytes = bits_to_bytes(&candidate_bits);
    let live_result = d2r_core::domain::item::serialization::parse_item_at_with_limit(
        &candidate_bytes,
        0,
        candidate_start_bit,
        huffman,
        item_index,
        is_alpha,
        Some(candidate_limit_bits),
        None,
        code_hint.as_deref(),
    );
    let (
        outcome,
        module_kind,
        failure_error,
        failure_context_stack,
        failure_bit_offset_abs,
        failure_bit_offset_rel,
        failure_context_relative_offset,
        failure_hint,
    ) = match live_result {
        Ok((reparsed, _consumed_bits)) => {
            let module_kind = reparsed.modules.iter().find_map(|module| match module {
                ItemModule::SemiOpaque { .. } => Some("SemiOpaque"),
                ItemModule::Opaque { .. } => Some("Opaque"),
                ItemModule::Residue { .. } => Some("Residue"),
                _ => None,
            });
            let outcome = match module_kind {
                Some("SemiOpaque") => "semi_opaque",
                Some(_) => "opaque",
                None => "parsed",
            };
            (
                outcome.to_string(),
                Some(module_kind.unwrap_or("Structured").to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
            )
        }
        Err(failure) => {
            let relative = failure.bit_offset.checked_sub(candidate_start_bit);
            (
                "parse_failure".to_string(),
                None,
                Some(failure.error.to_string()),
                Some(failure.context_stack),
                Some(failure.bit_offset),
                relative,
                Some(failure.context_relative_offset),
                failure.hint,
            )
        }
    };

    ParserProbe {
        probe_source: "bounded_live_reparse".to_string(),
        candidate_start_bit,
        candidate_limit_bits,
        code_hint,
        forced_compact: None,
        retained_outcome: retained_outcome.to_string(),
        retained_module_kind: retained_module_kind.to_string(),
        outcome,
        module_kind,
        failure_error,
        failure_context_stack,
        failure_bit_offset_abs,
        failure_bit_offset_rel,
        failure_context_relative_offset,
        failure_hint,
    }
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_opaque_snapshot")
        .description("Dumps raw payload of opaque/semi-opaque items from a D2R save file to JSON");

    parser.add_arg("save_file", "path to the save file (.d2s)");
    parser
        .add_flag("json", "print machine-readable report (JSON)")
        .long("json");

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

    let path = parsed.get("save_file").unwrap();

    let bytes = fs::read(path).with_context(|| format!("Cannot read '{}'", path))?;
    let save = Save::from_bytes(&bytes).with_context(|| "Cannot parse D2R header")?;
    let is_alpha = save.header.version == 105;

    let jm_positions = find_jm_markers(&bytes);
    let section_labels = [
        "Player Items",
        "Corpse Items",
        "Mercenary Items",
        "Iron Golem",
    ];

    let huffman = HuffmanTree::new();
    let mut opaque_items = Vec::new();
    let mut global_index = 0usize;

    for (section_index, &pos) in jm_positions.iter().enumerate() {
        let section_label = section_labels
            .get(section_index)
            .copied()
            .unwrap_or("Unknown Section");
        let item_count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        let next_pos = jm_positions
            .get(section_index + 1)
            .copied()
            .unwrap_or(bytes.len());
        let section_data = &bytes[pos..next_pos];

        if item_count == 0 && (next_pos - pos) <= 6 {
            continue;
        }

        let items = match Item::read_section(
            section_data,
            pos as u64 * 8,
            item_count,
            &huffman,
            is_alpha,
            false,
        ) {
            Ok(items) => items,
            Err(_) => {
                continue;
            }
        };

        for (section_item_index, item) in items.into_iter().enumerate() {
            let is_opaque = item.code == "Opaque" || item.is_opaque();
            let mut semi_opaque_module = None;
            for module in &item.modules {
                if let ItemModule::SemiOpaque { body_bits, reason } = module {
                    semi_opaque_module = Some((body_bits, reason));
                    break;
                }
            }

            if is_opaque || semi_opaque_module.is_some() {
                let raw_bits = bits_from_range(&bytes, item.range.start, item.range.end);
                let hex_payload = hex::encode(bits_to_bytes(&raw_bits));

                let (module_reason, module_body_hex) =
                    if let Some((body_bits, reason)) = semi_opaque_module {
                        (
                            Some(reason.clone()),
                            Some(hex::encode(bits_to_bytes(body_bits))),
                        )
                    } else {
                        (None, None)
                    };

                opaque_items.push(OpaqueItem {
                    index: global_index,
                    section_item_index,
                    section: section_label.to_string(),
                    section_header_byte_offset: pos as u64,
                    code: item.code.trim().to_string(),
                    bit_start: item.range.start,
                    bit_end: item.range.end,
                    total_bits: item.total_bits as u64,
                    hex_payload,
                    module_reason,
                    module_body_hex,
                    parser_probe: Some(probe_parser(
                        &item,
                        &bytes,
                        &huffman,
                        is_alpha,
                        global_index,
                    )),
                });
            }
            global_index += 1;
        }
    }

    let report = OpaqueSnapshotReport {
        save_file: path.to_string(),
        version: save.header.version,
        opaque_items,
    };

    let report_json = serde_json::to_string_pretty(&report)?;
    if let Some(output_path) = parsed.get("output") {
        fs::write(output_path, &report_json)
            .with_context(|| format!("Cannot write report to '{}'", output_path))?;
    } else {
        println!("{}", report_json);
    }

    Ok(())
}

#[test]
fn bounded_live_reparse_reports_concrete_failure() {
    let mut item = Item::default();
    item.range.start = 0;
    item.total_bits = 1;
    item.code = "Opaque".to_string();
    item.modules.push(ItemModule::Opaque(Vec::new()));

    let probe = probe_parser(&item, &[0], &HuffmanTree::new(), true, 0);
    assert_eq!(probe.probe_source, "bounded_live_reparse");
    assert_eq!(probe.retained_outcome, "opaque");
    assert_eq!(probe.retained_module_kind, "Opaque");
    assert_eq!(probe.outcome, "parse_failure");
    assert!(probe.failure_error.is_some());
    assert!(probe.failure_bit_offset_abs.is_some());
}
