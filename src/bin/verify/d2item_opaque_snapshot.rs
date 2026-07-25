use anyhow::{Context, Result};
use std::env;
use std::fs;

use d2r_core::domain::item::opaque_probe::{
    bits_from_range, bits_to_bytes, collect_boundary_candidates, probe_opaque_item,
    BoundaryCandidate, ExtensionSweepRecord, ExtensionSweepSummary, OpaqueProbeRequest,
    ParserProbe,
};
use d2r_core::domain::item::scanner::scan_item_markers;
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
    boundary_candidates: Vec<BoundaryCandidate>,
    extension_sweep_summary: ExtensionSweepSummary,
    extension_sweep: Vec<ExtensionSweepRecord>,
}

#[derive(serde::Serialize)]
struct OpaqueSnapshotReport {
    save_file: String,
    version: u32,
    opaque_items: Vec<OpaqueItem>,
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
        let section_bit_offset = pos as u64 * 8;

        if item_count == 0 && (next_pos - pos) <= 6 {
            continue;
        }

        let items = match Item::read_section(
            section_data,
            section_bit_offset,
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
        let scanner_markers = scan_item_markers(
            section_data,
            &huffman,
            is_alpha,
            section_bit_offset,
            Some(item_count),
            true,
        );

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
                let boundary_candidates = collect_boundary_candidates(
                    &scanner_markers,
                    section_bit_offset,
                    item.range.end,
                );
                let probe_result = probe_opaque_item(OpaqueProbeRequest {
                    item: &item,
                    bytes: &bytes,
                    huffman: &huffman,
                    is_alpha,
                    item_index: global_index,
                    boundary_candidates: &boundary_candidates,
                })?;

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
                    parser_probe: Some(probe_result.parser_probe),
                    boundary_candidates,
                    extension_sweep_summary: probe_result.extension_sweep_summary,
                    extension_sweep: probe_result.extension_sweep,
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

