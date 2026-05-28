use anyhow::{Context, Result};
use d2r_core::domain::item::scanner::{scan_item_markers, ItemMarker, MarkerStatus};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::collections::HashSet;
use std::{env, fs};

#[derive(Debug, Clone)]
enum TimelineEvent {
    ScannerMarker(ItemMarker),
    ItemStart {
        code: String,
        is_residue: bool,
    },
    ItemEnd {
        code: String,
        is_residue: bool,
    },
}

#[derive(Debug, Clone)]
struct TimelineEntry {
    offset: u64,
    event: TimelineEvent,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_scanner_visualizer")
        .description("Timeline visualization of scanner markers vs parser boundaries");

    parser.add_spec(ArgSpec::positional("file", "Path to D2S save file").required());
    parser.add_spec(ArgSpec::flag("alpha", Some('a'), Some("alpha"), "Enable Alpha v105 mode"));

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
    let alpha_mode = parsed.is_set("alpha");

    let bytes = fs::read(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    let huffman = HuffmanTree::new();

    let jm_positions = find_jm_markers(&bytes);
    println!("Found {} JM sections", jm_positions.len());

    for (jm_idx, &pos) in jm_positions.iter().enumerate() {
        if bytes.len() < pos + 4 {
            continue;
        }
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);

        let next_pos = jm_positions.get(jm_idx + 1).cloned().unwrap_or(bytes.len());
        let section_bytes = &bytes[pos..next_pos];
        let section_bit_offset = (pos as u64) * 8;

        println!("\nSection {} (JM at byte {} | offset {} bits, count={})", jm_idx, pos, section_bit_offset, count);
        println!("{:=<80}", "");

        // 1. Scan markers (verbose=true to get all markers)
        let markers = scan_item_markers(
            section_bytes,
            &huffman,
            alpha_mode,
            section_bit_offset,
            Some(count),
            true, // verbose
        );

        // 2. Parse items
        let items = if count > 0 {
            Item::read_section(
                section_bytes,
                section_bit_offset,
                count,
                &huffman,
                alpha_mode,
                false,
            ).unwrap_or_default()
        } else {
            Vec::new()
        };

        // 3. Build Timeline
        let mut timeline = Vec::new();

        for marker in markers {
            let absolute_offset = section_bit_offset + marker.offset;
            timeline.push(TimelineEntry {
                offset: absolute_offset,
                event: TimelineEvent::ScannerMarker(marker),
            });
        }

        let mut parsed_ranges = Vec::new();
        for item in &items {
            let code = item.code.trim().to_string();
            let is_residue = item.is_residue();
            
            parsed_ranges.push((item.range.start, item.range.end));

            timeline.push(TimelineEntry {
                offset: item.range.start,
                event: TimelineEvent::ItemStart {
                    code: code.clone(),
                    is_residue,
                },
            });
            timeline.push(TimelineEntry {
                offset: item.range.end,
                event: TimelineEvent::ItemEnd {
                    code: code.clone(),
                    is_residue,
                },
            });
        }

        // Sort by offset. For same offset, prefer ItemStart > Marker > ItemEnd (arbitrary but consistent)
        timeline.sort_by(|a, b| {
            a.offset.cmp(&b.offset).then_with(|| {
                let rank = |e: &TimelineEvent| match e {
                    TimelineEvent::ItemStart { .. } => 0,
                    TimelineEvent::ScannerMarker(_) => 1,
                    TimelineEvent::ItemEnd { .. } => 2,
                };
                rank(&a.event).cmp(&rank(&b.event))
            })
        });

        // 4. Output Timeline
        println!("{:<10} | {:<12} | {:<30} | {:<20}", "Bit Offset", "Type", "Content", "Orphan Status");
        println!("{:-<80}", "");

        let mut active_items = HashSet::new();

        for entry in timeline {
            let mut orphan_status = "";
            let content: String;
            let type_str: &str;

            match &entry.event {
                TimelineEvent::ScannerMarker(m) => {
                    type_str = match m.status {
                        MarkerStatus::Accepted => "MARKER:ACC",
                        MarkerStatus::Rejected => "MARKER:REJ",
                        MarkerStatus::Phantom => "MARKER:PHN",
                    };
                    content = format!("code='{}', conf={}, score={}", m.code.trim(), m.confidence, m.score);

                    // Orphan Classification
                    let is_inside = parsed_ranges.iter().any(|(s, e)| entry.offset >= *s && entry.offset < *e);
                    
                    if m.status == MarkerStatus::Accepted {
                        let is_item_start = items.iter().any(|it| it.range.start == entry.offset);
                        if !is_item_start {
                            orphan_status = "[SKIPPED BY PARSER]";
                        }
                    } else if !is_inside {
                        orphan_status = "[OUTSIDE PARSED RANGE]";
                    }
                }
                TimelineEvent::ItemStart { code, is_residue } => {
                    type_str = if *is_residue { "RESIDUE:START" } else { "ITEM:START" };
                    content = format!("code='{}'", code);
                    active_items.insert(code.clone());
                }
                TimelineEvent::ItemEnd { code, is_residue } => {
                    type_str = if *is_residue { "RESIDUE:END" } else { "ITEM:END" };
                    content = format!("code='{}'", code);
                    active_items.remove(code);
                }
            }

            println!("{:<10} | {:<12} | {:<30} | {:<20}", entry.offset, type_str, content, orphan_status);
        }
    }

    Ok(())
}
