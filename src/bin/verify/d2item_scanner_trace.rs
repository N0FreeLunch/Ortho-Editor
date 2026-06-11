use anyhow::{Context, Result, bail};
use d2r_core::domain::item::scanner::{ItemMarker, MarkerStatus, scan_item_markers};
use d2r_core::item::HuffmanTree;
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;

const DEFAULT_CONTEXT_BITS: u64 = 64;
const DEFAULT_DISPLAY_LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayRadix {
    Bits,
    Hex,
}

impl DisplayRadix {
    fn from_flags(hex: bool, bit: bool) -> Result<Self> {
        match (hex, bit) {
            (true, true) => bail!("--hex and --bit cannot be used together"),
            (true, false) => Ok(Self::Hex),
            _ => Ok(Self::Bits),
        }
    }

    fn format_offset(self, bits: u64) -> String {
        match self {
            Self::Bits => format!("{}b", bits),
            Self::Hex => format!("0x{:X}", bits),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct TraceFocus {
    section: Option<usize>,
    offset_bits: Option<u64>,
    range_bits: Option<u64>,
    context_bits: u64,
    verbose: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScannerTraceReport {
    file: String,
    version: u32,
    alpha_mode: bool,
    display_radix: String,
    verdict: String,
    focus: TraceFocus,
    total_jm_sections: usize,
    sections: Vec<SectionTrace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct SectionTrace {
    section_index: usize,
    jm_offset: u64,
    next_jm_offset: u64,
    section_bit_offset: u64,
    section_end_bit: u64,
    expected_count: u16,
    total_candidate_count: usize,
    accepted_count: usize,
    rejected_count: usize,
    phantom_count: usize,
    emitted_candidate_count: usize,
    truncated: bool,
    winner: Option<CandidateTrace>,
    runner_up: Option<CandidateTrace>,
    decision_path: Vec<String>,
    candidates: Vec<CandidateTrace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct CandidateTrace {
    rank: usize,
    offset_bits: u64,
    confidence: u32,
    score: i32,
    status: String,
    code: String,
    delta_to_winner: i32,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_scanner_trace")
        .description("Read-only trace of scanner candidate ranking and decision paths.");

    parser.add_spec(
        ArgSpec::option("file", Some('f'), Some("file"), "Path to D2S save file").required(),
    );
    parser.add_spec(ArgSpec::flag(
        "alpha",
        Some('a'),
        Some("alpha"),
        "Force Alpha v105 mode",
    ));
    parser.add_spec(ArgSpec::option(
        "section",
        Some('S'),
        Some("section"),
        "1-based JM section index to trace",
    ));
    parser.add_spec(ArgSpec::option(
        "offset",
        Some('o'),
        Some("offset"),
        "Focus window anchor bit offset",
    ));
    parser.add_spec(ArgSpec::option(
        "range",
        Some('R'),
        Some("range"),
        "Focus window length in bits",
    ));
    parser.add_spec(ArgSpec::option(
        "context",
        Some('C'),
        Some("context"),
        "Extra look-around radius in bits",
    ));
    parser.add_spec(ArgSpec::flag(
        "hex",
        None,
        Some("hex"),
        "Display offsets in hex",
    ));
    parser.add_spec(ArgSpec::flag(
        "bit",
        None,
        Some("bit"),
        "Display offsets in bits",
    ));
    parser.add_spec(ArgSpec::flag(
        "verbose",
        Some('v'),
        Some("verbose"),
        "Emit the full ranked candidate list",
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
    let section_filter = parsed
        .get("section")
        .map(|value| parse_usize(value, "section"))
        .transpose()?;
    let offset = parsed
        .get("offset")
        .map(|value| parse_u64(value, "offset"))
        .transpose()?;
    let range = parsed
        .get("range")
        .map(|value| parse_u64(value, "range"))
        .transpose()?;
    let context = parsed
        .get("context")
        .map(|value| parse_u64(value, "context"))
        .transpose()?
        .unwrap_or(DEFAULT_CONTEXT_BITS);
    let verbose = parsed.is_set("verbose");
    let display_radix = DisplayRadix::from_flags(parsed.is_set("hex"), parsed.is_set("bit"))?;
    let use_json = parsed.is_json();

    let bytes =
        fs::read(file_path).with_context(|| format!("Failed to read file: {}", file_path))?;
    if bytes.len() < 8 {
        bail!("File too small to read version header: {}", file_path);
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let alpha_mode = parsed.is_set("alpha") || version == 105;
    let huffman = HuffmanTree::new();
    let jm_positions = find_jm_markers(&bytes);
    let sections = build_sections(
        &bytes,
        &huffman,
        alpha_mode,
        &jm_positions,
        section_filter,
        offset,
        range,
        context,
        verbose,
        display_radix,
    )?;

    let report = ScannerTraceReport {
        file: file_path.to_string(),
        version,
        alpha_mode,
        display_radix: match display_radix {
            DisplayRadix::Bits => "bits".to_string(),
            DisplayRadix::Hex => "hex".to_string(),
        },
        verdict: build_verdict(&sections, &jm_positions),
        focus: TraceFocus {
            section: section_filter,
            offset_bits: offset,
            range_bits: range,
            context_bits: context,
            verbose,
        },
        total_jm_sections: jm_positions.len(),
        sections,
    };

    if use_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text_report(&report, display_radix);
    }

    Ok(())
}

fn build_sections(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha_mode: bool,
    jm_positions: &[usize],
    section_filter: Option<usize>,
    offset: Option<u64>,
    range: Option<u64>,
    context: u64,
    verbose: bool,
    display_radix: DisplayRadix,
) -> Result<Vec<SectionTrace>> {
    let mut reports = Vec::new();
    let focus_window = offset.map(|anchor| {
        let start = anchor.saturating_sub(context);
        let end = anchor.saturating_add(range.unwrap_or(context).max(1));
        (start, end)
    });

    for (index, &jm_offset) in jm_positions.iter().enumerate() {
        let display_index = index + 1;
        if let Some(filter) = section_filter {
            if filter != display_index {
                continue;
            }
        }

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

        let accepted_count = markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Accepted)
            .count();
        let rejected_count = markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Rejected)
            .count();
        let phantom_count = markers
            .iter()
            .filter(|marker| marker.status == MarkerStatus::Phantom)
            .count();

        let mut ranked = rank_markers(&markers);
        if let Some((start, end)) = focus_window {
            ranked.retain(|marker| {
                let absolute_offset = section_bit_offset + marker.offset;
                absolute_offset >= start && absolute_offset < end
            });
        }

        let total_candidate_count = markers.len();
        let emit_limit = if verbose {
            usize::MAX
        } else {
            DEFAULT_DISPLAY_LIMIT
        };
        let emitted_markers = ranked.into_iter().take(emit_limit).collect::<Vec<_>>();
        let emitted_candidate_count = emitted_markers.len();
        let truncated = emitted_candidate_count < total_candidate_count
            || (focus_window.is_some() && emitted_candidate_count < markers.len());

        let winner = emitted_markers
            .iter()
            .find(|marker| marker.status == MarkerStatus::Accepted)
            .cloned()
            .or_else(|| emitted_markers.first().cloned());
        let winner_score = winner.as_ref().map(|marker| marker.score).unwrap_or(0);
        let runner_up = emitted_markers
            .iter()
            .find(|marker| !same_marker(marker, winner.as_ref()))
            .cloned();

        let candidates = emitted_markers
            .iter()
            .enumerate()
            .map(|(idx, marker)| {
                build_candidate_trace(idx + 1, marker, winner_score, section_bit_offset)
            })
            .collect::<Vec<_>>();

        let decision_path = build_decision_path(
            &markers,
            winner.as_ref(),
            runner_up.as_ref(),
            emitted_candidate_count,
            total_candidate_count,
            focus_window,
            display_radix,
        );

        reports.push(SectionTrace {
            section_index: display_index,
            jm_offset: jm_offset as u64,
            next_jm_offset: next_jm_offset as u64,
            section_bit_offset,
            section_end_bit,
            expected_count,
            total_candidate_count,
            accepted_count,
            rejected_count,
            phantom_count,
            emitted_candidate_count,
            truncated,
            winner: winner
                .as_ref()
                .map(|marker| build_candidate_trace(1, marker, winner_score, section_bit_offset)),
            runner_up: runner_up
                .as_ref()
                .map(|marker| build_candidate_trace(2, marker, winner_score, section_bit_offset)),
            decision_path,
            candidates,
        });
    }

    Ok(reports)
}

fn rank_markers(markers: &[ItemMarker]) -> Vec<ItemMarker> {
    let mut ranked = markers.to_vec();
    ranked.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.confidence.cmp(&a.confidence))
            .then_with(|| status_rank(a.status).cmp(&status_rank(b.status)))
            .then_with(|| a.offset.cmp(&b.offset))
            .then_with(|| a.code.cmp(&b.code))
    });
    ranked
}

fn status_rank(status: MarkerStatus) -> u8 {
    match status {
        MarkerStatus::Accepted => 0,
        MarkerStatus::Phantom => 1,
        MarkerStatus::Rejected => 2,
    }
}

fn build_candidate_trace(
    rank: usize,
    marker: &ItemMarker,
    winner_score: i32,
    section_bit_offset: u64,
) -> CandidateTrace {
    CandidateTrace {
        rank,
        offset_bits: section_bit_offset + marker.offset,
        confidence: marker.confidence,
        score: marker.score,
        status: status_name(marker.status).to_string(),
        code: marker.code.trim().to_string(),
        delta_to_winner: winner_score - marker.score,
    }
}

fn build_decision_path(
    markers: &[ItemMarker],
    winner: Option<&ItemMarker>,
    runner_up: Option<&ItemMarker>,
    emitted_candidate_count: usize,
    total_candidate_count: usize,
    focus_window: Option<(u64, u64)>,
    display_radix: DisplayRadix,
) -> Vec<String> {
    let mut path = Vec::new();
    if let Some((start, end)) = focus_window {
        path.push(format!(
            "focus window: {}..{}",
            display_radix.format_offset(start),
            display_radix.format_offset(end)
        ));
    } else {
        path.push("focus window: full section".to_string());
    }

    let accepted_count = markers
        .iter()
        .filter(|marker| marker.status == MarkerStatus::Accepted)
        .count();
    let rejected_count = markers
        .iter()
        .filter(|marker| marker.status == MarkerStatus::Rejected)
        .count();
    let phantom_count = markers
        .iter()
        .filter(|marker| marker.status == MarkerStatus::Phantom)
        .count();

    path.push(format!(
        "candidate breakdown: accepted={}, rejected={}, phantom={}",
        accepted_count, rejected_count, phantom_count
    ));

    if let Some(winner) = winner {
        let status = status_name(winner.status);
        path.push(format!(
            "winner: {} at {} with confidence {} and score {}",
            winner.code.trim(),
            display_radix.format_offset(winner.offset),
            winner.confidence,
            winner.score
        ));
        if let Some(runner_up) = runner_up {
            path.push(format!(
                "runner-up: {} at {} with score {} (delta {})",
                runner_up.code.trim(),
                display_radix.format_offset(runner_up.offset),
                runner_up.score,
                winner.score - runner_up.score
            ));
        }
        if status != "accepted" {
            path.push(format!(
                "best visible candidate was not accepted; status={}",
                status
            ));
        }
    } else {
        path.push("no visible candidate after focus filtering".to_string());
    }

    if emitted_candidate_count < total_candidate_count {
        path.push(format!(
            "display truncated: showing {} of {} candidates",
            emitted_candidate_count, total_candidate_count
        ));
    } else {
        path.push(format!(
            "display complete: {} candidates",
            total_candidate_count
        ));
    }

    path
}

fn build_verdict(sections: &[SectionTrace], jm_positions: &[usize]) -> String {
    if jm_positions.is_empty() {
        return "no_jm_sections_found".to_string();
    }
    if sections.is_empty() {
        return "no_sections_in_focus".to_string();
    }
    if sections.iter().any(|section| {
        section.winner.as_ref().map(|winner| winner.status.as_str()) == Some("accepted")
    }) {
        "accepted_winner_found".to_string()
    } else {
        "trace_available_but_no_accepted_winner_found".to_string()
    }
}

fn print_text_report(report: &ScannerTraceReport, display_radix: DisplayRadix) {
    println!("Scanner trace: {}", report.file);
    println!(
        "Version: {} | Alpha mode: {} | Sections: {}",
        report.version, report.alpha_mode, report.total_jm_sections
    );
    println!("Verdict: {}", report.verdict);
    println!(
        "Focus: section={:?}, offset={:?}, range={:?}, context={} bits, verbose={}",
        report.focus.section,
        report.focus.offset_bits,
        report.focus.range_bits,
        report.focus.context_bits,
        report.focus.verbose
    );
    println!("Display radix: {}", report.display_radix);
    println!("{:-<100}", "");

    for section in &report.sections {
        println!(
            "Section {} | JM byte {} | expected_count={} | candidates={} | accepted={} | rejected={} | phantom={}",
            section.section_index,
            section.jm_offset / 8,
            section.expected_count,
            section.total_candidate_count,
            section.accepted_count,
            section.rejected_count,
            section.phantom_count
        );
        println!(
            "  window: {}..{} bits",
            section_bit_fmt(display_radix, section.section_bit_offset),
            section_bit_fmt(display_radix, section.section_end_bit)
        );
        if let Some(winner) = &section.winner {
            println!(
                "  winner: {} @ {} | score={} | confidence={} | status={}",
                winner.code,
                section_bit_fmt(display_radix, winner.offset_bits),
                winner.score,
                winner.confidence,
                winner.status
            );
        } else {
            println!("  winner: none");
        }
        if let Some(runner_up) = &section.runner_up {
            println!(
                "  runner-up: {} @ {} | score={} | delta={}",
                runner_up.code,
                section_bit_fmt(display_radix, runner_up.offset_bits),
                runner_up.score,
                runner_up.delta_to_winner
            );
        }

        for line in &section.decision_path {
            println!("  - {}", line);
        }

        println!(
            "  {:<6} | {:<16} | {:<10} | {:<10} | {:<10} | {:<12}",
            "Rank", "Offset", "Score", "Conf", "Delta", "Status/Code"
        );
        println!("  {:-<80}", "");
        for candidate in &section.candidates {
            println!(
                "  {:<6} | {:<16} | {:<10} | {:<10} | {:<10} | {} / {}",
                candidate.rank,
                section_bit_fmt(display_radix, candidate.offset_bits),
                candidate.score,
                candidate.confidence,
                candidate.delta_to_winner,
                candidate.status,
                candidate.code
            );
        }
        if section.truncated {
            println!(
                "  [TRUNCATED] showing {} of {} candidates",
                section.emitted_candidate_count, section.total_candidate_count
            );
        }
        println!("{:-<100}", "");
    }

    if report.sections.is_empty() {
        println!("No sections matched the current focus.");
    }
}

fn section_bit_fmt(display_radix: DisplayRadix, bits: u64) -> String {
    display_radix.format_offset(bits)
}

fn status_name(status: MarkerStatus) -> &'static str {
    match status {
        MarkerStatus::Accepted => "accepted",
        MarkerStatus::Rejected => "rejected",
        MarkerStatus::Phantom => "phantom",
    }
}

fn same_marker(left: &ItemMarker, right: Option<&ItemMarker>) -> bool {
    if let Some(right) = right {
        left.offset == right.offset
            && left.confidence == right.confidence
            && left.score == right.score
            && left.status == right.status
            && left.code == right.code
    } else {
        false
    }
}

fn parse_u64(value: &str, name: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("Invalid {} value: {}", name, value))
}

fn parse_usize(value: &str, name: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .with_context(|| format!("Invalid {} value: {}", name, value))
}
