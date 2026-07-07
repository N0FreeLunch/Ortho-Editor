use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, is_plausible_item_header, peek_item_header_at};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::Cursor;

#[derive(Serialize, Clone)]
struct PropertyEntry {
    index: usize,
    bit_offset: u64,
    stat_id: u16,
    value: u64,
}

#[derive(Serialize, Clone)]
struct WidthCandidate {
    width: u32,
    score: i32,
    terminator_bit: Option<u64>,
    valid_props: usize,
    stop_reason: String,
}

#[derive(Serialize, Clone)]
struct HeatmapCandidate {
    bit_offset: u64,
    code: String,
    header_bits: u64,
    is_plausible: bool,
    overlap_with: Vec<u64>,
}

#[derive(Serialize, Clone)]
struct SimStep {
    bit_offset: u64,
    code: String,
    width: u32,
    terminator_bit: u64,
}

#[derive(Serialize, Clone)]
struct SimulationPath {
    start_bit: u64,
    items_survived: usize,
    steps: Vec<SimStep>,
    stop_reason: String,
}

#[derive(Serialize)]
struct FuzzerPayload {
    file: String,
    start_bit: u64,
    best_width: u32,
    candidates: Vec<WidthCandidate>,
    heatmap: Vec<HeatmapCandidate>,
    simulation: Vec<SimulationPath>,
    best_properties: Vec<PropertyEntry>,
}

/// Bitstream Structural Fuzzer & Analyzer for D2R Alpha v105
/// Rank bit-width candidates based on terminator alignment.
fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_structural_fuzzer")
        .description("Bitstream Structural Fuzzer & Analyzer for D2R Alpha v105\nRank bit-width candidates based on terminator alignment.");

    parser.add_arg("save_path", "Path to save file");
    parser.add_spec(
        ArgSpec::option("start-bit", None, Some("start-bit"), "Start bit offset").with_default("0"),
    );
    parser.add_spec(ArgSpec::flag(
        "brute",
        None,
        Some("brute"),
        "Brute-force scan for best start-bit within 1000 bits",
    ));
    parser.add_spec(ArgSpec::flag(
        "heatmap",
        None,
        Some("heatmap"),
        "Show candidate heatmap and overlap diagnostics",
    ));
    parser.add_spec(ArgSpec::flag(
        "simulate",
        None,
        Some("simulate"),
        "Run sequence-aware path simulation",
    ));
    parser.add_spec(
        ArgSpec::option(
            "width-range",
            None,
            Some("width-range"),
            "Bit width range to sweep (n..m)",
        )
        .with_default("10..25"),
    );
    parser.add_spec(
        ArgSpec::option(
            "max-props",
            None,
            Some("max-props"),
            "Maximum properties to read per width",
        )
        .with_default("64"),
    );

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let path = parsed.get("save_path").unwrap();
    let start_bit: u64 = parsed.get("start-bit").unwrap().parse()?;
    let brute_mode = parsed.is_set("brute");
    let show_json = parsed.is_json();
    let show_heatmap = parsed.is_set("heatmap");
    let show_simulate = parsed.is_set("simulate");

    let range_str = parsed.get("width-range").unwrap();
    let range_parts: Vec<&str> = range_str.split("..").collect();
    let (width_range_start, width_range_end) = if range_parts.len() == 2 {
        (
            range_parts[0].parse::<u32>()?,
            range_parts[1].parse::<u32>()?,
        )
    } else {
        anyhow::bail!("Invalid range format (expected n..m)");
    };

    let max_props: usize = parsed.get("max-props").unwrap().parse()?;

    let bytes = fs::read(path)?;
    let huffman = HuffmanTree::new();

    if !show_json {
        println!("[StructuralFuzzer] File: {}", path);
        println!("  Start Bit: {}", start_bit);
        println!("  Brute Mode: {}", brute_mode);
        println!("  Width Range: {}..{}", width_range_start, width_range_end);
        println!("  Max Props: {}", max_props);
    }

    let scan_range = if brute_mode { 1000 } else { 1 };
    let mut report_candidates = Vec::new();
    let mut heatmap_candidates = Vec::new();
    let mut simulation_paths = Vec::new();

    if show_heatmap || show_simulate || show_json {
        let scan_start = start_bit.saturating_sub(128);
        let scan_end = start_bit + 128;

        for bit in scan_start..scan_end {
            if let Some((
                mode,
                loc,
                _x,
                code,
                flags,
                ver,
                _compact,
                header_bits,
                _nudge,
                _has_checksum,
            )) = peek_item_header_at(&bytes, bit, &huffman, true, 0)
            {
                if is_plausible_item_header(mode, loc, code.as_bytes(), flags, ver, true) {
                    heatmap_candidates.push(HeatmapCandidate {
                        bit_offset: bit,
                        code: code.trim().to_string(),
                        header_bits,
                        is_plausible: true,
                        overlap_with: Vec::new(),
                    });
                }
            }
        }

        // Detect overlaps
        let candidates_copy = heatmap_candidates.clone();
        for h in &mut heatmap_candidates {
            let start1 = h.bit_offset;
            let end1 = start1 + h.header_bits;

            for other in &candidates_copy {
                if h.bit_offset == other.bit_offset {
                    continue;
                }
                let start2 = other.bit_offset;
                let end2 = start2 + other.header_bits;

                if (start1 < end2) && (start2 < end1) {
                    h.overlap_with.push(other.bit_offset);
                }
            }
        }
    }

    if show_simulate || show_json {
        for heatmap_entry in &heatmap_candidates {
            if simulation_paths.len() > 10 {
                break;
            } // limit paths to simulate
            let mut current_bit = heatmap_entry.bit_offset;
            let mut steps = Vec::new();
            let mut items_survived = 0;
            let mut stop_reason = "Sequence complete or reached limit".to_string();

            for _ in 0..16 {
                // Limit simulation to 16 items
                let header = peek_item_header_at(&bytes, current_bit, &huffman, true, 0);
                if header.is_none() {
                    stop_reason = "Invalid item header".to_string();
                    break;
                }
                let (mode, loc, _, code, flags, ver, _, header_bits, _, _has_checksum) =
                    header.unwrap();
                if !is_plausible_item_header(mode, loc, code.as_bytes(), flags, ver, true) {
                    stop_reason = format!("Implausible header at {}", current_bit);
                    break;
                }

                // Header bits for Alpha v105 items are typically ~60-80 bits.
                // Property stats follow.
                let stats_offset = 19;
                let prop_start = current_bit + header_bits + stats_offset;

                let mut best_step_width = 0;
                let mut best_step_score = -1;
                let mut best_step_term = 0;

                for width in width_range_start..=width_range_end {
                    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
                    if reader.skip(prop_start as u32).is_err() {
                        continue;
                    }

                    let mut found_term = false;
                    let mut props_read = 0;
                    let mut term_at = 0;

                    for p in 0..max_props {
                        let current_pos = prop_start + (p as u64 * width as u64);
                        let stat_id = reader.read_var::<u16>(9).unwrap_or(0x1FF);
                        if stat_id == 0x1FF {
                            found_term = true;
                            props_read = p;
                            term_at = current_pos;
                            break;
                        }
                        if reader.skip((width - 9) as u32).is_err() {
                            break;
                        }
                    }

                    if found_term {
                        let score = props_read as i32;
                        if score > best_step_score {
                            best_step_score = score;
                            best_step_width = width;
                            best_step_term = term_at;
                        }
                    }
                }

                if best_step_width == 0 {
                    stop_reason = format!("No valid width found for item at {}", current_bit);
                    break;
                }

                steps.push(SimStep {
                    bit_offset: current_bit,
                    code: code.trim().to_string(),
                    width: best_step_width,
                    terminator_bit: best_step_term,
                });
                items_survived += 1;

                // 3. Jump to next header
                // Alpha v105 Heuristic: Items are often slotted at 80-bit boundaries.
                let mut next_header_bit = 0;
                let mut found_next = false;

                // Try 80/160/etc bit slots first
                for slots in 1..=2 {
                    let slot_candidate = current_bit + (slots * 80);
                    if let Some((m, l, _, c, f, v, _, _, _, _)) =
                        peek_item_header_at(&bytes, slot_candidate, &huffman, true, 0)
                    {
                        if is_plausible_item_header(m, l, c.as_bytes(), f, v, true) {
                            next_header_bit = slot_candidate;
                            found_next = true;
                            break;
                        }
                    }
                }

                if !found_next {
                    let item_end = best_step_term + best_step_width as u64;
                    for bit in item_end..(item_end + 128) {
                        if let Some((m, l, _, c, f, v, _, _, _, _)) =
                            peek_item_header_at(&bytes, bit, &huffman, true, 0)
                        {
                            if is_plausible_item_header(m, l, c.as_bytes(), f, v, true) {
                                next_header_bit = bit;
                                found_next = true;
                                break;
                            }
                        }
                    }
                }

                if !found_next {
                    stop_reason = "No next header found via slot or scan".to_string();
                    break;
                }
                current_bit = next_header_bit;
            }

            simulation_paths.push(SimulationPath {
                start_bit: heatmap_entry.bit_offset,
                items_survived,
                steps,
                stop_reason,
            });
        }
        simulation_paths.sort_by(|a, b| b.items_survived.cmp(&a.items_survived));
    }

    let mut best_overall_width = 0;
    let mut best_overall_score = -1;
    let mut best_overall_start = start_bit;

    for current_start in start_bit..(start_bit + scan_range) {
        let mut candidates = Vec::new();

        for width in width_range_start..=width_range_end {
            let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
            if let Err(_) = reader.skip(current_start as u32) {
                continue;
            }

            let mut found_terminator = false;
            let mut props_read = 0;
            let mut terminator_at = None;
            let mut stop_reason = "Max props reached".to_string();

            for p in 0..max_props {
                let current_pos = current_start + (p as u64 * width as u64);
                let stat_id = match reader.read_var::<u16>(9) {
                    Ok(id) => id,
                    Err(_) => {
                        stop_reason = "EOF".to_string();
                        break;
                    }
                };

                if stat_id == 0x1FF && !found_terminator {
                    found_terminator = true;
                    props_read = p;
                    terminator_at = Some(current_pos);
                    stop_reason = "Terminator found".to_string();
                }

                if width < 9 {
                    stop_reason = "Width < 9".to_string();
                    break;
                }
                if let Err(_) = reader.read_var::<u64>((width - 9) as u32) {
                    stop_reason = "EOF in value".to_string();
                    break;
                }
            }

            let score = if found_terminator {
                props_read as i32 + 100 // Bonus for having a terminator
            } else {
                0
            };

            candidates.push(WidthCandidate {
                width,
                score,
                terminator_bit: terminator_at,
                valid_props: props_read,
                stop_reason,
            });
        }

        // Sort candidates for this start_bit: score desc, then width asc (tie-break)
        candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.width.cmp(&b.width)));

        if let Some(best) = candidates.first() {
            if best.score > best_overall_score {
                best_overall_score = best.score;
                best_overall_width = best.width;
                best_overall_start = current_start;
            }
        }

        if !brute_mode {
            report_candidates = candidates;
        }
    }

    if show_json {
        let mut best_props = Vec::new();
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let _ = reader.skip(best_overall_start as u32);
        for p in 0..max_props {
            let current_pos = best_overall_start + (p as u64 * best_overall_width as u64);
            let stat_id = match reader.read_var::<u16>(9) {
                Ok(id) => id,
                Err(_) => break,
            };
            if stat_id == 0x1FF {
                break;
            }
            let value_bits = (best_overall_width - 9) as u32;
            let value = reader.read_var::<u64>(value_bits).unwrap_or(0);
            best_props.push(PropertyEntry {
                index: p,
                bit_offset: current_pos,
                stat_id,
                value,
            });
        }

        let payload = FuzzerPayload {
            file: path.clone(),
            start_bit: best_overall_start,
            best_width: best_overall_width,
            candidates: report_candidates,
            heatmap: heatmap_candidates,
            simulation: simulation_paths,
            best_properties: best_props,
        };
        let metadata =
            ReportMetadata::new("d2item_structural_fuzzer", path, env!("CARGO_PKG_VERSION"));
        let report = Report::new(metadata, ReportStatus::Ok).with_results(payload);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        if show_heatmap && !brute_mode {
            println!("\nCandidate Heatmap:");
            println!("---------------------------------------------------------------------------");
            println!("| Bit Offset | Code | Header Bits | Status    | Overlaps                  |");
            println!("---------------------------------------------------------------------------");
            for h in &heatmap_candidates {
                let status = if h.bit_offset == start_bit {
                    "TARGET*"
                } else {
                    "FOUND"
                };
                let overlaps_str = if h.overlap_with.is_empty() {
                    "none".to_string()
                } else {
                    h.overlap_with
                        .iter()
                        .map(|o: &u64| o.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                println!(
                    "| {:10} | {:4} | {:11} | {:9} | {:25} |",
                    h.bit_offset, h.code, h.header_bits, status, overlaps_str
                );
            }
            println!("---------------------------------------------------------------------------");
        }

        if show_simulate && !brute_mode {
            println!("\nSequence Simulation (Best Path Candidate):");
            if let Some(best_path) = simulation_paths.first() {
                println!(
                    "  Winner: Start Bit {}, Survived {} items",
                    best_path.start_bit, best_path.items_survived
                );
                println!("  Stop Reason: {}", best_path.stop_reason);
                println!(
                    "---------------------------------------------------------------------------"
                );
                println!("| Step | Bit Offset | Code | Width | Terminator Bit |");
                println!(
                    "---------------------------------------------------------------------------"
                );
                for (i, s) in best_path.steps.iter().enumerate() {
                    println!(
                        "| {:4} | {:10} | {:4} | {:5} | {:14} |",
                        i, s.bit_offset, s.code, s.width, s.terminator_bit
                    );
                }
                println!(
                    "---------------------------------------------------------------------------"
                );
            } else {
                println!("  No simulation paths found.");
            }
        }

        if !brute_mode {
            println!("\nWidth Candidate Table (Start Bit: {}):", start_bit);
            println!("---------------------------------------------------------------------------");
            println!("| Width | Score | Terminator Bit | Valid Props | Stop Reason              |");
            println!("---------------------------------------------------------------------------");
            for c in &report_candidates {
                let term_str = match c.terminator_bit {
                    Some(b) => format!("{:14}", b),
                    None => "none          ".to_string(),
                };
                println!(
                    "| {:5} | {:5} | {} | {:11} | {:24} |",
                    c.width, c.score, term_str, c.valid_props, c.stop_reason
                );
            }
            println!("---------------------------------------------------------------------------");
        }

        if best_overall_width > 0 {
            println!("\nBest Fit Width: {}", best_overall_width);
            if brute_mode {
                println!("Best Start Bit: {}", best_overall_start);
            }
            println!("------------------------------------------------------------------");
            println!("| Index | Bit Offset | Stat ID (Hex/Dec) | Raw Value (Hex/Dec)   |");
            println!("------------------------------------------------------------------");
            let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
            if let Err(_) = reader.skip(best_overall_start as u32) {
                println!("Error re-reading best fit width.");
            } else {
                for p in 0..max_props {
                    let current_pos = best_overall_start + (p as u64 * best_overall_width as u64);
                    let stat_id = match reader.read_var::<u16>(9) {
                        Ok(id) => id,
                        Err(_) => break,
                    };

                    if stat_id == 0x1FF {
                        println!(
                            "| {:5} | {:10} | [ TERMINATOR ]    |                       |",
                            p, current_pos
                        );
                        break;
                    } else {
                        let value_bits = (best_overall_width - 9) as u32;
                        let value = reader.read_var::<u64>(value_bits).unwrap_or(0);

                        println!(
                            "| {:5} | {:10} | {:#05x} ({:4})    | {:#010x} ({:10}) |",
                            p, current_pos, stat_id, stat_id, value, value
                        );
                    }
                }
            }
            println!("------------------------------------------------------------------");
        } else {
            println!("\nNo valid width found with a terminator.");
        }
    }

    Ok(())
}
