use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::domain::item::geometry::{
    ExpectedHeaderWidthProducer, HeaderChecksumBranch, LiveHeaderFamilyClassifier,
};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use rayon::prelude::*;
use serde_json::json;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn print_bits_window(bytes: &[u8], start_bit: usize, bit_count: usize) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(start_bit as u32).unwrap_or(());
    println!("Bits from {} ({} bits):", start_bit, bit_count);
    for i in 0..bit_count {
        let bit = reader.read_bit().unwrap_or(false);
        print!("{}", if bit { '1' } else { '0' });
        if (i + 1) % 8 == 0 {
            print!(" ");
        }
        if (i + 1) % 32 == 0 {
            println!("(bit {})", start_bit + i + 1);
        }
    }
    println!();
}

fn read_bits(reader: &mut BitReader<Cursor<&[u8]>, LittleEndian>, count: u32) -> u32 {
    reader.read_var(count).unwrap_or(0)
}

fn analyze_non_compact_item(bytes: &[u8], bit_start: usize, huffman: &HuffmanTree) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit_start as u32).unwrap_or(());
    let mut offset = bit_start;

    let flags = read_bits(&mut reader, 32);
    println!(
        "  flags           {:>5}-{:>5} = 0x{:08X}",
        offset,
        offset + 32,
        flags
    );
    offset += 32;

    let version = read_bits(&mut reader, 3);
    let mode = read_bits(&mut reader, 3);
    let location = read_bits(&mut reader, 4);
    let x = read_bits(&mut reader, 4);
    let y = read_bits(&mut reader, 4);
    let page = read_bits(&mut reader, 3);
    println!(
        "  version         {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        version
    );
    offset += 3;
    println!(
        "  mode            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        mode
    );
    offset += 3;
    println!(
        "  location        {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        location
    );
    offset += 4;
    println!("  x               {:>5}-{:>5} = {}", offset, offset + 4, x);
    offset += 4;
    println!("  y               {:>5}-{:>5} = {}", offset, offset + 4, y);
    offset += 4;
    println!(
        "  page            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        page
    );
    offset += 3;

    let code_start = offset;
    let mut code = String::new();
    for _ in 0..4 {
        code.push(huffman.decode(&mut reader).unwrap_or('?'));
    }
    let code_end = reader.position_in_bits().unwrap_or(0) as usize;
    println!(
        "  code            {:>5}-{:>5} = '{}'",
        code_start, code_end, code
    );
    offset = code_end;

    let socketed_count = read_bits(&mut reader, 3);
    println!(
        "  post-code bits  {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        socketed_count
    );
    offset += 3;

    let id = read_bits(&mut reader, 32);
    let level = read_bits(&mut reader, 7);
    let quality = read_bits(&mut reader, 4);
    let multi_graphics = read_bits(&mut reader, 1);
    println!(
        "  id              {:>5}-{:>5} = {}",
        offset,
        offset + 32,
        id
    );
    offset += 32;
    println!(
        "  level           {:>5}-{:>5} = {}",
        offset,
        offset + 7,
        level
    );
    offset += 7;
    println!(
        "  quality         {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        quality
    );
    offset += 4;
    println!(
        "  has graphics    {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        multi_graphics
    );
    offset += 1;
    if multi_graphics != 0 {
        let graphic_id = read_bits(&mut reader, 3);
        println!(
            "  graphic id      {:>5}-{:>5} = {}",
            offset,
            offset + 3,
            graphic_id
        );
        offset += 3;
    }

    let class_specific = read_bits(&mut reader, 1);
    println!(
        "  class specific  {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        class_specific
    );
    offset += 1;
    if class_specific != 0 {
        let class_bits = read_bits(&mut reader, 11);
        println!(
            "  class data      {:>5}-{:>5} = {}",
            offset,
            offset + 11,
            class_bits
        );
        offset += 11;
    }

    println!("  next 64 bits from {}", offset);
    print_bits_window(bytes, offset, 64);

    println!("  0x1FF candidates after {}", offset);
    for delta in 0..48 {
        let mut probe = BitReader::endian(Cursor::new(bytes), LittleEndian);
        let _ = probe.skip((offset + delta) as u32).unwrap_or(());
        if probe.read::<9, u32>().unwrap_or(0) == 0x1FF {
            println!("    offset {} -> bit {}", delta, offset + delta);
        }
    }
}

fn item_to_json(item: &Item, provenance: Option<serde_json::Value>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("code".to_string(), json!(item.code.trim()));
    map.insert(
        "bit_length".to_string(),
        json!(item.range.end - item.range.start),
    );
    map.insert(
        "stats".to_string(),
        json!(item
            .properties
            .iter()
            .map(|p| {
                json!({
                    "id": p.stat_id,
                    "name": p.name,
                    "is_unknown": p.name.starts_with("Unknown")
                })
            })
            .collect::<Vec<_>>()),
    );

    let residue = item.modules.iter().find_map(|m| {
        if let d2r_core::item::ItemModule::Residue(bits) = m {
            Some(
                bits.iter()
                    .map(|&b| if b { '1' } else { '0' })
                    .collect::<String>(),
            )
        } else {
            None
        }
    });
    map.insert("residue_bits".to_string(), json!(residue));

    if let Some(prov) = provenance {
        map.insert("provenance".to_string(), prov);
    }

    serde_json::Value::Object(map)
}

fn compare_first_difference_offset(left: &[bool], right: &[bool], base_bit: u64) -> Option<u64> {
    let compare_len = left.len().min(right.len());
    if let Some(idx) = left[..compare_len]
        .iter()
        .zip(&right[..compare_len])
        .position(|(lhs, rhs)| lhs != rhs)
    {
        return Some(base_bit + idx as u64);
    }

    if left.len() != right.len() {
        return Some(base_bit + compare_len as u64);
    }

    None
}

fn classify_local_73_padding_influence(
    with_padding_bits: &[bool],
    without_padding_bits: &[bool],
    base_bit: u64,
) -> (String, String) {
    let local_bit = 73usize;
    let local_bit_offset = base_bit + local_bit as u64;
    let first_difference_offset =
        compare_first_difference_offset(with_padding_bits, without_padding_bits, base_bit)
            .map(|offset| offset.to_string())
            .unwrap_or_else(|| "none".to_string());

    if with_padding_bits.len() <= local_bit || without_padding_bits.len() <= local_bit {
        return (
            "unobservable".to_string(),
            format!(
                "Local bit 73 is unavailable because strict rebuild lengths are {} and {} bits; first difference offset = {}.",
                with_padding_bits.len(),
                without_padding_bits.len(),
                first_difference_offset
            ),
        );
    }

    let with_bit = with_padding_bits[local_bit];
    let without_bit = without_padding_bits[local_bit];

    if with_bit != without_bit {
        (
            "influences_local_73".to_string(),
            format!(
                "Local bit 73 changes from {} to {} at absolute bit {}; first difference offset = {}.",
                u8::from(with_bit),
                u8::from(without_bit),
                local_bit_offset,
                first_difference_offset
            ),
        )
    } else {
        (
            "does_not_influence_local_73".to_string(),
            format!(
                "Local bit 73 remains {} in both strict rebuilds at absolute bit {}; first difference offset = {}.",
                u8::from(with_bit),
                local_bit_offset,
                first_difference_offset
            ),
        )
    }
}

fn section_context_report(
    item: &Item,
    section_item_index: usize,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    emit_phase_trace: bool,
    coordinate_bit: Option<u64>,
) -> serde_json::Value {
    let mut with_padding_item = item.clone();
    with_padding_item.bits.clear();
    let with_padding_bits = match with_padding_item.to_bits(section_item_index, huffman, alpha_mode)
    {
        Ok(bits) => bits,
        Err(e) => {
            return json!({
                "section_item_index": section_item_index,
                "code": item.code.trim(),
                "range_start": item.range.start,
                "range_end": item.range.end,
                "segments": item.segments.clone(),
                "alpha_alignment_padding_len": item.body.alpha_alignment_padding.len(),
                "strict_with_padding_bits": serde_json::Value::Null,
                "strict_without_padding_bits": serde_json::Value::Null,
                "first_difference_offset": serde_json::Value::Null,
                "local_73_padding_influence": "unobservable",
                "local_73_padding_influence_reason": format!(
                    "Failed to rebuild the retained-padding strict view for section item {}: {}",
                    section_item_index,
                    e
                ),
                "errors": [e.to_string()],
            });
        }
    };

    let mut without_padding_item = item.clone();
    without_padding_item.bits.clear();
    without_padding_item.body.alpha_alignment_padding.clear();
    let without_padding_bits =
        match without_padding_item.to_bits(section_item_index, huffman, alpha_mode) {
            Ok(bits) => bits,
            Err(e) => {
                return json!({
                    "section_item_index": section_item_index,
                    "code": item.code.trim(),
                    "range_start": item.range.start,
                    "range_end": item.range.end,
                    "segments": item.segments.clone(),
                    "alpha_alignment_padding_len": item.body.alpha_alignment_padding.len(),
                    "strict_with_padding_bits": with_padding_bits,
                    "strict_without_padding_bits": serde_json::Value::Null,
                    "first_difference_offset": serde_json::Value::Null,
                    "local_73_padding_influence": "unobservable",
                    "local_73_padding_influence_reason": format!(
                        "Failed to rebuild the cleared-padding strict view for section item {}: {}",
                        section_item_index,
                        e
                    ),
                    "errors": [e.to_string()],
                });
            }
        };

    let target_local_bit = coordinate_bit
        .and_then(|absolute| {
            (absolute >= item.range.start && absolute < item.range.end)
                .then(|| absolute - item.range.start)
        })
        .or_else(|| coordinate_bit.is_none().then_some(73));
    let target_absolute_bit = coordinate_bit.unwrap_or(item.range.start + 73);

    let (strict_emission_bits, strict_emission_phases) = if emit_phase_trace {
        match with_padding_item.to_bits_with_phase_trace(section_item_index, huffman, alpha_mode) {
            Ok((bits, phases)) => (Some(bits), phases),
            Err(e) => {
                return json!({
                    "section_item_index": section_item_index,
                    "code": item.code.trim(),
                    "range_start": item.range.start,
                    "range_end": item.range.end,
                    "strict_bit_count": without_padding_bits.len(),
                    "target_local_bit": target_local_bit,
                    "target_absolute_bit": target_absolute_bit,
                    "strict_emission_phases": [],
                    "target_phase": serde_json::Value::Null,
                    "strict_emission_phase_observable": false,
                    "strict_emission_phase_unobservable_reason": format!(
                        "Strict emission trace failed for section item {}: {}",
                        section_item_index, e
                    ),
                });
            }
        }
    } else {
        (None, Vec::new())
    };

    let strict_emission_phase_report = if emit_phase_trace {
        let target_phase = target_local_bit.and_then(|local| {
            strict_emission_phases
                .iter()
                .find(|phase| phase.start <= local && local < phase.end)
        });
        let target_phases = target_local_bit
            .map(|local| {
                strict_emission_phases
                    .iter()
                    .filter(|phase| {
                        phase.start <= local
                            && local < phase.end
                            && phase.label != "summary_body_and_alignment"
                            && phase.label != "summary_target_width_alignment"
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        json!({
            "strict_emission_phases": strict_emission_phases,
            "target_phase": target_phase,
            "target_containing_subphases": target_phases,
            "strict_emission_phase_observable": target_phases.len() == 1,
            "strict_emission_phase_unobservable_reason": if target_phases.len() == 1 {
                serde_json::Value::Null
            } else if target_local_bit.is_none() {
                json!(format!(
                    "Target absolute bit {} is outside the selected item's half-open range {}..{}",
                    target_absolute_bit, item.range.start, item.range.end
                ))
            } else if target_phases.is_empty() {
                json!(format!(
                    "No recorded phase contains target local bit {}",
                    target_local_bit.unwrap()
                ))
            } else {
                json!(format!(
                    "Multiple recorded phases contain target local bit {}",
                    target_local_bit.unwrap()
                ))
            },
        })
    } else {
        json!({})
    };
    let target_phase = target_local_bit.and_then(|local| {
        strict_emission_phases
            .iter()
            .find(|phase| phase.start <= local && local < phase.end)
    });

    let first_difference_offset = compare_first_difference_offset(
        &with_padding_bits,
        &without_padding_bits,
        item.range.start,
    );
    let (local_73_padding_influence, local_73_padding_influence_reason) =
        classify_local_73_padding_influence(
            &with_padding_bits,
            &without_padding_bits,
            item.range.start,
        );

    let segments = item
        .segments
        .iter()
        .map(|segment| {
            json!({
                "label": segment.label,
                "start": segment.start,
                "end": segment.end,
                "depth": segment.depth
            })
        })
        .collect::<Vec<_>>();

    json!({
        "section_item_index": section_item_index,
        "code": item.code.trim(),
        "range_start": item.range.start,
        "range_end": item.range.end,
        "segments": segments,
        "alpha_alignment_padding_len": item.body.alpha_alignment_padding.len(),
        "strict_with_padding_bit_count": with_padding_bits.len(),
        "strict_with_padding_bits": with_padding_bits,
        "strict_without_padding_bit_count": without_padding_bits.len(),
        "strict_without_padding_bits": without_padding_bits,
        "strict_bit_count": strict_emission_bits
            .as_ref()
            .map(|bits| bits.len())
            .unwrap_or_else(|| item.range.end.saturating_sub(item.range.start) as usize),
        "target_local_bit": target_local_bit,
        "target_absolute_bit": target_absolute_bit,
        "strict_emission_phases": if emit_phase_trace {
            json!(strict_emission_phases)
        } else {
            json!([])
        },
        "target_phase": if emit_phase_trace {
            json!(target_phase)
        } else {
            serde_json::Value::Null
        },
        "strict_emission_phase_observable": emit_phase_trace && target_phase.is_some(),
        "first_difference_offset": first_difference_offset,
        "local_73_padding_influence": local_73_padding_influence,
        "local_73_padding_influence_reason": local_73_padding_influence_reason,
        "strict_emission_phase_provenance": strict_emission_phase_report,
    })
}

fn section_segment_witness_report(items: &[Item], fixture: &str) -> serde_json::Value {
    const CLAIMED_GAP_LABEL: &str = "alpha_reader_claimed_width_gap";
    const CLAIMED_GAP_START: u64 = 72;
    const CLAIMED_GAP_END: u64 = 224;
    const PADDING_TAIL_LABEL: &str = "alpha_alignment_padding_tail_capture";
    const PADDING_TAIL_START: u64 = 224;
    const PADDING_TAIL_END: u64 = 264;

    let candidates = items
        .iter()
        .enumerate()
        .filter_map(|(section_item_index, item)| {
            let segments = item
                .segments
                .iter()
                .filter(|segment| {
                    segment.depth == 0
                        && ((segment.label == CLAIMED_GAP_LABEL
                            && segment.start == CLAIMED_GAP_START
                            && segment.end == CLAIMED_GAP_END)
                            || (segment.label == PADDING_TAIL_LABEL
                                && segment.start == PADDING_TAIL_START
                                && segment.end == PADDING_TAIL_END))
                })
                .map(|segment| {
                    json!({
                        "label": segment.label,
                        "start": segment.start,
                        "end": segment.end,
                        "depth": segment.depth
                    })
                })
                .collect::<Vec<_>>();

            if segments.len() == 2 {
                Some(json!({
                    "section_item_index": section_item_index,
                    "code": item.code.trim(),
                    "range_start": item.range.start,
                    "range_end": item.range.end,
                    "segments": segments
                }))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    json!({
        "fixture": fixture,
        "top_level_item_count": items.len(),
        "candidate_count": candidates.len(),
        "candidates": candidates
    })
}

fn collect_d2s_paths(root: &Path, paths: &mut Vec<PathBuf>, errors: &mut Vec<serde_json::Value>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(json!({
                "path": root.to_string_lossy(),
                "error": format!("Failed to read directory: {}", error)
            }));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(json!({
                    "path": root.to_string_lossy(),
                    "error": format!("Failed to enumerate directory entry: {}", error)
                }));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_d2s_paths(&path, paths, errors);
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("d2s"))
        {
            paths.push(path);
        }
    }
}

fn section_segment_witness_fixture_report(
    path: &Path,
) -> Result<serde_json::Value, serde_json::Value> {
    let fixture = path.to_string_lossy().to_string();
    let bytes = fs::read(path).map_err(|error| {
        json!({
            "fixture": fixture,
            "error": format!("Failed to read file: {}", error)
        })
    })?;
    let version_raw = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let is_alpha = version_raw == 105 || version_raw == 6;
    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes, &huffman, is_alpha).map_err(|error| {
        json!({
            "fixture": fixture,
            "error": format!("Failed to parse player items: {}", error)
        })
    })?;

    Ok(section_segment_witness_report(&items, &fixture))
}

fn section_segment_witness_directory_report(directory: &str) -> serde_json::Value {
    let root = Path::new(directory);
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    collect_d2s_paths(root, &mut paths, &mut errors);
    paths.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));

    d2r_core::init_rayon_thread_pool();
    let reports = paths
        .par_iter()
        .map(|path| section_segment_witness_fixture_report(path))
        .collect::<Vec<_>>();
    let mut fixtures = Vec::new();
    let mut candidate_fixture_count = 0;
    let mut candidate_count = 0;

    for result in reports {
        match result {
            Ok(report) => {
                let fixture_candidate_count = report
                    .get("candidate_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                if fixture_candidate_count > 0 {
                    candidate_fixture_count += 1;
                }
                candidate_count += fixture_candidate_count;
                fixtures.push(report);
            }
            Err(error) => errors.push(error),
        }
    }

    json!({
        "directory": directory,
        "fixture_count": fixtures.len(),
        "candidate_fixture_count": candidate_fixture_count,
        "candidate_count": candidate_count,
        "fixtures": fixtures,
        "errors": errors
    })
}

fn corpus_fixture_manifest(
    path: &Path,
    max_fixture_ms: Option<u64>,
) -> Result<serde_json::Value, serde_json::Value> {
    let start_time = std::time::Instant::now();
    let fixture = path.to_string_lossy().to_string();
    let bytes = fs::read(path).map_err(|error| {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        json!({
            "fixture": fixture,
            "status": "error",
            "elapsed_ms": elapsed_ms,
            "error": format!("Failed to read file: {}", error)
        })
    })?;
    let version_raw = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let is_alpha = version_raw == 105 || version_raw == 6;
    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes, &huffman, is_alpha).map_err(|error| {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        json!({
            "fixture": fixture,
            "status": "error",
            "elapsed_ms": elapsed_ms,
            "error": format!("Failed to parse player items: {}", error)
        })
    })?;

    let mut item_manifests = Vec::new();
    let mut fixture_admitted = 0usize;
    let mut fixture_unadmitted = 0usize;

    for (index, item) in items.iter().enumerate() {
        let range_start = item.range.start;
        let raw_len_bits = item.range.end.saturating_sub(range_start);
        let code_hint = item.code.trim().to_string();
        let mut carrier = d2r_core::domain::item::serialization::SegmentTraceCarrier::default();
        let geometry_operands =
            match d2r_core::domain::item::serialization::parse_item_at_with_limit_with_carrier(
                &bytes,
                range_start,
                0,
                &huffman,
                index,
                is_alpha,
                Some(raw_len_bits),
                Some(item.header.is_compact),
                Some(code_hint.as_str()),
                &mut carrier,
            ) {
                Ok(_) => {
                    let header_consumed_bits = carrier
                        .segments
                        .iter()
                        .find(|segment| segment.label == "Header")
                        .map(|segment| segment.end.saturating_sub(segment.start));
                    let code_consumed_bits = carrier
                        .segments
                        .iter()
                        .find(|segment| segment.label == "Code")
                        .map(|segment| segment.end.saturating_sub(segment.start));
                    let observed_extended_stats_start = carrier
                        .segments
                        .iter()
                        .find(|segment| segment.label == "ExtendedStats")
                        .map(|segment| range_start + segment.start);
                    (
                        header_consumed_bits,
                        code_consumed_bits,
                        observed_extended_stats_start,
                        Vec::new(),
                    )
                }
                Err(error) => (
                    None,
                    None,
                    None,
                    vec![format!("Bounded geometry reparse failed: {}", error)],
                ),
            };
        let (family_str, admitted, reason) = match LiveHeaderFamilyClassifier::classify(item) {
            Ok(family) => (format!("{:?}", family), true, serde_json::Value::Null),
            Err(err) => ("Unadmitted".to_string(), false, json!(err.to_string())),
        };
        let checksum_branch = if item.header.has_checksum {
            HeaderChecksumBranch::ChecksumAndVersion
        } else {
            HeaderChecksumBranch::VersionOnly
        };
        let nominal_expected_bits =
            LiveHeaderFamilyClassifier::classify(item)
                .ok()
                .and_then(|family| {
                    ExpectedHeaderWidthProducer::compute_expected_width(family, checksum_branch)
                        .ok()
                });
        if admitted {
            fixture_admitted += 1;
        } else {
            fixture_unadmitted += 1;
        }

        item_manifests.push(json!({
            "fixture": fixture,
            "section_item_index": index,
            "code": item.code.trim(),
            "range": {
                "start": item.range.start,
                "end": item.range.end,
            },
            "family": family_str,
            "admitted": admitted,
            "classification_reason": reason,
            "header_consumed_bits": geometry_operands.0,
            "checksum_branch": format!("{:?}", checksum_branch),
            "nominal_expected_bits": if admitted {
                nominal_expected_bits
            } else {
                None::<u64>
            },
            "observed_header_bits": geometry_operands.0,
            "comparison_status": match (nominal_expected_bits, geometry_operands.0) {
                (Some(nominal), Some(observed)) if nominal == observed => "match",
                (Some(_), Some(_)) => "mismatch",
                _ => "unavailable",
            },
            "availability": "not_assessed",
            "code_consumed_bits": geometry_operands.1,
            "observed_extended_stats_start": geometry_operands.2,
            "errors": geometry_operands.3,
        }));
    }

    let elapsed_ms = start_time.elapsed().as_millis() as u64;
    let status = if let Some(max_ms) = max_fixture_ms {
        if elapsed_ms > max_ms {
            "budget_exceeded"
        } else {
            "completed"
        }
    } else {
        "completed"
    };

    Ok(json!({
        "fixture": fixture,
        "status": status,
        "elapsed_ms": elapsed_ms,
        "item_count": items.len(),
        "admitted_count": fixture_admitted,
        "unadmitted_count": fixture_unadmitted,
        "items": item_manifests,
        "errors": Vec::<String>::new(),
    }))
}

fn corpus_manifest_directory_report(
    directory: &str,
    max_fixture_ms: Option<u64>,
    corpus_timeout_ms: Option<u64>,
) -> serde_json::Value {
    let overall_start = std::time::Instant::now();
    let root = Path::new(directory);
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    collect_d2s_paths(root, &mut paths, &mut errors);
    paths.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    let discovered_fixture_count = paths.len();

    d2r_core::init_rayon_thread_pool();
    let mut collection_status = None;
    let mut collection_diagnostic = None;
    let mut received_fixture_count = None;
    let mut reports = match corpus_timeout_ms {
        None => paths
            .par_iter()
            .map(|path| corpus_fixture_manifest(path, max_fixture_ms))
            .collect::<Vec<_>>(),
        Some(timeout_ms) => {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                paths.par_iter().for_each_with(sender, |sender, path| {
                    let _ = sender.send(corpus_fixture_manifest(path, max_fixture_ms));
                });
            });

            let deadline = overall_start + std::time::Duration::from_millis(timeout_ms);
            let mut reports = Vec::with_capacity(discovered_fixture_count);
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                match receiver.recv_timeout(remaining) {
                    Ok(report) => {
                        reports.push(report);
                        if reports.len() == discovered_fixture_count {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        received_fixture_count = Some(reports.len());
                        collection_status = Some("timeout");
                        collection_diagnostic = Some(if reports.is_empty() {
                            format!(
                                "Corpus manifest collection exceeded the requested timeout of {} ms; no fixture receipt arrived before the deadline.",
                                timeout_ms
                            )
                        } else {
                            format!(
                                "Corpus manifest collection exceeded the requested timeout of {} ms; retained {} fixture receipts delivered before the deadline.",
                                timeout_ms,
                                reports.len()
                            )
                        });
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        received_fixture_count = Some(reports.len());
                        if reports.len() < discovered_fixture_count {
                            collection_status = Some("error");
                            collection_diagnostic = Some(format!(
                                "Corpus manifest worker disconnected after {} fixture receipts; retained every receipt delivered before disconnect.",
                                reports.len()
                            ));
                        }
                        break;
                    }
                }
            }
            reports
        }
    };

    if corpus_timeout_ms.is_some() {
        reports.sort_by(|left, right| {
            let left_fixture = left
                .as_ref()
                .ok()
                .and_then(|report| report["fixture"].as_str())
                .unwrap_or("");
            let right_fixture = right
                .as_ref()
                .ok()
                .and_then(|report| report["fixture"].as_str())
                .unwrap_or("");
            left_fixture.cmp(right_fixture)
        });
    }

    let mut fixtures = Vec::new();
    let mut total_items = 0usize;
    let mut total_admitted = 0usize;
    let mut total_unadmitted = 0usize;
    let mut completed_fixtures = 0usize;
    let mut budget_exceeded_fixtures = 0usize;

    for result in reports {
        match result {
            Ok(report) => {
                let status = report["status"].as_str().unwrap_or("completed");
                if status == "budget_exceeded" {
                    budget_exceeded_fixtures += 1;
                } else {
                    completed_fixtures += 1;
                }

                let items_cnt = report["item_count"].as_u64().unwrap_or(0) as usize;
                let adm_cnt = report["admitted_count"].as_u64().unwrap_or(0) as usize;
                let unadm_cnt = report["unadmitted_count"].as_u64().unwrap_or(0) as usize;

                total_items += items_cnt;
                total_admitted += adm_cnt;
                total_unadmitted += unadm_cnt;

                fixtures.push(report);
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    let elapsed_ms = overall_start.elapsed().as_millis() as u64;

    let mut response = json!({
        "directory": directory,
        "total_fixtures": fixtures.len(),
        "completed_fixtures": completed_fixtures,
        "budget_exceeded_fixtures": budget_exceeded_fixtures,
        "fixture_error_count": errors.len(),
        "total_items": total_items,
        "admitted_items": total_admitted,
        "unadmitted_items": total_unadmitted,
        "elapsed_ms": elapsed_ms,
        "fixtures": fixtures,
        "errors": errors
    });

    if let Some(status) = collection_status {
        response["corpus_status"] = json!(status);
        response["corpus_timeout_ms"] = json!(corpus_timeout_ms);
        response["discovered_fixture_count"] = json!(discovered_fixture_count);
        response["received_fixture_count"] = json!(received_fixture_count.unwrap_or(0));
        response["diagnostic"] = json!(collection_diagnostic.unwrap_or_default());
    }

    response
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

    let recovery_signals = !scanner_hint.is_empty()
        && scanner_hint == final_code
        && normalized_code != final_code
        && gap_source.starts_with("normalization:");

    let replay_signals = gap_source == "header_gap_lookup"
        || is_kk_seam_drift
        || (!scanner_hint.is_empty()
            && scanner_hint == normalized_code
            && normalized_code == final_code
            && gap_len > 0);

    let ownership_hint = if recovery_signals {
        "recovery_path"
    } else if replay_signals && !padding_signals {
        "capture_replay"
    } else if padding_signals && !replay_signals {
        "emission_padding"
    } else {
        "ambiguous"
    };

    let ownership_reason = match ownership_hint {
        "recovery_path" => format!(
            "Recovered final_code='{}' from the scanner/header hint while normalized_code='{}' diverged via '{}'. Residual ambiguity is confined to the normalization track, not split ownership.",
            final_code, normalized_code, gap_source
        ),
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

fn main() {
    let mut parser = ArgParser::new("d2item_inspect")
        .description("Decomposes a .d2i or .d2s item into its bit-fields and props.");
    parser
        .add_arg("file", "Path to .d2i or .d2s file")
        .optional();
    parser.add_spec(ArgSpec::option(
        "corpus-dir",
        None,
        Some("corpus-dir"),
        "Directory containing .d2s files for batch scanning",
    ));
    parser.add_spec(ArgSpec::option(
        "dump-manifest",
        None,
        Some("dump-manifest"),
        "Output path for generated item manifest JSON",
    ));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));
    parser.add_spec(ArgSpec::option(
        "bit-offset",
        None,
        Some("bit-offset"),
        "Start parsing at specific bit offset",
    ));
    parser.add_spec(ArgSpec::flag(
        "trace-provenance",
        None,
        Some("trace-provenance"),
        "Trace item code provenance (scanner hint, normalized code, final code)",
    ));
    parser.add_spec(ArgSpec::flag(
        "trace-segments",
        None,
        Some("trace-segments"),
        "Expose parser semantic segments for a boundary-correct item parse",
    ));
    parser.add_spec(ArgSpec::option(
        "section-item",
        None,
        Some("section-item"),
        "Select a zero-based player-item section index after read_player_items parsing",
    ));
    parser.add_spec(ArgSpec::option(
        "marker-section",
        None,
        Some("marker-section"),
        "Select a one-based validity-filtered JM section for a receipt-only marker lookup",
    ));
    parser.add_spec(ArgSpec::option(
        "marker-ordinal",
        None,
        Some("marker-ordinal"),
        "Select a zero-based accepted marker ordinal within the selected JM section",
    ));
    parser.add_spec(ArgSpec::option(
        "marker-reparse-limit-bits",
        None,
        Some("marker-reparse-limit-bits"),
        "Opt-in bit limit for a bounded reparse from the selected marker envelope",
    ));
    parser.add_spec(ArgSpec::option(
        "reparse-limit-bits",
        None,
        Some("reparse-limit-bits"),
        "Opt-in bit limit for a selected-item bounded reparse receipt",
    ));
    parser.add_spec(ArgSpec::option(
        "compare-file",
        None,
        Some("compare-file"),
        "Compare the selected section item against a second .d2s fixture",
    ));
    parser.add_spec(ArgSpec::flag(
        "section-context",
        None,
        Some("section-context"),
        "Expose strict section-context rebuild comparison for a selected player-item entry",
    ));
    parser.add_spec(ArgSpec::flag(
        "section-segment-witnesses",
        None,
        Some("section-segment-witnesses"),
        "Enumerate player-item entries containing the claimed-gap segment pair",
    ));
    parser.add_spec(ArgSpec::flag(
        "emit-phase-trace",
        None,
        Some("emit-phase-trace"),
        "Expose bounded strict-emission phase provenance for a selected section item",
    ));
    parser.add_spec(ArgSpec::option(
        "coordinate-bit",
        None,
        Some("coordinate-bit"),
        "Crosswalk an absolute bit coordinate against the parsed item segments",
    ));
    parser.add_spec(ArgSpec::option(
        "max-fixture-ms",
        None,
        Some("max-fixture-ms"),
        "Maximum elapsed time in ms per fixture before budget_exceeded status",
    ));
    parser.add_spec(ArgSpec::option(
        "corpus-timeout-ms",
        None,
        Some("corpus-timeout-ms"),
        "Maximum overall corpus scan timeout limit in ms",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let file_opt = parsed.get("file");
    let corpus_dir_opt = parsed.get("corpus-dir");
    let dump_manifest_opt = parsed.get("dump-manifest");
    let max_fixture_ms = parsed
        .get("max-fixture-ms")
        .and_then(|s| s.parse::<u64>().ok());
    let corpus_timeout_ms = parsed
        .get("corpus-timeout-ms")
        .and_then(|s| s.parse::<u64>().ok());

    let is_corpus_mode = corpus_dir_opt.is_some();

    if is_corpus_mode && file_opt.is_some() {
        eprintln!(
            "error: Cannot specify both positional file and --corpus-dir\n\n{}",
            parser.usage()
        );
        std::process::exit(1);
    }
    if !is_corpus_mode && file_opt.is_none() {
        eprintln!(
            "error: Must specify either positional file or --corpus-dir\n\n{}",
            parser.usage()
        );
        std::process::exit(1);
    }
    if dump_manifest_opt.is_some() && !is_corpus_mode {
        eprintln!(
            "error: Option --dump-manifest requires --corpus-dir\n\n{}",
            parser.usage()
        );
        std::process::exit(1);
    }

    let is_json = parsed.is_json();

    if is_corpus_mode {
        let corpus_dir = corpus_dir_opt.unwrap();
        let manifest =
            corpus_manifest_directory_report(corpus_dir, max_fixture_ms, corpus_timeout_ms);

        if let Some(out_path) = dump_manifest_opt {
            if let Some(parent) = Path::new(out_path).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            let pretty_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
            if let Err(e) = fs::write(out_path, pretty_json) {
                eprintln!("Failed to write dump manifest to {}: {}", out_path, e);
                std::process::exit(1);
            }
            if !is_json {
                println!("Successfully dumped corpus item manifest to {}", out_path);
            }
        }

        if is_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&manifest).unwrap_or_default()
            );
        } else if dump_manifest_opt.is_none() {
            println!(
                "Corpus scan for directory '{}': {} fixtures, {} items (admitted: {}, unadmitted: {})",
                corpus_dir,
                manifest["total_fixtures"].as_u64().unwrap_or(0),
                manifest["total_items"].as_u64().unwrap_or(0),
                manifest["admitted_items"].as_u64().unwrap_or(0),
                manifest["unadmitted_items"].as_u64().unwrap_or(0)
            );
        }
        return;
    }

    let path = file_opt.unwrap();
    let bit_offset = parsed
        .get("bit-offset")
        .and_then(|s| s.parse::<usize>().ok());
    let trace_provenance = parsed.is_set("trace-provenance");
    let trace_segments = parsed.is_set("trace-segments");
    let section_item = parsed
        .get("section-item")
        .and_then(|s| s.parse::<usize>().ok());
    let marker_section_raw = parsed.get("marker-section");
    let marker_ordinal_raw = parsed.get("marker-ordinal");
    let marker_selector_requested = marker_section_raw.is_some() || marker_ordinal_raw.is_some();
    let marker_section = marker_section_raw.and_then(|s| s.parse::<usize>().ok());
    let marker_ordinal = marker_ordinal_raw.and_then(|s| s.parse::<usize>().ok());
    if marker_selector_requested
        && (marker_section.is_none()
            || marker_ordinal.is_none()
            || marker_section_raw.is_none()
            || marker_ordinal_raw.is_none())
    {
        if is_json {
            println!(
                "{}",
                json!({
                    "errors": [{
                        "kind": "selector_pair_required",
                        "message": "--marker-section and --marker-ordinal must be supplied as a complete numeric pair"
                    }]
                })
            );
        } else {
            eprintln!(
                "error: --marker-section and --marker-ordinal must be supplied as a complete numeric pair"
            );
        }
        return;
    }
    if marker_selector_requested && (section_item.is_some() || bit_offset.is_some()) {
        if is_json {
            println!(
                "{}",
                json!({
                    "errors": [{
                        "kind": "selector_mutually_exclusive",
                        "options": ["--marker-section", "--marker-ordinal"],
                        "conflicts_with": if section_item.is_some() { "--section-item" } else { "--bit-offset" }
                    }]
                })
            );
        } else {
            eprintln!(
                "error: --marker-section/--marker-ordinal cannot be combined with {}",
                if section_item.is_some() {
                    "--section-item"
                } else {
                    "--bit-offset"
                }
            );
        }
        return;
    }
    let compare_path = parsed.get("compare-file");
    let section_context = parsed.is_set("section-context");
    let section_segment_witnesses = parsed.is_set("section-segment-witnesses");
    let emit_phase_trace = parsed.is_set("emit-phase-trace");
    let coordinate_bit = parsed
        .get("coordinate-bit")
        .and_then(|s| s.parse::<u64>().ok());
    let reparse_limit_bits = match parsed.get("reparse-limit-bits") {
        Some(value) => match value.parse::<u64>() {
            Ok(limit) => Some(limit),
            Err(_) => {
                if is_json {
                    println!(
                        "{}",
                        json!({"errors": ["--reparse-limit-bits must be an unsigned integer"]})
                    );
                } else {
                    eprintln!("error: --reparse-limit-bits must be an unsigned integer");
                }
                return;
            }
        },
        None => None,
    };
    let marker_reparse_limit_bits = match parsed.get("marker-reparse-limit-bits") {
        Some(value) => match value.parse::<u64>() {
            Ok(limit) => Some(limit),
            Err(_) => {
                if is_json {
                    println!(
                        "{}",
                        json!({"errors": ["--marker-reparse-limit-bits must be an unsigned integer"]})
                    );
                } else {
                    eprintln!("error: --marker-reparse-limit-bits must be an unsigned integer");
                }
                return;
            }
        },
        None => None,
    };
    if marker_reparse_limit_bits.is_some()
        && !(is_json && marker_section.is_some() && marker_ordinal.is_some())
    {
        let message =
            "--marker-reparse-limit-bits requires --json and a complete marker selector pair";
        if is_json {
            println!("{}", json!({"errors": [message]}));
        } else {
            eprintln!("error: {}", message);
        }
        return;
    }
    if marker_reparse_limit_bits.is_some()
        && (reparse_limit_bits.is_some() || section_item.is_some() || bit_offset.is_some())
    {
        let message = "--marker-reparse-limit-bits cannot be combined with --reparse-limit-bits, --section-item, or --bit-offset";
        if is_json {
            println!("{}", json!({"errors": [message]}));
        } else {
            eprintln!("error: {}", message);
        }
        return;
    }
    if reparse_limit_bits.is_some() && !(is_json && trace_segments && section_item.is_some()) {
        let message = "--reparse-limit-bits requires --json, --trace-segments, and --section-item";
        if is_json {
            println!("{}", json!({"errors": [message]}));
        } else {
            eprintln!("error: {}", message);
        }
        return;
    }

    if section_segment_witnesses && is_json && Path::new(path).is_dir() {
        println!("{}", section_segment_witness_directory_report(path));
        return;
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if is_json {
                println!(
                    "{}",
                    json!({"errors": [format!("Failed to read file: {}", e)]})
                );
            } else {
                eprintln!("Failed to read file: {}", e);
            }
            return;
        }
    };
    let huffman = HuffmanTree::new();

    let version_raw = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let is_alpha = version_raw == 105 || version_raw == 6;

    if let (Some(marker_section), Some(marker_ordinal)) = (marker_section, marker_ordinal) {
        let valid_sections: Vec<usize> = d2r_core::save::find_jm_markers(&bytes)
            .into_iter()
            .filter(|&position| {
                d2r_core::domain::item::serialization::is_likely_jm_section_header(
                    &bytes, position, is_alpha, &huffman,
                )
            })
            .collect();
        let section_index = marker_section.checked_sub(1);
        let Some(section_index) = section_index.filter(|&index| index < valid_sections.len())
        else {
            if is_json {
                println!(
                    "{}",
                    json!({
                        "errors": [{
                            "kind": "unknown_marker_section",
                            "marker_section": marker_section,
                            "available_section_count": valid_sections.len()
                        }]
                    })
                );
            } else {
                eprintln!(
                    "error: marker section {} is unknown; available section count is {}",
                    marker_section,
                    valid_sections.len()
                );
            }
            return;
        };

        let section_marker_byte_offset = valid_sections[section_index];
        let section_end = valid_sections
            .get(section_index + 1)
            .copied()
            .unwrap_or(bytes.len());
        let declared_top_level_count = u16::from_le_bytes([
            bytes[section_marker_byte_offset + 2],
            bytes[section_marker_byte_offset + 3],
        ]);
        let envelope = match d2r_core::domain::item::scanner::preselect_marker_local_envelope(
            &bytes[section_marker_byte_offset..section_end],
            &huffman,
            is_alpha,
            section_marker_byte_offset as u64,
            (section_marker_byte_offset as u64) * 8,
            declared_top_level_count,
            marker_ordinal,
        ) {
            Ok(envelope) => envelope,
            Err(d2r_core::domain::item::scanner::MarkerPreselectionError::AcceptedMarkerOrdinalOutOfRange {
                requested,
                accepted_count,
            }) => {
                if is_json {
                    println!(
                        "{}",
                        json!({
                            "errors": [{
                                "kind": "marker_ordinal_out_of_range",
                                "marker_section": marker_section,
                                "marker_ordinal": requested,
                                "accepted_count": accepted_count
                            }]
                        })
                    );
                } else {
                    eprintln!(
                        "error: marker ordinal {} is out of range in section {}; accepted count is {}",
                        requested,
                        marker_section,
                        accepted_count
                    );
                }
                return;
            }
        };

        if let Some(requested_limit_bits) = marker_reparse_limit_bits {
            let mut carrier = d2r_core::domain::item::serialization::SegmentTraceCarrier::default();
            let bounded_parse =
                d2r_core::domain::item::serialization::parse_item_at_with_limit_with_carrier(
                    &bytes,
                    envelope.absolute_item_start_bit,
                    0,
                    &huffman,
                    marker_ordinal,
                    is_alpha,
                    Some(requested_limit_bits),
                    None,
                    Some(envelope.trimmed_code.as_str()),
                    &mut carrier,
                );
            let (error, parser_consumed_bits) = match bounded_parse {
                Ok((_, consumed_bits)) => (serde_json::Value::Null, Some(consumed_bits)),
                Err(error) => (json!(error.to_string()), None),
            };
            let status = match carrier.status {
                d2r_core::domain::item::serialization::ParseStatus::Success => "success",
                d2r_core::domain::item::serialization::ParseStatus::Failure => "failure",
            };
            let segments = carrier
                .segments
                .iter()
                .map(|segment| {
                    json!({
                        "label": segment.label,
                        "start": segment.start,
                        "end": segment.end,
                        "depth": segment.depth
                    })
                })
                .collect::<Vec<_>>();

            println!(
                "{}",
                json!({
                    "selector": {
                        "marker_section": marker_section,
                        "marker_ordinal": marker_ordinal
                    },
                    "envelope": envelope,
                    "bounded_reparse": {
                        "provenance": "marker_local_bounded_reparse",
                        "requested_limit_bits": requested_limit_bits,
                        "observed_range": {
                            "start": carrier.start_bit,
                            "end": carrier.final_bit
                        },
                        "status": status,
                        "error": error,
                        "parser_consumed_bits": parser_consumed_bits,
                        "segments": segments
                    },
                    "errors": []
                })
            );
        } else if is_json {
            println!(
                "{}",
                json!({
                    "selector": {
                        "marker_section": marker_section,
                        "marker_ordinal": marker_ordinal
                    },
                    "envelope": envelope,
                    "errors": []
                })
            );
        } else {
            println!("Receipt-only marker selection succeeded.");
        }
        return;
    }

    if let Some(offset) = bit_offset {
        if trace_segments {
            let trace_header =
                d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                    &bytes,
                    offset as u64,
                    Some(offset as u64),
                    &huffman,
                    is_alpha,
                    0,
                )
                .map(|peek| (peek.3.trim().to_string(), peek.6));
            let trace_limit = (bytes.len() as u64 * 8).saturating_sub(offset as u64);
            let trace_code_hint = trace_header.as_ref().map(|(code, _)| code.as_str());
            let trace_forced_compact = trace_header
                .as_ref()
                .and_then(|(_, is_compact)| (*is_compact).then_some(true));
            match d2r_core::domain::item::serialization::parse_item_at_with_limit(
                &bytes,
                offset as u64,
                0,
                &huffman,
                0,
                is_alpha,
                Some(trace_limit),
                trace_forced_compact,
                trace_code_hint,
            ) {
                Ok((item, bit_end)) => {
                    let trace_end = item.range.end;
                    let coordinate = coordinate_bit.map(|absolute| {
                        let item_local = (absolute >= item.range.start && absolute < trace_end)
                            .then(|| absolute - item.range.start);
                        let owner_segments = item_local
                            .map(|local| {
                                item.segments
                                    .iter()
                                    .filter(|segment| segment.start <= local && local < segment.end)
                                    .map(|segment| {
                                        json!({
                                            "label": segment.label,
                                            "start": segment.start,
                                            "end": segment.end,
                                            "depth": segment.depth
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        json!({
                            "absolute": absolute,
                            "item_local": item_local,
                            "owner_segments": owner_segments
                        })
                    });
                    let segments = item
                        .segments
                        .iter()
                        .map(|segment| {
                            json!({
                                "label": segment.label,
                                "start": segment.start,
                                "end": segment.end,
                                "depth": segment.depth
                            })
                        })
                        .collect::<Vec<_>>();
                    if is_json {
                        println!(
                            "{}",
                            json!({
                                "item": item_to_json(&item, None),
                                "range": {"start": item.range.start, "end": trace_end},
                                "parser_consumed_bits": bit_end,
                                "coordinate": coordinate,
                                "segments": segments
                            })
                        );
                    } else {
                        println!(
                            "Trace segments for '{}' bits {}-{}",
                            item.code.trim(),
                            item.range.start,
                            item.range.end
                        );
                        println!(
                            "{}",
                            serde_json::to_string_pretty(
                                &json!({"coordinate": coordinate, "segments": segments})
                            )
                            .unwrap_or_default()
                        );
                    }
                }
                Err(e) => {
                    if is_json {
                        println!("{}", json!({"errors": [e.to_string()]}));
                    } else {
                        eprintln!("Failed to trace item at offset {}: {}", offset, e);
                    }
                }
            }
            return;
        }
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let _ = reader.skip(offset as u32).unwrap_or(());
        match Item::from_reader(&mut reader, &huffman, is_alpha) {
            Ok(item) => {
                let bit_end = reader.position_in_bits().unwrap_or(0) as usize;

                let provenance = if trace_provenance && is_alpha {
                    let scanner_hint =
                        d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                            &bytes,
                            offset as u64,
                            Some(offset as u64),
                            &huffman,
                            true,
                            0,
                        )
                        .map(|p| p.3.trim().to_string())
                        .unwrap_or_default();

                    let (normalized_code, gap_len, gap_source) = {
                        let mut reader2 = BitReader::endian(Cursor::new(&bytes), LittleEndian);
                        let _ = reader2.skip(offset as u32).unwrap_or(());
                        let mut cursor = d2r_core::data::bit_cursor::BitCursor::new(&mut reader2);

                        let gap_override =
                            d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                                &bytes,
                                offset as u64,
                                Some(offset as u64),
                                &huffman,
                                true,
                                0,
                            )
                            .map(|p| {
                                let mut gap = p.8 as usize;
                                if p.5 == 7 && !p.6 {
                                    gap = gap.saturating_sub(45);
                                }
                                gap
                            });

                        let has_checksum_peek =
                            d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                                &bytes,
                                offset as u64,
                                Some(offset as u64),
                                &huffman,
                                true,
                                0,
                            )
                            .map(|p| p.9);

                        if let Ok((header, _, _)) =
                            d2r_core::domain::item::entity::parse_item_header(
                                &mut cursor,
                                true,
                                Some(scanner_hint.as_str()),
                                gap_override,
                                true,
                                None,
                                has_checksum_peek,
                                Some(offset as u64),
                            )
                        {
                            if header.is_compact {
                                cursor.base_pos = offset as u64;
                            }
                            let s_axiom = d2r_core::domain::stats::axiom::StatsAxiom::new(
                                header.version,
                                header.quality.unwrap_or(
                                    d2r_core::domain::item::quality::ItemQuality::Normal,
                                ),
                                true,
                            );
                            let is_ho = s_axiom.is_header_only(
                                header.flags,
                                Some(scanner_hint.as_str()).unwrap_or(""),
                            );

                            if is_ho {
                                (scanner_hint.clone(), 0usize, "header_only".to_string())
                            } else {
                                let gap_len = if scanner_hint.trim() == "buc"
                                    || matches!(header.version, 1)
                                {
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
                        let trimmed_code = item
                            .code
                            .trim_matches(|c: char| c.is_whitespace() || c == '\0');
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

                    let clean_code = |s: &str| {
                        s.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                            .to_string()
                    };
                    let clean_scanner = clean_code(&scanner_hint);
                    let clean_final = clean_code(&final_code);

                    // DIAGNOSTICS-CONTRACT: Exposes registry overrides to prevent AI reasoning desync.
                    // Do not remove this block unless performing structural refactoring of the diagnostics channel.
                    let reg = d2r_core::domain::forensic::registry::get_registry();
                    let reg_override = reg
                        .item_overrides
                        .as_ref()
                        .and_then(|overrides| {
                            overrides
                                .get(&clean_scanner)
                                .or_else(|| overrides.get(&clean_final))
                        })
                        .map(|map| json!(map))
                        .unwrap_or(serde_json::Value::Null);

                    Some(json!({
                        "scanner_hint": scanner_hint,
                        "normalized_code": normalized_code,
                        "final_code": final_code,
                        "gap_len": gap_len,
                        "gap_source": gap_source,
                        "emitter_bypass": emitter_bypass,
                        "ownership_hint": ownership_hint,
                        "ownership_reason": ownership_reason,
                        "registry_override": reg_override
                    }))
                } else {
                    None
                };

                if is_json {
                    println!(
                        "{}",
                        json!({ "item": item_to_json(&item, provenance), "errors": [], "range": {"start": offset, "end": bit_end} })
                    );
                } else {
                    println!(
                        "Parsed item at offset {}: '{}' bits {}-{} loc={} quality={:?}",
                        offset, item.code, offset, bit_end, item.location, item.header.quality
                    );
                    if let Some(ref prov) = provenance {
                        println!("  [PROVENANCE]");
                        println!(
                            "    Scanner Hint   : {}",
                            prov["scanner_hint"].as_str().unwrap_or("")
                        );
                        println!(
                            "    Normalized Code: {}",
                            prov["normalized_code"].as_str().unwrap_or("")
                        );
                        println!(
                            "    Final Code     : {}",
                            prov["final_code"].as_str().unwrap_or("")
                        );
                        println!(
                            "    Gap Len        : {}",
                            prov["gap_len"].as_u64().unwrap_or(0)
                        );
                        println!(
                            "    Gap Source     : {}",
                            prov["gap_source"].as_str().unwrap_or("")
                        );
                        println!(
                            "    Emitter Bypass : {}",
                            prov["emitter_bypass"].as_bool().unwrap_or(false)
                        );
                        println!(
                            "    Ownership Hint : {}",
                            prov["ownership_hint"].as_str().unwrap_or("")
                        );
                        println!(
                            "    Ownership Reason: {}",
                            prov["ownership_reason"].as_str().unwrap_or("")
                        );
                        if !prov["registry_override"].is_null() {
                            println!("    Registry Override: {:?}", prov["registry_override"]);
                        }
                    }
                    for prop in &item.properties {
                        println!(
                            "  Prop: id={} value={} param={} bits {}-{}",
                            prop.stat_id,
                            prop.raw_value,
                            prop.param,
                            prop.range.start,
                            prop.range.end
                        );
                    }
                }
            }
            Err(e) => {
                // DIAGNOSTICS-CONTRACT: Exposes registry overrides on parsing crash sites to triage geometry desync.
                // Do not remove this block unless performing structural refactoring of the diagnostics channel.
                let mut prescription = String::new();
                if is_alpha {
                    let peeked_code =
                        d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                            &bytes,
                            offset as u64,
                            Some(offset as u64),
                            &huffman,
                            true,
                            0,
                        )
                        .map(|p| {
                            p.3.split_whitespace()
                                .next()
                                .unwrap_or("")
                                .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                                .to_string()
                        });

                    if let Some(ref code) = peeked_code {
                        let reg = d2r_core::domain::forensic::registry::get_registry();
                        if let Some(overrides) = &reg.item_overrides {
                            if let Some(map) = overrides.get(code) {
                                prescription = format!(
                                    " [Prescription: Active Registry Override Detected for code '{}' at offset {}. Registry has overrides: {:?}. This might cause geometry parsing conflict/desync.]",
                                    code, offset, map
                                );
                            }
                        }
                    }
                }

                if is_json {
                    let err_msg = if prescription.is_empty() {
                        format!("Error at offset {}: {}", offset, e)
                    } else {
                        format!("Error at offset {}: {}{}", offset, e, prescription)
                    };
                    println!("{}", json!({ "errors": [err_msg] }));
                } else {
                    if prescription.is_empty() {
                        eprintln!("Error at offset {}: {}", offset, e);
                    } else {
                        eprintln!("Error at offset {}: {}{}", offset, e, prescription);
                    }
                    analyze_non_compact_item(&bytes, offset, &huffman);
                }
            }
        }
        return;
    }

    // 1. Try reading as player items (save file format)
    if let Ok(items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
        if section_segment_witnesses {
            if !is_json {
                eprintln!("--section-segment-witnesses requires --json");
                return;
            }
            println!("{}", section_segment_witness_report(&items, path));
            return;
        }

        if let Some(section_item_index) = section_item {
            if section_item_index >= items.len() {
                if is_json {
                    println!(
                        "{}",
                        json!({
                            "errors": [format!(
                                "Requested section item {} but only {} top-level items were parsed",
                                section_item_index,
                                items.len()
                            )]
                        })
                    );
                } else {
                    eprintln!(
                        "Requested section item {} but only {} top-level items were parsed",
                        section_item_index,
                        items.len()
                    );
                }
                return;
            }

            let item = &items[section_item_index];
            if section_context {
                let report = section_context_report(
                    item,
                    section_item_index,
                    &huffman,
                    is_alpha,
                    emit_phase_trace,
                    coordinate_bit,
                );
                if let Some(compare_path) = compare_path {
                    if !is_json {
                        eprintln!("--compare-file requires --json with --section-context");
                        return;
                    }

                    let compare_bytes = match fs::read(compare_path) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            println!(
                                "{}",
                                json!({
                                    "errors": [format!(
                                        "Failed to read comparison file: {}",
                                        error
                                    )]
                                })
                            );
                            return;
                        }
                    };
                    let compare_version_raw = if compare_bytes.len() >= 8 {
                        u32::from_le_bytes(compare_bytes[4..8].try_into().unwrap_or([0; 4]))
                    } else {
                        0
                    };
                    let compare_is_alpha = compare_version_raw == 105 || compare_version_raw == 6;
                    let compare_items =
                        match Item::read_player_items(&compare_bytes, &huffman, compare_is_alpha) {
                            Ok(items) => items,
                            Err(error) => {
                                println!(
                                    "{}",
                                    json!({
                                        "errors": [format!(
                                            "Failed to parse comparison file: {}",
                                            error
                                        )]
                                    })
                                );
                                return;
                            }
                        };
                    if section_item_index >= compare_items.len() {
                        println!(
                            "{}",
                            json!({
                                "errors": [format!(
                                    "Requested comparison section item {} but only {} top-level items were parsed",
                                    section_item_index,
                                    compare_items.len()
                                )]
                            })
                        );
                        return;
                    }

                    let comparison_item = &compare_items[section_item_index];
                    let comparison_report = section_context_report(
                        comparison_item,
                        section_item_index,
                        &huffman,
                        compare_is_alpha,
                        emit_phase_trace,
                        coordinate_bit,
                    );
                    let start_delta_bits =
                        item.range.start as i64 - comparison_item.range.start as i64;
                    let base_span_bits = item.range.end as i64 - item.range.start as i64;
                    let comparison_span_bits =
                        comparison_item.range.end as i64 - comparison_item.range.start as i64;
                    let span_delta_bits = base_span_bits - comparison_span_bits;
                    let end_delta_bits = item.range.end as i64 - comparison_item.range.end as i64;
                    println!(
                        "{}",
                        json!({
                            "base": report,
                            "comparison": comparison_report,
                            "start_delta_bits": start_delta_bits,
                            "span_delta_bits": span_delta_bits,
                            "end_delta_bits": end_delta_bits,
                            "delta_identity_holds": end_delta_bits == start_delta_bits + span_delta_bits
                        })
                    );
                    return;
                }
                if is_json {
                    println!("{}", report);
                } else {
                    println!(
                        "Section item {} '{}' bits {}-{}",
                        section_item_index,
                        item.code.trim(),
                        item.range.start,
                        item.range.end
                    );
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).unwrap_or_default()
                    );
                }
                return;
            }

            if trace_segments {
                let range_start = item.range.start;
                let range_end = item.range.end;
                let raw_len_bits = range_end.saturating_sub(range_start);
                let code_hint = item.code.trim().to_string();
                let forced_compact = Some(item.header.is_compact);

                if let Some(requested_limit_bits) = reparse_limit_bits {
                    let reparse_limit_bits = requested_limit_bits.min(raw_len_bits);
                    let mut carrier =
                        d2r_core::domain::item::serialization::SegmentTraceCarrier::default();
                    let bounded_parse = d2r_core::domain::item::serialization::parse_item_at_with_limit_with_carrier(
                        &bytes,
                        range_start,
                        0,
                        &huffman,
                        section_item_index,
                        is_alpha,
                        Some(reparse_limit_bits),
                        forced_compact,
                        Some(code_hint.as_str()),
                        &mut carrier,
                    );
                    let (error, parser_consumed_bits) = match bounded_parse {
                        Ok((_, consumed_bits)) => (serde_json::Value::Null, Some(consumed_bits)),
                        Err(error) => (json!(error.to_string()), None),
                    };
                    let status = match carrier.status {
                        d2r_core::domain::item::serialization::ParseStatus::Success => "success",
                        d2r_core::domain::item::serialization::ParseStatus::Failure => "failure",
                    };
                    let segments = carrier
                        .segments
                        .iter()
                        .map(|segment| {
                            json!({
                                "label": segment.label,
                                "start": segment.start,
                                "end": segment.end,
                                "depth": segment.depth
                            })
                        })
                        .collect::<Vec<_>>();

                    println!(
                        "{}",
                        json!({
                            "section_item_index": section_item_index,
                            "code": code_hint,
                            "selected_range": {"start": range_start, "end": range_end},
                            "raw_len_bits": raw_len_bits,
                            "bounded_reparse": {
                                "provenance": "selected_item_bounded_reparse",
                                "requested_limit_bits": requested_limit_bits,
                                "observed_range": {
                                    "start": carrier.start_bit,
                                    "end": carrier.final_bit
                                },
                                "status": status,
                                "error": error,
                                "parser_consumed_bits": parser_consumed_bits,
                                "segments": segments
                            }
                        })
                    );
                    return;
                }

                unsafe {
                    env::set_var("D2R_ITEM_TRACE", "1");
                }
                let bounded_parse = d2r_core::domain::item::serialization::parse_item_at_with_limit(
                    &bytes,
                    range_start,
                    0,
                    &huffman,
                    section_item_index,
                    is_alpha,
                    Some(raw_len_bits),
                    forced_compact,
                    Some(code_hint.as_str()),
                );
                unsafe {
                    env::remove_var("D2R_ITEM_TRACE");
                }

                match bounded_parse {
                    Ok((traced_item, parser_consumed_bits)) => {
                        let coordinate = coordinate_bit.map(|absolute| {
                            let item_local = (absolute >= range_start && absolute < range_end)
                                .then(|| absolute - range_start);
                            let owner_segments = item_local
                                .map(|local| {
                                    traced_item
                                        .segments
                                        .iter()
                                        .filter(|segment| {
                                            segment.start <= local && local < segment.end
                                        })
                                        .map(|segment| {
                                            json!({
                                                "label": segment.label,
                                                "start": segment.start,
                                                "end": segment.end,
                                                "depth": segment.depth
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            json!({
                                "absolute": absolute,
                                "item_local": item_local,
                                "owner_segments": owner_segments
                            })
                        });
                        let segments = traced_item
                            .segments
                            .iter()
                            .map(|segment| {
                                json!({
                                    "label": segment.label,
                                    "start": segment.start,
                                    "end": segment.end,
                                    "depth": segment.depth
                                })
                            })
                            .collect::<Vec<_>>();
                        let header_width_comparison =
                            match LiveHeaderFamilyClassifier::classify(&traced_item) {
                                Ok(family) => {
                                    let checksum_branch = if traced_item.header.has_checksum {
                                        HeaderChecksumBranch::ChecksumAndVersion
                                    } else {
                                        HeaderChecksumBranch::VersionOnly
                                    };
                                    let nominal_expected_bits =
                                        ExpectedHeaderWidthProducer::compute_expected_width(
                                            family,
                                            checksum_branch,
                                        )
                                        .ok();
                                    let observed_header_bits = traced_item
                                        .segments
                                        .iter()
                                        .find(|segment| segment.label == "Header")
                                        .map(|segment| segment.end.saturating_sub(segment.start));
                                    let comparison_status =
                                        match (nominal_expected_bits, observed_header_bits) {
                                            (Some(nominal), Some(observed))
                                                if nominal == observed =>
                                            {
                                                "match"
                                            }
                                            (Some(_), Some(_)) => "mismatch",
                                            _ => "unavailable",
                                        };
                                    json!({
                                        "family": format!("{:?}", family),
                                        "checksum_branch": format!("{:?}", checksum_branch),
                                        "nominal_expected_bits": nominal_expected_bits,
                                        "observed_header_bits": observed_header_bits,
                                        "comparison_status": comparison_status,
                                        "availability": "not_assessed"
                                    })
                                }
                                Err(error) => json!({
                                    "family": "Unadmitted",
                                    "checksum_branch": if traced_item.header.has_checksum {
                                        "ChecksumAndVersion"
                                    } else {
                                        "VersionOnly"
                                    },
                                    "nominal_expected_bits": serde_json::Value::Null,
                                    "observed_header_bits": traced_item
                                        .segments
                                        .iter()
                                        .find(|segment| segment.label == "Header")
                                        .map(|segment| segment.end.saturating_sub(segment.start)),
                                    "comparison_status": "unavailable",
                                    "availability": "not_assessed",
                                    "error": error.to_string()
                                }),
                            };

                        if is_json {
                            println!(
                                "{}",
                                json!({
                                    "section_item_index": section_item_index,
                                    "code": code_hint,
                                    "range": {"start": range_start, "end": range_end},
                                    "raw_len_bits": raw_len_bits,
                                    "parser_consumed_bits": parser_consumed_bits,
                                    "coordinate": coordinate,
                                    "segments": segments,
                                    "header_width_comparison": header_width_comparison
                                })
                            );
                        } else {
                            println!(
                                "Bounded trace for section item {} '{}' bits {}-{}",
                                section_item_index, code_hint, range_start, range_end
                            );
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&json!({
                                    "raw_len_bits": raw_len_bits,
                                    "parser_consumed_bits": parser_consumed_bits,
                                    "coordinate": coordinate,
                                    "segments": segments
                                }))
                                .unwrap_or_default()
                            );
                        }
                    }
                    Err(error) => {
                        if is_json {
                            println!(
                                "{}",
                                json!({
                                    "section_item_index": section_item_index,
                                    "code": code_hint,
                                    "range": {"start": range_start, "end": range_end},
                                    "raw_len_bits": raw_len_bits,
                                    "parser_consumed_bits": serde_json::Value::Null,
                                    "errors": [error.to_string()]
                                })
                            );
                        } else {
                            eprintln!(
                                "Failed bounded trace for section item {} at bits {}-{}: {}",
                                section_item_index, range_start, range_end, error
                            );
                        }
                    }
                }
                return;
            }

            if is_json {
                println!(
                    "{}",
                    json!({
                        "section_item_index": section_item_index,
                        "item": item_to_json(item, None),
                        "errors": []
                    })
                );
            } else {
                println!(
                    "Section item {}: '{}' range {}-{}",
                    section_item_index,
                    item.code.trim(),
                    item.range.start,
                    item.range.end
                );
            }
            return;
        }

        if is_json {
            let item_objs: Vec<_> = items.iter().map(|it| item_to_json(it, None)).collect();
            println!("{}", json!({"items": item_objs, "errors": []}));
            return;
        }

        println!(
            "Library parse recovered {} top-level items from player section",
            items.len()
        );
        for (i, item) in items.iter().enumerate() {
            println!(
                "Item {:2}: '{:4}' mode={} loc={} flags=0x{:08X} name={:?} children={} range={}-{}",
                i,
                item.code,
                item.header.mode,
                item.header.location,
                item.flags,
                item.personalized_player_name,
                item.socketed_items.len(),
                item.range.start,
                item.range.end
            );
            for prop in &item.properties {
                println!(
                    "  Prop: id={} value={} param={} bits {}-{}",
                    prop.stat_id, prop.raw_value, prop.param, prop.range.start, prop.range.end
                );
            }

            for (socket_index, child) in item.socketed_items.iter().enumerate() {
                println!(
                    "  socket {:2}: '{}' mode={} loc={}",
                    socket_index, child.code, child.mode, child.location
                );
            }
        }
        return;
    }

    // 2. Fallback: search for JM markers or treat as raw item
    let jm_pos =
        (0..bytes.len().saturating_sub(2)).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M');

    let (section_bytes, jm_offset_bits, item_count) = if let Some(pos) = jm_pos {
        let count = u16::from_le_bytes([
            bytes.get(pos + 2).cloned().unwrap_or(0),
            bytes.get(pos + 3).cloned().unwrap_or(0),
        ]);
        let next_jm = (pos + 4..bytes.len().saturating_sub(1))
            .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
            .unwrap_or(bytes.len());
        (&bytes[pos + 4..next_jm], (pos + 4) * 8, count)
    } else {
        (&bytes[..], 0, 1)
    };

    let section_bits = (section_bytes.len() * 8) as u64;
    let mut reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    let mut visible_items: Vec<(usize, usize, Item)> = Vec::new();
    let mut errors = Vec::new();
    let mut raw_index = 0usize;

    while reader.position_in_bits().unwrap_or(section_bits) < section_bits {
        let _ = reader.byte_align();
        let pos = reader.position_in_bits().unwrap_or(0);
        if pos >= section_bits {
            break;
        }
        let bit_start = jm_offset_bits + pos as usize;

        match Item::from_reader(&mut reader, &huffman, is_alpha) {
            Ok(item) => {
                let pos_end = reader.position_in_bits().unwrap_or(0);
                let bit_end = jm_offset_bits + pos_end as usize;
                if item.mode == 6 {
                    if let Some((_, _, parent)) = visible_items.last_mut() {
                        parent.socketed_items.push(item);
                    } else {
                        errors.push(format!(
                            "Error at raw item {}: socketed item without a parent",
                            raw_index
                        ));
                        break;
                    }
                } else {
                    visible_items.push((bit_start, bit_end, item));
                }
            }
            Err(e) => {
                if visible_items.len() >= item_count as usize {
                    break;
                }
                errors.push(format!("Error at raw item {}: {}", raw_index, e));
                if !is_json {
                    analyze_non_compact_item(&bytes, bit_start, &huffman);
                }
                break;
            }
        }
        raw_index += 1;
    }

    if is_json {
        let item_objs: Vec<_> = visible_items
            .iter()
            .map(|(_, _, it)| item_to_json(it, None))
            .collect();
        if item_objs.len() == 1 {
            println!("{}", json!({ "item": item_objs[0], "errors": errors }));
        } else {
            println!("{}", json!({ "items": item_objs, "errors": errors }));
        }
    } else {
        println!(
            "Parsed {} visible items from a section expecting {} top-level items",
            visible_items.len(),
            item_count
        );

        for (i, (bit_start, bit_end, item)) in visible_items.iter().enumerate() {
            println!(
                "Item {:2}: '{}' bits {}-{} loc={} socketed_children={}",
                i,
                item.code,
                bit_start,
                bit_end,
                item.location,
                item.socketed_items.len()
            );
            for (socket_index, child) in item.socketed_items.iter().enumerate() {
                println!(
                    "  socket {:2}: '{}' loc={}",
                    socket_index, child.code, child.location
                );
            }
        }
    }
}
