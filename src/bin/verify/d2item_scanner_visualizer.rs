use anyhow::{bail, Context, Result};
use colored::Colorize;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::stats::lookup_alpha_map_by_raw;
use d2r_core::domain::item::scanner::{scan_item_markers, ItemMarker, MarkerStatus};
use d2r_core::item::{HuffmanTree, Item, ItemProperty};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::{env, fs};

const VISUAL_WIDTH: usize = 72;

#[derive(Debug, Clone, Serialize)]
struct VisualizerReport {
    save_file: String,
    alpha_mode: bool,
    sections: Vec<SectionReport>,
}

#[derive(Debug, Clone, Serialize)]
struct SectionReport {
    index: usize,
    jm_offset: u64,
    next_jm_offset: u64,
    section_bit_offset: u64,
    section_end_bit: u64,
    expected_count: u16,
    parse_error: Option<String>,
    registry_nudges: Vec<u64>,
    summary: SectionSummary,
    timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct SectionSummary {
    accepted_markers: usize,
    rejected_markers: usize,
    phantom_markers: usize,
    items: usize,
    residue_items: usize,
    marker_inside_item_range: usize,
    accepted_markers_outside_range: usize,
    item_starts_without_accepted_marker: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TimelineEntry {
    offset: u64,
    #[serde(flatten)]
    event: TimelineEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TimelineEvent {
    ScannerMarker {
        code: String,
        confidence: u32,
        score: i32,
        status: MarkerStatus,
        orphan_status: Option<String>,
        registry_tags: Vec<String>,
    },
    ItemStart {
        code: String,
        is_residue: bool,
        range_start: u64,
        range_end: u64,
        registry_tags: Vec<String>,
        stat_tags: Vec<String>,
        stat_validation_ok: bool,
    },
    ItemEnd {
        code: String,
        is_residue: bool,
        range_start: u64,
        range_end: u64,
        registry_tags: Vec<String>,
        stat_tags: Vec<String>,
        stat_validation_ok: bool,
    },
}

#[derive(Debug, Clone)]
struct SectionContext {
    index: usize,
    jm_offset: usize,
    next_jm_offset: usize,
    section_bit_offset: u64,
    section_end_bit: u64,
    expected_count: u16,
    markers: Vec<ItemMarker>,
    items: Vec<Item>,
    parse_error: Option<String>,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_scanner_visualizer")
        .description("Timeline visualization of scanner markers vs parser boundaries");

    parser.add_spec(ArgSpec::positional("file", "Path to D2S save file").required());
    parser.add_spec(ArgSpec::flag("alpha", Some('a'), Some("alpha"), "Enable Alpha v105 mode"));
    parser.add_spec(ArgSpec::flag(
        "visual-gap",
        None,
        Some("visual-gap"),
        "Render a colored gap grid alongside the timeline",
    ));
    parser.add_spec(ArgSpec::flag(
        "json-timeline",
        None,
        Some("json-timeline"),
        "Emit the scanner/parser timeline as JSON",
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
    let alpha_mode = parsed.is_set("alpha");
    let visual_gap = parsed.is_set("visual-gap");
    let json_timeline = parsed.is_set("json-timeline");

    if visual_gap && json_timeline {
        bail!("--visual-gap and --json-timeline cannot be used together");
    }

    let bytes = fs::read(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    let huffman = HuffmanTree::new();
    let registry = get_registry();
    let jm_positions = find_jm_markers(&bytes);
    let section_reports = build_sections(&bytes, &huffman, registry, alpha_mode, &jm_positions);

    if json_timeline {
        let report = VisualizerReport {
            save_file: file_path.to_string(),
            alpha_mode,
            sections: section_reports,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    print_text_report(file_path, &bytes, alpha_mode, visual_gap, &section_reports);
    Ok(())
}

fn build_sections(
    bytes: &[u8],
    huffman: &HuffmanTree,
    registry: &d2r_core::domain::forensic::registry::AlphaForensics,
    alpha_mode: bool,
    jm_positions: &[usize],
) -> Vec<SectionReport> {
    jm_positions
        .iter()
        .enumerate()
        .map(|(index, &jm_offset)| {
            let next_jm_offset = jm_positions.get(index + 1).copied().unwrap_or(bytes.len());
            let section_bytes = &bytes[jm_offset..next_jm_offset];
            let section_bit_offset = (jm_offset as u64) * 8;
            let section_end_bit = (next_jm_offset as u64) * 8;
            let expected_count = if bytes.len() >= jm_offset + 4 {
                u16::from_le_bytes([bytes[jm_offset + 2], bytes[jm_offset + 3]])
            } else {
                0
            };

            let markers = scan_item_markers(
                section_bytes,
                huffman,
                alpha_mode,
                section_bit_offset,
                Some(expected_count),
                true,
            );

            let (items, parse_error) = match Item::read_section(
                section_bytes,
                section_bit_offset,
                expected_count,
                huffman,
                alpha_mode,
                false,
            ) {
                Ok(items) => (items, None),
                Err(err) => (Vec::new(), Some(err.to_string())),
            };

            let section_parse_error = parse_error.clone();
            let context = SectionContext {
                index,
                jm_offset,
                next_jm_offset,
                section_bit_offset,
                section_end_bit,
                expected_count,
                markers,
                items,
                parse_error,
            };

            let timeline = build_timeline(&context, registry);
            let summary = summarize_section(&context);

            let nudges = registry
                .scanner_nudges
                .as_ref()
                .map(|map| {
                    let mut values = map.values().copied().collect::<Vec<_>>();
                    values.sort_unstable();
                    values
                })
                .unwrap_or_default();

            SectionReport {
                index,
                jm_offset: jm_offset as u64,
                next_jm_offset: next_jm_offset as u64,
                section_bit_offset,
                section_end_bit,
                expected_count,
                parse_error: section_parse_error,
                registry_nudges: nudges,
                summary,
                timeline,
            }
        })
        .collect()
}

fn build_timeline(
    context: &SectionContext,
    registry: &d2r_core::domain::forensic::registry::AlphaForensics,
) -> Vec<TimelineEntry> {
    let mut timeline = Vec::new();
    let mut parsed_ranges = Vec::new();

    for item in &context.items {
        parsed_ranges.push((item.range.start, item.range.end));
        let registry_tags = build_registry_tags(&item.code, registry);
        let (stat_tags, stat_validation_ok) = build_stat_validation_tags(item);
        timeline.push(TimelineEntry {
            offset: item.range.start,
            event: TimelineEvent::ItemStart {
                code: item.code.trim().to_string(),
                is_residue: item.is_residue(),
                range_start: item.range.start,
                range_end: item.range.end,
                registry_tags: registry_tags.clone(),
                stat_tags: stat_tags.clone(),
                stat_validation_ok,
            },
        });
        timeline.push(TimelineEntry {
            offset: item.range.end,
            event: TimelineEvent::ItemEnd {
                code: item.code.trim().to_string(),
                is_residue: item.is_residue(),
                range_start: item.range.start,
                range_end: item.range.end,
                registry_tags,
                stat_tags,
                stat_validation_ok,
            },
        });
    }

    for marker in &context.markers {
        let absolute_offset = context.section_bit_offset + marker.offset;
        let orphan_status = marker_orphan_status(marker, absolute_offset, &context.items, &parsed_ranges);
        timeline.push(TimelineEntry {
            offset: absolute_offset,
            event: TimelineEvent::ScannerMarker {
                code: marker.code.trim().to_string(),
                confidence: marker.confidence,
                score: marker.score,
                status: marker.status,
                orphan_status,
                registry_tags: build_registry_tags(&marker.code, registry),
            },
        });
    }

    timeline.sort_by(|a, b| {
        a.offset.cmp(&b.offset).then_with(|| {
            let rank = |e: &TimelineEvent| match e {
                TimelineEvent::ItemStart { .. } => 0,
                TimelineEvent::ScannerMarker { .. } => 1,
                TimelineEvent::ItemEnd { .. } => 2,
            };
            rank(&a.event).cmp(&rank(&b.event))
        })
    });

    timeline
}

fn summarize_section(context: &SectionContext) -> SectionSummary {
    let accepted_markers = context
        .markers
        .iter()
        .filter(|m| m.status == MarkerStatus::Accepted)
        .count();
    let rejected_markers = context
        .markers
        .iter()
        .filter(|m| m.status == MarkerStatus::Rejected)
        .count();
    let phantom_markers = context
        .markers
        .iter()
        .filter(|m| m.status == MarkerStatus::Phantom)
        .count();

    let item_ranges: Vec<(u64, u64)> = context.items.iter().map(|item| (item.range.start, item.range.end)).collect();
    let marker_inside_item_range = context
        .markers
        .iter()
        .filter(|marker| {
            let absolute_offset = context.section_bit_offset + marker.offset;
            item_ranges
                .iter()
                .any(|(start, end)| absolute_offset >= *start && absolute_offset < *end)
        })
        .count();
    let accepted_markers_outside_range = context
        .markers
        .iter()
        .filter(|marker| {
            marker.status == MarkerStatus::Accepted
                && !item_ranges.iter().any(|(start, end)| {
                    let absolute_offset = context.section_bit_offset + marker.offset;
                    absolute_offset >= *start && absolute_offset < *end
                })
        })
        .count();

    let item_starts_without_accepted_marker = context
        .items
        .iter()
        .filter(|item| {
            !context.markers.iter().any(|marker| {
                marker.status == MarkerStatus::Accepted
                    && (context.section_bit_offset + marker.offset) == item.range.start
            })
        })
        .count();

    SectionSummary {
        accepted_markers,
        rejected_markers,
        phantom_markers,
        items: context.items.len(),
        residue_items: context.items.iter().filter(|item| item.is_residue()).count(),
        marker_inside_item_range,
        accepted_markers_outside_range,
        item_starts_without_accepted_marker,
    }
}

fn marker_orphan_status(
    marker: &ItemMarker,
    absolute_offset: u64,
    items: &[Item],
    parsed_ranges: &[(u64, u64)],
) -> Option<String> {
    let is_inside = parsed_ranges
        .iter()
        .any(|(start, end)| absolute_offset >= *start && absolute_offset < *end);
    let is_item_start = items.iter().any(|item| item.range.start == absolute_offset);

    if marker.status == MarkerStatus::Accepted && !is_item_start {
        return Some("[SKIPPED BY PARSER]".to_string());
    }
    if !is_inside {
        return Some("[OUTSIDE PARSED RANGE]".to_string());
    }
    None
}

fn build_registry_tags(code: &str, registry: &d2r_core::domain::forensic::registry::AlphaForensics) -> Vec<String> {
    let trimmed = code.trim();
    let mut tags = Vec::new();

    if let Some(effective) = registry.effective_codes.get(trimmed) {
        if effective.trim() != trimmed {
            tags.push(format!("effective={}", effective.trim()));
        }
    }

    if registry
        .forced_compact_codes
        .as_ref()
        .map(|codes| codes.iter().any(|c| c == trimmed))
        .unwrap_or(false)
    {
        tags.push("forced_compact".to_string());
    }

    if registry
        .forced_runeword_codes
        .as_ref()
        .map(|codes| codes.iter().any(|c| c == trimmed))
        .unwrap_or(false)
    {
        tags.push("forced_runeword".to_string());
    }

    if registry
        .force_summary_rhythm_codes
        .as_ref()
        .map(|codes| codes.iter().any(|c| c == trimmed))
        .unwrap_or(false)
    {
        tags.push("summary_rhythm".to_string());
    }

    if let Some(overrides) = &registry.item_overrides {
        if let Some(item_map) = overrides.get(trimmed) {
            for key in ["fixed_width", "header_gap", "is_compact", "is_shadow", "is_authority_overlap"] {
                if let Some(value) = item_map.get(key) {
                    tags.push(format!("override:{}={}", key, value));
                }
            }
        }
    }

    if let Some(nudge) = registry.scanner_nudges.as_ref().and_then(|nudges| nudges.get(trimmed)) {
        tags.push(format!("scanner_nudge={}", nudge));
    }

    tags
}

fn build_stat_validation_tags(item: &Item) -> (Vec<String>, bool) {
    let mut tags = Vec::new();
    let mut all_mapped = true;

    for (source, prop) in collect_item_properties(item) {
        if let Some(mapping) = lookup_alpha_map_by_raw(prop.stat_id) {
            let expected = mapping.name.trim();
            let actual = prop.name.trim();
            let mut tag = format!("{source}:{}->{}", prop.stat_id, expected);
            if let Some(bits) = mapping.save_bits {
                tag.push_str(&format!("({}b)", bits));
            }
            if !actual.is_empty() && actual != expected {
                tag.push_str(&format!(" [name={}]", actual));
            }
            tags.push(tag);
        } else {
            all_mapped = false;
            if prop.name.trim().is_empty() {
                tags.push(format!("{source}:{}->unmapped", prop.stat_id));
            } else {
                tags.push(format!("{source}:{}->{} [unmapped]", prop.stat_id, prop.name.trim()));
            }
        }
    }

    (tags, all_mapped)
}

fn collect_item_properties(item: &Item) -> Vec<(String, &ItemProperty)> {
    let mut properties = Vec::new();

    for prop in &item.properties {
        properties.push(("properties".to_string(), prop));
    }
    for (index, list) in item.set_attributes.iter().enumerate() {
        for prop in list {
            properties.push((format!("set[{}]", index), prop));
        }
    }
    for prop in &item.runeword_attributes {
        properties.push(("runeword".to_string(), prop));
    }

    properties
}

fn print_text_report(
    file_path: &str,
    bytes: &[u8],
    alpha_mode: bool,
    visual_gap: bool,
    section_reports: &[SectionReport],
) {
    println!("Found {} JM sections", section_reports.len());

    for report in section_reports {
        let section_size_bits = report.section_end_bit.saturating_sub(report.section_bit_offset);
        let section_label = section_label(report.index);
        println!(
            "\nSection {} ({section_label} | JM at byte {} | offset {} bits, count={})",
            report.index,
            report.jm_offset,
            report.section_bit_offset,
            report.expected_count
        );
        if let Some(err) = &report.parse_error {
            println!("  [ERROR] Failed to parse items: {}", err);
        }
        println!("{:=<80}", "");
        println!(
            "{:<10} | {:<12} | {:<30} | {:<18} | {:<20}",
            "Bit Offset", "Type", "Content", "Visual", "Orphan Status"
        );
        println!("{:-<120}", "");

        for entry in &report.timeline {
            let (type_str, content, orphan_status, visual, style) = render_text_entry(
                entry,
                report.section_bit_offset,
                report.section_end_bit,
                visual_gap,
                alpha_mode,
            );

            let visual_cell = if visual_gap {
                visual
            } else {
                String::new()
            };

            let line = format!(
                "{:<10} | {:<12} | {:<30} | {:<18} | {:<20}",
                entry.offset, type_str, content, visual_cell, orphan_status
            );

            if let Some(style_fn) = style {
                println!("{}", style_fn(&line));
            } else {
                println!("{}", line);
            }
        }

        if visual_gap {
            println!("\n    [GAP GRID]");
            for entry in &report.timeline {
                let (label, bar, style) = render_gap_bar(entry, report.section_bit_offset, report.section_end_bit);
                if let Some(style_fn) = style {
                    println!("    {:<16} {}", label, style_fn(&bar));
                } else {
                    println!("    {:<16} {}", label, bar);
                }
            }
        }

        println!("\n    [SUMMARY]");
        println!(
            "    accepted={} rejected={} phantom={} items={} residue={}",
            report.summary.accepted_markers,
            report.summary.rejected_markers,
            report.summary.phantom_markers,
            report.summary.items,
            report.summary.residue_items
        );
        println!(
            "    marker_inside_item_range={} accepted_markers_outside_range={} item_starts_without_accepted_marker={}",
            report.summary.marker_inside_item_range,
            report.summary.accepted_markers_outside_range,
            report.summary.item_starts_without_accepted_marker
        );
        if !report.registry_nudges.is_empty() {
            println!("    registry scanner nudges: {:?}", report.registry_nudges);
        }
        if report.section_end_bit > report.section_bit_offset {
            println!(
                "    section span: {} bits ({} bytes)",
                section_size_bits,
                section_size_bits / 8
            );
        }
    }

    if section_reports.is_empty() {
        println!("  [WARN] No JM markers found in file {}", file_path);
    }

    println!("\n[SUMMARY]");
    println!("  Total JM sections: {}", section_reports.len());
    if let Some(first) = section_reports.first() {
        println!("  Header + pre-item data: {} bytes", first.jm_offset);
    }
    if !bytes.is_empty() && alpha_mode {
        println!("  Alpha mode: enabled");
    }
}

fn render_text_entry(
    entry: &TimelineEntry,
    section_start: u64,
    section_end: u64,
    visual_gap: bool,
    _alpha_mode: bool,
) -> (String, String, String, String, Option<fn(&str) -> String>) {
    let mut orphan_status = String::new();
    let mut type_str = String::new();
    let mut content = String::new();
    let mut visual = String::new();
    let mut style: Option<fn(&str) -> String> = None;

    match &entry.event {
        TimelineEvent::ScannerMarker {
            code,
            confidence,
            score,
            status,
            orphan_status: marker_orphan,
            registry_tags,
        } => {
            type_str = match status {
                MarkerStatus::Accepted => "MARKER:ACC".to_string(),
                MarkerStatus::Rejected => "MARKER:REJ".to_string(),
                MarkerStatus::Phantom => "MARKER:PHN".to_string(),
            };
            content = format!(
                "code='{}', conf={}, score={}, tags={}",
                code,
                confidence,
                score,
                registry_tags.join("|")
            );
            orphan_status = marker_orphan.clone().unwrap_or_default();
            visual = if visual_gap {
                render_point_bar(section_start, section_end, entry.offset, status)
            } else {
                String::new()
            };
            style = Some(match status {
                MarkerStatus::Accepted => |s: &str| s.green().to_string(),
                MarkerStatus::Rejected => |s: &str| s.yellow().to_string(),
                MarkerStatus::Phantom => |s: &str| s.red().to_string(),
            });
        }
        TimelineEvent::ItemStart {
            code,
            is_residue,
            range_start,
            range_end,
            registry_tags,
            stat_tags,
            stat_validation_ok,
        } => {
            type_str = if *is_residue {
                "RESIDUE:START".to_string()
            } else {
                "ITEM:START".to_string()
            };
            content = format!(
                "code='{}', range={}-{}, tags={}, stats_ok={}",
                code,
                range_start,
                range_end,
                registry_tags.join("|"),
                stat_validation_ok
            );
            if !stat_tags.is_empty() {
                content.push_str(&format!(", stat_tags={}", stat_tags.join("|")));
            }
            visual = if visual_gap {
                render_span_bar(section_start, section_end, *range_start, *range_end, *is_residue)
            } else {
                String::new()
            };
            style = Some(if *is_residue {
                |s: &str| s.yellow().to_string()
            } else {
                |s: &str| s.cyan().to_string()
            });
        }
        TimelineEvent::ItemEnd {
            code,
            is_residue,
            range_start,
            range_end,
            registry_tags,
            stat_tags,
            stat_validation_ok,
        } => {
            type_str = if *is_residue {
                "RESIDUE:END".to_string()
            } else {
                "ITEM:END".to_string()
            };
            content = format!(
                "code='{}', range={}-{}, tags={}, stats_ok={}",
                code,
                range_start,
                range_end,
                registry_tags.join("|"),
                stat_validation_ok
            );
            if !stat_tags.is_empty() {
                content.push_str(&format!(", stat_tags={}", stat_tags.join("|")));
            }
            visual = if visual_gap {
                render_span_bar(section_start, section_end, *range_start, *range_end, *is_residue)
            } else {
                String::new()
            };
            style = Some(if *is_residue {
                |s: &str| s.yellow().to_string()
            } else {
                |s: &str| s.blue().to_string()
            });
        }
    }

    (type_str, content, orphan_status, visual, style)
}

fn render_gap_bar(
    entry: &TimelineEntry,
    section_start: u64,
    section_end: u64,
) -> (String, String, Option<fn(&str) -> String>) {
    match &entry.event {
        TimelineEvent::ScannerMarker { code, status, .. } => {
            let label = format!("marker {}", code.trim());
            let bar = render_point_bar(section_start, section_end, entry.offset, status);
            let style = Some(match status {
                MarkerStatus::Accepted => |s: &str| s.green().to_string(),
                MarkerStatus::Rejected => |s: &str| s.yellow().to_string(),
                MarkerStatus::Phantom => |s: &str| s.red().to_string(),
            });
            (label, bar, style)
        }
        TimelineEvent::ItemStart {
            code,
            is_residue,
            range_start,
            range_end,
            ..
        } => {
            let label = format!("item {}", code.trim());
            let bar = render_span_bar(section_start, section_end, *range_start, *range_end, *is_residue);
            let style = Some(if *is_residue {
                |s: &str| s.yellow().to_string()
            } else {
                |s: &str| s.cyan().to_string()
            });
            (label, bar, style)
        }
        TimelineEvent::ItemEnd {
            code,
            is_residue,
            range_start,
            range_end,
            ..
        } => {
            let label = format!("item-end {}", code.trim());
            let bar = render_span_bar(section_start, section_end, *range_start, *range_end, *is_residue);
            let style = Some(if *is_residue {
                |s: &str| s.yellow().to_string()
            } else {
                |s: &str| s.blue().to_string()
            });
            (label, bar, style)
        }
    }
}

fn render_point_bar(section_start: u64, section_end: u64, point: u64, status: &MarkerStatus) -> String {
    let mut bar = build_grid_bar(section_start, section_end);
    let pos = relative_position(section_start, section_end, point);
    if !bar.is_empty() {
        replace_at(&mut bar, pos, "^");
    }

    match status {
        MarkerStatus::Accepted => bar.green().to_string(),
        MarkerStatus::Rejected => bar.yellow().to_string(),
        MarkerStatus::Phantom => bar.red().to_string(),
    }
}

fn render_span_bar(section_start: u64, section_end: u64, start: u64, end: u64, residue: bool) -> String {
    let mut bar = build_grid_bar(section_start, section_end);
    let start_pos = relative_position(section_start, section_end, start);
    let end_pos = relative_position(section_start, section_end, end.saturating_sub(1).max(start));

    if !bar.is_empty() {
        fill_range(&mut bar, start_pos, end_pos);
        replace_at(&mut bar, start_pos, "[");
        replace_at(&mut bar, end_pos, "]");
    }

    if residue {
        bar.yellow().to_string()
    } else {
        bar.cyan().to_string()
    }
}

fn build_grid_bar(section_start: u64, section_end: u64) -> String {
    let mut bar = vec!['.'; VISUAL_WIDTH];
    for tick in (0..VISUAL_WIDTH).step_by(8) {
        bar[tick] = '|';
    }
    bar.into_iter().collect()
}

fn relative_position(section_start: u64, section_end: u64, offset: u64) -> usize {
    let span = section_end.saturating_sub(section_start).max(1);
    let rel = offset.saturating_sub(section_start);
    let pos = rel.saturating_mul(VISUAL_WIDTH as u64 - 1) / span;
    pos.min((VISUAL_WIDTH - 1) as u64) as usize
}

fn replace_at(bar: &mut String, idx: usize, token: &str) {
    if idx >= bar.len() {
        return;
    }
    let mut chars: Vec<char> = bar.chars().collect();
    let token_char = token.chars().next().unwrap_or(' ');
    chars[idx] = token_char;
    *bar = chars.into_iter().collect();
}

fn fill_range(bar: &mut String, start: usize, end: usize) {
    let mut chars: Vec<char> = bar.chars().collect();
    let s = start.min(chars.len().saturating_sub(1));
    let e = end.min(chars.len().saturating_sub(1));
    for idx in s..=e {
        if chars[idx] == '.' {
            chars[idx] = '=';
        }
    }
    *bar = chars.into_iter().collect();
}

fn section_label(index: usize) -> &'static str {
    match index {
        0 => "Player Items",
        1 => "Corpse Items",
        2 => "Mercenary Items",
        3 => "Iron Golem",
        _ => "Unknown Section",
    }
}
