// d2item_opaque_census.rs -- Report-only corpus census harness for opaque predicate hit routing
// Standalone verification binary for collecting item opaque predicate hits across a save corpus.

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use std::collections::BTreeMap;

use d2r_core::domain::item::opaque_probe::{
    collect_boundary_candidates, probe_opaque_item, BoundaryCandidate, OpaqueProbeRequest,
    ParserProbe,
};
use d2r_core::domain::item::scanner::scan_item_markers;
use d2r_core::item::{HuffmanTree, Item, ItemClassProjection};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::Serialize;

#[derive(Serialize, Debug)]
struct FailureFamily {
    failure_error: String,
    failure_context_stack: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nearest_boundary_delta_bits: Option<i64>,
    count: usize,
    representative_item_path: String,
    representative_save_file: String,
}

#[derive(Serialize, Debug)]
struct CausalTaxonomy {
    included_top_level_probe_rows: usize,
    outcome_counts: BTreeMap<String, usize>,
    failure_families: Vec<FailureFamily>,
}

#[derive(Serialize, Debug, Default)]
struct ClassificationCounts {
    parsed_with_raw_carrier: usize,
    semi_opaque: usize,
    residue: usize,
    opaque: usize,
}

#[derive(Serialize, Debug)]
struct UnreadableSave {
    save_file: String,
    reason: String,
}

#[derive(Serialize, Debug)]
struct CensusRow {
    save_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<String>,
    item_index: usize,
    item_path: String,
    code: String,
    bit_start: u64,
    bit_end: u64,
    total_bits: u64,
    classification: String,
    is_semi_opaque: bool,
    is_residue: bool,
    is_opaque: bool,
    is_top_level: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    parser_probe: Option<ParserProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    boundary_candidates: Option<Vec<BoundaryCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained_header_layout: Option<RetainedHeaderLayout>,
}

#[derive(Serialize, Debug)]
struct RetainedHeaderLayout {
    bit_start: u64,
    bit_end: u64,
    flags: u32,
    version: u8,
    mode: u8,
    location: u8,
    x: u8,
    y: u8,
    page: u8,
    socket_hint: u8,
    checksum: Option<u8>,
    is_compact: bool,
    is_runeword: bool,
    alpha_header_gap_value: Option<u32>,
    alpha_header_gap_bit_count: usize,
    alpha_code_bit_count: usize,
    alpha_nudge: Option<u8>,
}

#[derive(Serialize, Debug)]
struct OpaqueCensusReport {
    saves_dir: String,
    collection_policy: String,
    total_saves_scanned: usize,
    total_items_collected: usize,
    cache_ineligible_count: usize,
    opaque_predicate_hit_count: usize,
    classification_counts: ClassificationCounts,
    unreadable_saves: Vec<UnreadableSave>,
    rows: Vec<CensusRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    causal_taxonomy: Option<CausalTaxonomy>,
}

struct CollectedItemInfo<'a> {
    item: &'a Item,
    section: Option<String>,
    item_index: usize,
    item_path: String,
    top_level_item_index: usize,
    is_top_level: bool,
}

fn collect_items_recursive<'a>(
    item: &'a Item,
    section: Option<String>,
    parent_path: &str,
    save_item_counter: &mut usize,
    top_level_item_index: usize,
    is_top_level: bool,
    collected: &mut Vec<CollectedItemInfo<'a>>,
) {
    let item_index = *save_item_counter;
    *save_item_counter += 1;

    collected.push(CollectedItemInfo {
        item,
        section,
        item_index,
        item_path: parent_path.to_string(),
        top_level_item_index,
        is_top_level,
    });

    for (s_idx, socketed) in item.socketed_items.iter().enumerate() {
        let child_path = format!("{}/socket/{}", parent_path, s_idx);
        collect_items_recursive(
            socketed,
            collected.last().and_then(|info| info.section.clone()),
            &child_path,
            save_item_counter,
            top_level_item_index,
            false,
            collected,
        );
    }
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_opaque_census")
        .description("Report-only corpus census harness for opaque predicate hit routing");

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_flag("json", "Emit opaque census report in JSON format to stdout")
        .short('j')
        .long("json");

    parser
        .add_flag("probe", "Probe top-level opaque items using section boundary candidates")
        .long("probe");

    parser
        .add_flag(
            "causal-taxonomy",
            "Aggregate causal opaque bitstream taxonomy and failure families",
        )
        .long("causal-taxonomy");

    parser
        .add_flag(
            "trace-header-gap",
            "Emit retained-item header/layout snapshot for parse_failure probe rows",
        )
        .long("trace-header-gap");

    parser
        .add_opt("output", "Path to output JSON report file")
        .short('o')
        .long("output");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let enable_probe = parsed.is_set("probe");
    let enable_causal_taxonomy = parsed.is_set("causal-taxonomy");
    let enable_trace_header_gap = parsed.is_set("trace-header-gap");

    if enable_causal_taxonomy && !enable_probe {
        eprintln!("Error: --causal-taxonomy requires --probe flag.");
        process::exit(1);
    }

    let saves_dir_str = parsed
        .get("saves-dir")
        .cloned()
        .unwrap_or_else(|| "tests/fixtures/savegames/modified".to_string());

    let saves_dir_path = Path::new(&saves_dir_str);
    if !saves_dir_path.exists() || !saves_dir_path.is_dir() {
        eprintln!(
            "[ERROR] Saves directory '{}' does not exist or is not a directory.",
            saves_dir_str
        );
        process::exit(1);
    }

    let enable_probe = parsed.is_set("probe");

    let mut save_files: Vec<PathBuf> = Vec::new();
    fn collect_d2s_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_d2s_recursive(&path, files);
                } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("d2s") {
                    files.push(path);
                }
            }
        }
    }
    collect_d2s_recursive(saves_dir_path, &mut save_files);
    save_files.sort();

    let total_saves_scanned = save_files.len();
    let huffman = HuffmanTree::new();

    let mut unreadable_saves = Vec::new();
    let mut total_items_collected = 0usize;
    let mut cache_ineligible_count = 0usize;
    let mut opaque_predicate_hit_count = 0usize;
    let mut classification_counts = ClassificationCounts::default();
    let mut rows = Vec::new();

    for save_path in &save_files {
        let save_file_rel = save_path.display().to_string();
        let bytes = match fs::read(save_path) {
            Ok(b) => b,
            Err(e) => {
                unreadable_saves.push(UnreadableSave {
                    save_file: save_file_rel,
                    reason: format!("File read error: {}", e),
                });
                continue;
            }
        };

        if bytes.len() < 8 {
            unreadable_saves.push(UnreadableSave {
                save_file: save_file_rel,
                reason: "File buffer under 8 bytes".to_string(),
            });
            continue;
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105;

        let jm_positions = find_jm_markers(&bytes);
        if jm_positions.is_empty() {
            unreadable_saves.push(UnreadableSave {
                save_file: save_file_rel,
                reason: "No JM section markers found".to_string(),
            });
            continue;
        }

        let pos = jm_positions[0];
        if bytes.len() < pos + 4 {
            unreadable_saves.push(UnreadableSave {
                save_file: save_file_rel,
                reason: "Player Items JM section header truncated".to_string(),
            });
            continue;
        }

        let item_count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        let next_pos = jm_positions.get(1).copied().unwrap_or(bytes.len());
        let section_data = &bytes[pos..next_pos];
        let section_bit_offset = pos as u64 * 8;

        let top_items = match Item::read_section(
            section_data,
            section_bit_offset,
            item_count,
            &huffman,
            is_alpha,
            false,
        ) {
            Ok(items) => items,
            Err(e) => {
                unreadable_saves.push(UnreadableSave {
                    save_file: save_file_rel,
                    reason: format!("Failed to parse Player Items section: {}", e),
                });
                continue;
            }
        };

        let scanner_markers = if enable_probe {
            Some(scan_item_markers(
                section_data,
                &huffman,
                is_alpha,
                section_bit_offset,
                Some(item_count),
                true,
            ))
        } else {
            None
        };

        let mut save_item_counter = 0usize;

        for (t_idx, top_item) in top_items.iter().enumerate() {
            let mut collected = Vec::new();
            let top_path = format!("{}", t_idx);
            collect_items_recursive(
                top_item,
                Some("Player Items".to_string()),
                &top_path,
                &mut save_item_counter,
                t_idx,
                true,
                &mut collected,
            );

            for info in collected {
                process_collected_item(
                    info,
                    &save_file_rel,
                    &bytes,
                    &huffman,
                    is_alpha,
                    enable_probe,
                    enable_trace_header_gap,
                    scanner_markers.as_deref(),
                    section_bit_offset,
                    &mut total_items_collected,
                    &mut cache_ineligible_count,
                    &mut opaque_predicate_hit_count,
                    &mut classification_counts,
                    &mut rows,
                )?;
            }
        }
    }

    let causal_taxonomy = if enable_causal_taxonomy {
        let mut included_count = 0usize;
        let mut outcome_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut family_map: BTreeMap<(String, Vec<String>, Option<i64>), (usize, String, String)> =
            BTreeMap::new();

        for row in &rows {
            if row.is_top_level {
                if let Some(probe) = &row.parser_probe {
                    included_count += 1;
                    *outcome_counts.entry(probe.outcome.clone()).or_insert(0) += 1;

                    if probe.outcome == "parse_failure" {
                        let err_str = probe.failure_error.clone().unwrap_or_default();
                        let ctx_stack = probe.failure_context_stack.clone().unwrap_or_default();
                        let nearest_delta = if let Some(fail_offset) = probe.failure_bit_offset_abs {
                            if let Some(candidates) = &row.boundary_candidates {
                                candidates
                                    .iter()
                                    .map(|c| c.absolute_bit_position as i64 - fail_offset as i64)
                                    .min_by_key(|d| d.abs())
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let key = (err_str, ctx_stack, nearest_delta);
                        let entry = family_map.entry(key).or_insert((
                            0,
                            row.item_path.clone(),
                            row.save_file.clone(),
                        ));
                        entry.0 += 1;
                    }
                }
            }
        }

        let failure_families = family_map
            .into_iter()
            .map(
                |(
                    (failure_error, failure_context_stack, nearest_boundary_delta_bits),
                    (count, representative_item_path, representative_save_file),
                )| {
                    FailureFamily {
                        failure_error,
                        failure_context_stack,
                        nearest_boundary_delta_bits,
                        count,
                        representative_item_path,
                        representative_save_file,
                    }
                },
            )
            .collect();

        Some(CausalTaxonomy {
            included_top_level_probe_rows: included_count,
            outcome_counts,
            failure_families,
        })
    } else {
        None
    };

    let report = OpaqueCensusReport {
        saves_dir: saves_dir_str,
        collection_policy: "recursive (top-level and socketed player items)".to_string(),
        total_saves_scanned,
        total_items_collected,
        cache_ineligible_count,
        opaque_predicate_hit_count,
        classification_counts,
        unreadable_saves,
        rows,
        causal_taxonomy,
    };

    let report_json = serde_json::to_string_pretty(&report)
        .context("Failed to serialize opaque census report to JSON")?;

    if let Some(output_path) = parsed.get("output") {
        let out_path = Path::new(output_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out_path, &report_json)
            .with_context(|| format!("Cannot write output report to '{}'", output_path))?;
    }

    if parsed.is_set("json") {
        println!("{}", report_json);
    } else if parsed.get("output").is_none() {
        println!(
            "Scanned {} saves: {} total items, {} cache-ineligible, {} opaque predicate hits (semi_opaque: {}, residue: {}, opaque: {}).",
            report.total_saves_scanned,
            report.total_items_collected,
            report.cache_ineligible_count,
            report.opaque_predicate_hit_count,
            report.classification_counts.semi_opaque,
            report.classification_counts.residue,
            report.classification_counts.opaque,
        );
        if !report.unreadable_saves.is_empty() {
            println!("Unreadable saves: {}", report.unreadable_saves.len());
        }
    }

    Ok(())
}

fn process_collected_item(
    info: CollectedItemInfo,
    save_file_rel: &str,
    bytes: &[u8],
    huffman: &HuffmanTree,
    is_alpha: bool,
    enable_probe: bool,
    enable_trace_header_gap: bool,
    scanner_markers: Option<&[d2r_core::domain::item::scanner::ItemMarker]>,
    section_bit_offset: u64,
    total_items_collected: &mut usize,
    cache_ineligible_count: &mut usize,
    opaque_predicate_hit_count: &mut usize,
    classification_counts: &mut ClassificationCounts,
    rows: &mut Vec<CensusRow>,
) -> Result<()> {
    *total_items_collected += 1;

    let item = info.item;
    if ItemClassProjection::extract(item).is_none() {
        *cache_ineligible_count += 1;
    }

    let is_semi_opaque = item.is_semi_opaque();
    let is_residue = item.is_residue();
    let is_opaque = item.is_opaque();
    let fully_parsed_raw_carrier = is_opaque
        && !is_residue
        && !item.code.trim().is_empty()
        && item.parser_consumed_bits() == Some(item.total_bits);

    // Mutually exclusive classification according to Mini-Spec precedence
    let classification = if is_semi_opaque {
        Some("semi_opaque")
    } else if is_residue {
        Some("residue")
    } else if fully_parsed_raw_carrier {
        Some("parsed_with_raw_carrier")
    } else if is_opaque {
        Some("opaque")
    } else {
        None
    };

    if let Some(cls) = classification {
        *opaque_predicate_hit_count += 1;
        match cls {
            "parsed_with_raw_carrier" => classification_counts.parsed_with_raw_carrier += 1,
            "semi_opaque" => classification_counts.semi_opaque += 1,
            "residue" => classification_counts.residue += 1,
            "opaque" => classification_counts.opaque += 1,
            _ => {}
        }

        let (parser_probe, boundary_candidates) =
            if enable_probe && info.is_top_level && scanner_markers.is_some() {
                let markers = scanner_markers.unwrap();
                let candidates =
                    collect_boundary_candidates(markers, section_bit_offset, item.range.end);
                let probe_res = probe_opaque_item(OpaqueProbeRequest {
                    item,
                    bytes,
                    huffman,
                    is_alpha,
                    item_index: info.top_level_item_index,
                    boundary_candidates: &candidates,
                })?;
                (Some(probe_res.parser_probe), Some(candidates))
            } else {
                (None, None)
            };

        let retained_header_layout = if enable_trace_header_gap {
            if let Some(ref probe) = parser_probe {
                if probe.outcome == "parse_failure" || probe.failure_error.is_some() {
                    Some(RetainedHeaderLayout {
                        bit_start: item.range.start,
                        bit_end: item.range.end,
                        flags: item.header.flags,
                        version: item.header.version,
                        mode: item.header.mode,
                        location: item.header.location,
                        x: item.header.x,
                        y: item.header.y,
                        page: item.header.page,
                        socket_hint: item.header.socket_hint,
                        checksum: item.header.alpha_checksum,
                        is_compact: item.header.is_compact,
                        is_runeword: item.header.is_runeword,
                        alpha_header_gap_value: item.body.alpha_header_gap,
                        alpha_header_gap_bit_count: item.body.alpha_header_gap_bits.len(),
                        alpha_code_bit_count: item.body.alpha_code_bits.len(),
                        alpha_nudge: item.body.alpha_nudge,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        rows.push(CensusRow {
            save_file: save_file_rel.to_string(),
            section: info.section,
            item_index: info.item_index,
            item_path: info.item_path,
            code: item.code.trim().to_string(),
            bit_start: item.range.start,
            bit_end: item.range.end,
            total_bits: item.total_bits as u64,
            classification: cls.to_string(),
            is_semi_opaque,
            is_residue,
            is_opaque,
            is_top_level: info.is_top_level,
            parser_probe,
            boundary_candidates,
            retained_header_layout,
        });
    }

    Ok(())
}
