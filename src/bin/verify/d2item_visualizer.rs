use anyhow::{Context, Result};
use colored::Colorize;
use d2r_core::domain::item::scanner::{ItemMarker, MarkerStatus, scan_item_markers};
use d2r_core::item::{BitSegment, HuffmanTree, Item};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;

const DEFAULT_ITEM_LIMIT: usize = 12;
const DEFAULT_DETAIL_ITEM_LIMIT: usize = 4;
const DEFAULT_SEGMENT_LIMIT: usize = 32;
const DEFAULT_MARKER_SAMPLE_LIMIT: usize = 6;

#[derive(Debug, Clone)]
enum SectionFocus {
    Summary,
    Items,
    Segments,
    All,
    Filter(String),
}

use d2r_core::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};

#[derive(Debug, Clone, Serialize)]
struct VisualizerPayload {
    alpha_mode: bool,
    section: String,
    offset: Option<u64>,
    range: Option<u64>,
    selection: SelectionReport,
    summary: SummaryReport,
    scanner_marker_sample: Vec<ItemMarker>,
    sections: Vec<ItemReport>,
}

#[derive(Debug, Clone, Serialize)]
struct SelectionReport {
    focus: String,
    selected_items: usize,
    detailed: bool,
    full_dump: bool,
    selected_item_index: Option<usize>,
    nearest_item_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct SummaryReport {
    section_start_bit: u64,
    section_end_bit: u64,
    jm_markers: usize,
    scanner_markers: usize,
    accepted_markers: usize,
    rejected_markers: usize,
    phantom_markers: usize,
    parsed_items: usize,
    visible_items: usize,
    segment_total: usize,
    opaque_items: usize,
    semi_opaque_items: usize,
    residue_items: usize,
    parse_errors: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ItemReport {
    index: usize,
    code: String,
    start_bit: u64,
    end_bit: u64,
    bit_len: u64,
    segment_count: usize,
    marker_count: usize,
    is_residue: bool,
    is_opaque: bool,
    is_semi_opaque: bool,
    semantic_at_offset: Option<String>,
    segments: Vec<BitSegment>,
    markers: Vec<ItemMarker>,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let mut parser = ArgParser::new("d2item_visualizer").description(
        "Failure-first read-only item segment visualizer with JSON and semantic navigation.",
    );

    parser.add_spec(
        ArgSpec::positional("input", "Path to the save file (.d2s) to inspect").optional(),
    );
    parser.add_spec(
        ArgSpec::option(
            "file",
            None,
            Some("file"),
            "Path to the save file (.d2s) to inspect",
        )
        .optional(),
    );
    parser.add_spec(
        ArgSpec::option(
            "section",
            Some('S'),
            Some("section"),
            "Semantic focus (items, segments, summary, all, or an item-code filter)",
        )
        .with_default("items"),
    );
    parser.add_spec(
        ArgSpec::option(
            "offset",
            Some('O'),
            Some("offset"),
            "Absolute bit offset to focus on",
        )
        .optional(),
    );
    parser.add_spec(
        ArgSpec::option(
            "range",
            Some('R'),
            Some("range"),
            "Bit range to inspect from offset",
        )
        .optional(),
    );
    parser.add_spec(ArgSpec::flag(
        "verbose",
        Some('v'),
        Some("verbose"),
        "Show detailed segment trees for the selected items",
    ));
    parser.add_spec(ArgSpec::flag(
        "full-dump",
        None,
        Some("full-dump"),
        "Show every selected segment and marker without truncation",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(ArgError::Help(help)) => {
            println!("{help}");
            return Ok(0);
        }
        Err(ArgError::Error(err)) => {
            eprintln!("error: {err}\n\n{}", parser.usage());
            return Ok(1);
        }
    };

    let file = parsed
        .get("file")
        .or_else(|| parsed.get("input"))
        .cloned()
        .context("missing file argument")?;
    let section_raw = parsed
        .get("section")
        .cloned()
        .unwrap_or_else(|| "items".to_string());
    let section_focus = parse_section_focus(&section_raw);
    let offset = parse_u64(parsed.get("offset"));
    let range = parse_u64(parsed.get("range"));
    let verbose = parsed.is_set("verbose");
    let full_dump = parsed.is_set("full-dump");
    let json_mode = parsed.is_json();

    unsafe {
        env::set_var("D2R_ITEM_TRACE", "1");
    }

    let bytes = match fs::read(&file) {
        Ok(bytes) => bytes,
        Err(err) => {
            let metadata =
                ReportMetadata::new("d2item_visualizer", &file, env!("CARGO_PKG_VERSION"));
            let payload = VisualizerPayload {
                alpha_mode: false,
                section: section_raw,
                offset,
                range,
                selection: SelectionReport {
                    focus: focus_label(&section_focus),
                    selected_items: 0,
                    detailed: false,
                    full_dump: false,
                    selected_item_index: None,
                    nearest_item_index: None,
                },
                summary: SummaryReport {
                    section_start_bit: 0,
                    section_end_bit: 0,
                    jm_markers: 0,
                    scanner_markers: 0,
                    accepted_markers: 0,
                    rejected_markers: 0,
                    phantom_markers: 0,
                    parsed_items: 0,
                    visible_items: 0,
                    segment_total: 0,
                    opaque_items: 0,
                    semi_opaque_items: 0,
                    residue_items: 0,
                    parse_errors: 1,
                },
                scanner_marker_sample: Vec::new(),
                sections: Vec::new(),
            };
            let report = Report::new(metadata, ReportStatus::Fail)
                .with_results(payload)
                .with_issues(vec![ReportIssue {
                    kind: "io".to_string(),
                    message: format!("Failed to read file: {err}"),
                    bit_offset: None,
                }])
                .with_hints(vec![
                    "Check the file path or point the visualizer at a .d2s fixture.".to_string(),
                ]);

            emit_report(&report, json_mode)?;
            return Ok(1);
        }
    };

    let huffman = HuffmanTree::new();
    let alpha_mode = is_alpha_v105(&bytes);
    let jm_markers = find_jm_markers(&bytes);
    let section_slice = resolve_item_section(&bytes, &jm_markers);
    let scanner_markers = scan_item_markers(
        section_slice.bytes,
        &huffman,
        alpha_mode,
        section_slice.start_bit,
        section_slice.expected_count,
        false,
    );

    let (items, parse_error) = match Item::read_player_items(&bytes, &huffman, alpha_mode) {
        Ok(items) => (items, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };

    let selection = select_items(
        &items,
        &section_focus,
        offset,
        range,
        section_slice.start_bit,
        section_slice.end_bit,
    );

    let detailed = full_dump
        || verbose
        || matches!(section_focus, SectionFocus::Segments | SectionFocus::All)
        || offset.is_some()
        || range.is_some();

    let item_reports = build_item_reports(
        &items,
        &scanner_markers,
        &selection.selected_indices,
        offset,
        range,
        detailed,
        full_dump,
    );
    let scanner_marker_sample = scanner_markers
        .iter()
        .take(DEFAULT_MARKER_SAMPLE_LIMIT)
        .cloned()
        .collect::<Vec<_>>();

    let summary = build_summary(
        section_slice.start_bit,
        section_slice.end_bit,
        &items,
        &selection.selected_indices,
        &scanner_markers,
        jm_markers.len(),
        parse_error.is_some(),
    );

    let parse_error_present = parse_error.is_some();
    let summary_has_failure = report_has_failures(&section_focus, offset, range, &summary);

    let mut issues = selection.issues;
    if let Some(err) = parse_error.as_ref() {
        issues.push(format!("Item parser failed: {err}"));
    }
    if item_reports.is_empty() {
        if summary.parsed_items == 0 {
            issues.push("No parsed items recovered; showing scanner markers only.".to_string());
        } else if !matches!(section_focus, SectionFocus::Summary) {
            issues.push("No items matched the current selection.".to_string());
        }
    }

    let hint = if !issues.is_empty() {
        Some(build_hint(
            &section_focus,
            offset,
            range,
            item_reports.is_empty(),
        ))
    } else {
        None
    };

    let metadata = ReportMetadata::new("d2item_visualizer", &file, env!("CARGO_PKG_VERSION"));
    let status = if !parse_error_present && !summary_has_failure {
        ReportStatus::Ok
    } else {
        ReportStatus::Fail
    };

    let payload = VisualizerPayload {
        alpha_mode,
        section: section_raw,
        offset,
        range,
        selection: SelectionReport {
            focus: focus_label(&section_focus),
            selected_items: item_reports.len(),
            detailed,
            full_dump,
            selected_item_index: selection.primary_index,
            nearest_item_index: selection.nearest_index,
        },
        summary,
        scanner_marker_sample,
        sections: item_reports,
    };

    let mut report = Report::new(metadata, status)
        .with_results(payload)
        .with_issues(
            issues
                .into_iter()
                .map(|msg| ReportIssue {
                    kind: "general".to_string(),
                    message: msg,
                    bit_offset: None,
                })
                .collect(),
        );

    if let Some(h) = hint {
        report = report.with_hints(vec![h]);
    }

    emit_report(&report, json_mode)?;
    Ok(if report.status == ReportStatus::Ok {
        0
    } else {
        1
    })
}

fn emit_report(report: &Report<VisualizerPayload>, json_mode: bool) -> Result<()> {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    print_human_report(report);
    Ok(())
}

fn print_human_report(report: &Report<VisualizerPayload>) {
    let payload = report.scan_results.as_ref().unwrap();
    println!(
        "{}",
        format!(
            "d2item_visualizer | file={} | section={} | alpha={} | status={:?}",
            report.metadata.file, payload.section, payload.alpha_mode, report.status
        )
        .cyan()
        .bold()
    );

    println!(
        "Summary: items={} visible={} segments={} markers={} (accepted={} rejected={} phantom={})",
        payload.summary.parsed_items,
        payload.summary.visible_items,
        payload.summary.segment_total,
        payload.summary.scanner_markers,
        payload.summary.accepted_markers,
        payload.summary.rejected_markers,
        payload.summary.phantom_markers
    );

    if let Some(offset) = payload.offset {
        match payload.range {
            Some(range) => println!("Focus window: offset={} range={}", offset, range),
            None => println!("Focus offset: {}", offset),
        }
    }

    if !report.issues.is_empty() {
        println!("{}", "Issues:".yellow().bold());
        for issue in &report.issues {
            println!("  - {}", issue.message.red());
        }
    }

    if payload.sections.is_empty() && !payload.scanner_marker_sample.is_empty() {
        println!("{}", "Scanner markers:".cyan().bold());
        for marker in &payload.scanner_marker_sample {
            println!(
                "  {:>8} {:<9} score={:<4} conf={:<4} code={}",
                marker.offset,
                marker_status_label(marker.status).yellow(),
                marker.score,
                marker.confidence,
                marker.code.trim()
            );
        }
        if payload.summary.scanner_markers > payload.scanner_marker_sample.len() {
            println!(
                "{}",
                format!(
                    "  ... {} more scanner marker(s) hidden; use --full-dump to expand.",
                    payload.summary.scanner_markers - payload.scanner_marker_sample.len()
                )
                .yellow()
            );
        }
    }

    if matches!(payload.selection.focus.as_str(), "summary") {
        return;
    }

    let item_limit = if payload.selection.full_dump {
        usize::MAX
    } else if payload.selection.detailed {
        DEFAULT_DETAIL_ITEM_LIMIT
    } else {
        DEFAULT_ITEM_LIMIT
    };

    let mut printed = 0usize;
    for item in &payload.sections {
        if printed >= item_limit {
            let omitted = payload.sections.len().saturating_sub(printed);
            if omitted > 0 {
                println!(
                    "{}",
                    format!(
                        "... {} more item(s) hidden; use --full-dump to expand.",
                        omitted
                    )
                    .yellow()
                );
            }
            break;
        }

        let status = if item.is_semi_opaque {
            "semi-opaque".yellow()
        } else if item.is_opaque || item.is_residue {
            "opaque".magenta()
        } else {
            "ok".green()
        };

        println!(
            "  #{:<3} {:<8} bits {:>8}..{:>8} len={:>5} segs={:>3} markers={:>3} {}",
            item.index,
            item.code.green().bold(),
            item.start_bit,
            item.end_bit,
            item.bit_len,
            item.segment_count,
            item.marker_count,
            status
        );

        if payload.selection.detailed {
            if let Some(semantic) = &item.semantic_at_offset {
                println!("      semantic@offset: {}", semantic.blue());
            }

            print_segment_tree(
                item,
                payload.offset,
                payload.range,
                payload.selection.detailed,
                payload.selection.focus.as_str(),
                printed == 0,
            );

            if !item.markers.is_empty() {
                println!("      markers:");
                for marker in item.markers.iter().take(DEFAULT_MARKER_SAMPLE_LIMIT) {
                    println!(
                        "        {:>8} {:<9} score={:<4} conf={:<4} code={}",
                        marker.offset,
                        marker_status_label(marker.status).yellow(),
                        marker.score,
                        marker.confidence,
                        marker.code.trim()
                    );
                }
                if item.markers.len() > DEFAULT_MARKER_SAMPLE_LIMIT {
                    println!(
                        "        {}",
                        format!(
                            "... {} more marker(s) hidden; use --full-dump to expand.",
                            item.markers.len() - DEFAULT_MARKER_SAMPLE_LIMIT
                        )
                        .yellow()
                    );
                }
            }
        }

        printed += 1;
    }
}

fn print_segment_tree(
    item: &ItemReport,
    offset: Option<u64>,
    range: Option<u64>,
    detailed: bool,
    focus: &str,
    primary_item: bool,
) {
    let mut printed = 0usize;
    let segment_limit = if detailed {
        usize::MAX
    } else {
        DEFAULT_SEGMENT_LIMIT
    };
    let window = selection_window(offset, range);

    if primary_item {
        println!(
            "      segment tree: focus={} window={}",
            focus,
            window
                .map(|(start, end)| format!("{}..{}", start, end))
                .unwrap_or_else(|| "none".to_string())
        );
    }

    for seg in &item.segments {
        if !segment_matches(seg, window) {
            continue;
        }

        let indent = "  ".repeat(seg.depth.saturating_add(1));
        let len = seg.end.saturating_sub(seg.start);
        let label = if let Some(off) = offset {
            if off >= seg.start && off < seg.end {
                seg.label.clone().cyan().bold().to_string()
            } else {
                seg.label.clone()
            }
        } else {
            seg.label.clone()
        };

        println!(
            "{}[{:>8}..{:>8}] len={:>5} {}",
            indent, seg.start, seg.end, len, label
        );
        printed += 1;

        if printed >= segment_limit {
            let hidden = item.segments.len().saturating_sub(printed);
            if hidden > 0 {
                println!(
                    "{}",
                    format!("{}... {} more segment(s) hidden.", indent, hidden).yellow()
                );
            }
            break;
        }
    }
}

fn build_item_reports(
    items: &[Item],
    markers: &[ItemMarker],
    selected_indices: &[usize],
    offset: Option<u64>,
    range: Option<u64>,
    detailed: bool,
    full_dump: bool,
) -> Vec<ItemReport> {
    let segment_limit = if detailed || full_dump {
        usize::MAX
    } else {
        DEFAULT_SEGMENT_LIMIT
    };

    selected_indices
        .iter()
        .filter_map(|&index| items.get(index).map(|item| (index, item)))
        .map(|(index, item)| {
            let item_markers = markers
                .iter()
                .filter(|marker| {
                    marker.offset >= item.range.start && marker.offset < item.range.end
                })
                .cloned()
                .take(DEFAULT_MARKER_SAMPLE_LIMIT)
                .collect::<Vec<_>>();

            let segments = item
                .segments
                .iter()
                .filter(|seg| {
                    if let Some((start, end)) = selection_window(offset, range) {
                        segment_matches(seg, Some((start, end)))
                    } else {
                        true
                    }
                })
                .take(segment_limit)
                .cloned()
                .collect::<Vec<_>>();

            ItemReport {
                index,
                code: item.code.trim().to_string(),
                start_bit: item.range.start,
                end_bit: item.range.end,
                bit_len: item.range.end.saturating_sub(item.range.start),
                segment_count: item.segments.len(),
                marker_count: markers
                    .iter()
                    .filter(|marker| {
                        marker.offset >= item.range.start && marker.offset < item.range.end
                    })
                    .count(),
                is_residue: item.is_residue(),
                is_opaque: item.is_opaque(),
                is_semi_opaque: item.is_semi_opaque(),
                semantic_at_offset: offset
                    .filter(|off| *off >= item.range.start && *off < item.range.end)
                    .and_then(|off| item.query_bit(off).map(|semantic| semantic.label)),
                segments,
                markers: item_markers,
            }
        })
        .collect()
}

fn build_summary(
    section_start_bit: u64,
    section_end_bit: u64,
    items: &[Item],
    selected_indices: &[usize],
    markers: &[ItemMarker],
    jm_markers: usize,
    parse_error: bool,
) -> SummaryReport {
    SummaryReport {
        section_start_bit,
        section_end_bit,
        jm_markers,
        scanner_markers: markers.len(),
        accepted_markers: markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Accepted)
            .count(),
        rejected_markers: markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Rejected)
            .count(),
        phantom_markers: markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Phantom)
            .count(),
        parsed_items: items.len(),
        visible_items: selected_indices.len(),
        segment_total: selected_indices
            .iter()
            .filter_map(|&index| items.get(index))
            .map(|item| item.segments.len())
            .sum(),
        opaque_items: items.iter().filter(|item| item.is_opaque()).count(),
        semi_opaque_items: items.iter().filter(|item| item.is_semi_opaque()).count(),
        residue_items: items.iter().filter(|item| item.is_residue()).count(),
        parse_errors: usize::from(parse_error),
    }
}

fn selection_window(offset: Option<u64>, range: Option<u64>) -> Option<(u64, u64)> {
    let start = offset.unwrap_or(0);
    range.map(|range| (start, start.saturating_add(range)))
}

fn segment_matches(segment: &BitSegment, window: Option<(u64, u64)>) -> bool {
    match window {
        None => true,
        Some((start, end)) => segment.end > start && segment.start < end,
    }
}

fn select_items(
    items: &[Item],
    focus: &SectionFocus,
    offset: Option<u64>,
    range: Option<u64>,
    section_start_bit: u64,
    section_end_bit: u64,
) -> SelectionState {
    let mut issues = Vec::new();
    let mut selected_indices = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let matches_focus = match focus {
            SectionFocus::Summary => false,
            SectionFocus::Items | SectionFocus::Segments | SectionFocus::All => true,
            SectionFocus::Filter(filter) => item
                .code
                .trim()
                .to_ascii_lowercase()
                .contains(&filter.to_ascii_lowercase()),
        };

        let matches_window = match (offset, range) {
            (Some(off), Some(rng)) => {
                let start = off;
                let end = off.saturating_add(rng);
                item.range.end > start && item.range.start < end
            }
            (Some(off), None) => off >= item.range.start && off < item.range.end,
            (None, Some(rng)) => {
                let end = section_start_bit.saturating_add(rng);
                item.range.end > section_start_bit && item.range.start < end
            }
            (None, None) => true,
        };

        if matches_focus && matches_window {
            selected_indices.push(index);
        }
    }

    let primary_index = if let Some(off) = offset {
        selected_indices
            .iter()
            .copied()
            .find(|&index| {
                let item = &items[index];
                off >= item.range.start && off < item.range.end
            })
            .or_else(|| nearest_item_index(items, off))
    } else {
        selected_indices.first().copied()
    };

    let nearest_index = offset.and_then(|off| nearest_item_index(items, off));

    if selected_indices.is_empty()
        && !matches!(focus, SectionFocus::Summary)
        && !items.is_empty()
        && offset.is_none()
    {
        issues.push("No items matched the current section filter.".to_string());
    }

    if selected_indices.is_empty() {
        if let Some(index) = primary_index {
            selected_indices.push(index);
        } else if let Some(off) = offset {
            issues.push(format!(
                "No item contains offset {}; no fallback item was available.",
                off
            ));
        }
    }

    if let (Some(off), Some(range)) = (offset, range) {
        let end = off.saturating_add(range);
        if end < off || end < section_start_bit || off > section_end_bit {
            issues.push("Requested bit window is outside the item section.".to_string());
        }
    }

    SelectionState {
        selected_indices,
        primary_index,
        nearest_index,
        issues,
    }
}

fn nearest_item_index(items: &[Item], offset: u64) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .min_by_key(|(_, item)| distance_to_range(offset, item.range.start, item.range.end))
        .map(|(index, _)| index)
}

fn distance_to_range(offset: u64, start: u64, end: u64) -> u64 {
    if offset < start {
        start - offset
    } else if offset >= end {
        offset - end
    } else {
        0
    }
}

fn report_has_failures(
    focus: &SectionFocus,
    offset: Option<u64>,
    range: Option<u64>,
    summary: &SummaryReport,
) -> bool {
    summary.parse_errors > 0
        || (summary.parsed_items == 0 && summary.scanner_markers == 0)
        || (summary.parsed_items > 0
            && summary.visible_items == 0
            && !matches!(focus, SectionFocus::Summary))
        || offset.is_some() && range.is_some() && summary.visible_items == 0
}

fn build_hint(
    focus: &SectionFocus,
    offset: Option<u64>,
    range: Option<u64>,
    empty_selection: bool,
) -> String {
    if empty_selection {
        return "Try --section items --verbose or narrow with --offset <bit> --range <bits>."
            .to_string();
    }

    if matches!(focus, SectionFocus::Summary) {
        return "Use --section items to inspect item ranges or --section segments for the segment tree.".to_string();
    }

    if offset.is_some() && range.is_none() {
        return "Add --range <bits> to narrow the window around the selected offset.".to_string();
    }

    if range.is_some() {
        return "If the window is too wide, reduce --range or use --full-dump for an explicit expansion.".to_string();
    }

    "Use --json for machine-readable inspection or --full-dump for exhaustive segment trees."
        .to_string()
}

fn parse_section_focus(raw: &str) -> SectionFocus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "summary" => SectionFocus::Summary,
        "items" => SectionFocus::Items,
        "segments" => SectionFocus::Segments,
        "all" => SectionFocus::All,
        other => SectionFocus::Filter(other.to_string()),
    }
}

fn focus_label(focus: &SectionFocus) -> String {
    match focus {
        SectionFocus::Summary => "summary".to_string(),
        SectionFocus::Items => "items".to_string(),
        SectionFocus::Segments => "segments".to_string(),
        SectionFocus::All => "all".to_string(),
        SectionFocus::Filter(filter) => format!("filter:{filter}"),
    }
}

fn parse_u64(raw: Option<&String>) -> Option<u64> {
    raw.and_then(|value| value.parse::<u64>().ok())
}

fn is_alpha_v105(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(&[0x69, 0, 0, 0])
}

struct SectionSlice<'a> {
    bytes: &'a [u8],
    start_bit: u64,
    end_bit: u64,
    expected_count: Option<u16>,
}

fn resolve_item_section<'a>(bytes: &'a [u8], jm_markers: &[usize]) -> SectionSlice<'a> {
    if let Some(&jm_offset) = jm_markers.first() {
        let start = jm_offset.saturating_add(4);
        let end = jm_markers.get(1).copied().unwrap_or(bytes.len()).max(start);
        let expected_count = bytes.get(jm_offset + 2..jm_offset + 4).and_then(|slice| {
            let bytes: [u8; 2] = slice.try_into().ok()?;
            Some(u16::from_le_bytes(bytes))
        });

        SectionSlice {
            bytes: &bytes[start.min(bytes.len())..end.min(bytes.len())],
            start_bit: (start.min(bytes.len()) as u64) * 8,
            end_bit: (end.min(bytes.len()) as u64) * 8,
            expected_count,
        }
    } else {
        SectionSlice {
            bytes,
            start_bit: 0,
            end_bit: (bytes.len() as u64) * 8,
            expected_count: None,
        }
    }
}

fn marker_status_label(status: MarkerStatus) -> &'static str {
    match status {
        MarkerStatus::Accepted => "accepted",
        MarkerStatus::Rejected => "rejected",
        MarkerStatus::Phantom => "phantom",
    }
}

struct SelectionState {
    selected_indices: Vec<usize>,
    primary_index: Option<usize>,
    nearest_index: Option<usize>,
    issues: Vec<String>,
}
