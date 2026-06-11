use d2r_core::domain::stats::parser::{
    clear_socket_recovery_trace_events, set_socket_recovery_trace_enabled,
    take_socket_recovery_trace_events,
};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum ProbeStatus {
    Ok,
    FallbackParent,
    NoParent,
    ParseError,
}

#[derive(Serialize, Debug)]
struct SeamDiscovery {
    bit_offset: u64,
    source: String,
    marker_bit: Option<u64>,
    terminator_bit: Option<u64>,
    fallback_bit: u64,
}

#[derive(Serialize, Debug)]
struct ShiftProbe {
    shift: u32,
    status: ProbeStatus,
    parent_code: Option<String>,
    mode: Option<String>,
    parsed_children: usize,
    expected_children: usize,
    child_codes: Vec<String>,
    pre_loop: Vec<ProbeTraceEvent>,
    per_child: Vec<ProbeTraceEvent>,
    fallback_entry: Vec<ProbeTraceEvent>,
    post_loop: Vec<ProbeTraceEvent>,
    expected_next_marker: Option<u64>,
    observed_next_marker: Option<u64>,
    marker_940_actionable: bool,
    error: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct ProbeTraceEvent {
    current_rel_pos: u64,
    next_marker: Option<u64>,
    observed_marker: Option<u64>,
    note: String,
}

#[derive(Serialize, Debug)]
struct SocketFuzzerPayload {
    fixture: PathBuf,
    anchor_bit: u64,
    jm_byte_pos: usize,
    parent_code: String,
    expected_children: usize,
    shift_start: u32,
    shift_end: u32,
    seam: SeamDiscovery,
    probes: Vec<ShiftProbe>,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_socket_fuzzer");
    parser.add_spec(ArgSpec::positional("fixture", "Path to the save file (.d2s)").optional());
    parser.add_spec(
        ArgSpec::option(
            "parent-code",
            None,
            Some("parent-code"),
            "Item code to track for socket-child recovery",
        )
        .with_default("xrs"),
    );
    parser.add_spec(
        ArgSpec::option(
            "expected-children",
            None,
            Some("expected-children"),
            "Expected socket child count for the tracked parent",
        )
        .with_default("3"),
    );
    parser.add_spec(
        ArgSpec::option(
            "shift-start",
            None,
            Some("shift-start"),
            "First shift to probe",
        )
        .with_default("0"),
    );
    parser.add_spec(
        ArgSpec::option(
            "shift-end",
            None,
            Some("shift-end"),
            "Last shift to probe (inclusive)",
        )
        .with_default("48"),
    );
    parser.add_spec(ArgSpec::option(
        "seam-bit",
        None,
        Some("seam-bit"),
        "Override the discovered seam bit offset",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixture_path = parsed.get("fixture").map(PathBuf::from).unwrap_or_else(|| {
        let mut p =
            PathBuf::from("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        if !p.exists() {
            p = PathBuf::from(
                "d2r-core/tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
            );
        }
        p
    });

    if !fixture_path.exists() {
        anyhow::bail!("fixture path not found: {}", fixture_path.display());
    }

    let parent_code = parsed
        .get("parent-code")
        .cloned()
        .unwrap_or_else(|| "xrs".to_string());
    let expected_children = parsed
        .get("expected-children")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(3);
    let shift_start = parsed
        .get("shift-start")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let shift_end = parsed
        .get("shift-end")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(48);
    let seam_override = parsed.get("seam-bit").and_then(|s| s.parse::<u64>().ok());
    let use_json = parsed.is_json();

    let bytes = fs::read(&fixture_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", fixture_path.display(), e))?;

    if bytes.len() < 2 {
        anyhow::bail!("fixture is too small to contain a JM anchor");
    }

    let jm_byte_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .ok_or_else(|| anyhow::anyhow!("No JM marker found"))?;
    let anchor = (jm_byte_pos as u64) * 8;

    let huffman = HuffmanTree::new();
    let is_alpha = true;

    let initial_items =
        Item::read_section_ext(&bytes[jm_byte_pos..], anchor, 15, &huffman, is_alpha, false)?;

    let parent = select_parent(&initial_items, &parent_code)
        .ok_or_else(|| anyhow::anyhow!("No items found to determine seam"))?;

    let section_bits = to_bits(&bytes[jm_byte_pos..]);
    let seam = if let Some(seam_bit) = seam_override {
        SeamDiscovery {
            bit_offset: seam_bit,
            source: "override".to_string(),
            marker_bit: None,
            terminator_bit: None,
            fallback_bit: parent.range.end.saturating_sub(anchor),
        }
    } else {
        discover_seam(&section_bits, parent, anchor)
    };

    let mut probes = Vec::new();
    set_socket_recovery_trace_enabled(true);
    for shift in shift_start..=shift_end {
        clear_socket_recovery_trace_events();
        let fuzzed_bytes = if shift == 0 {
            bytes.clone()
        } else {
            inject_bits(&bytes, seam.bit_offset, shift)
        };

        let section_bytes = &fuzzed_bytes[jm_byte_pos..];
        let res = Item::read_section_ext(section_bytes, anchor, 15, &huffman, is_alpha, false);

        match res {
            Ok(items) => {
                let traces = take_socket_recovery_trace_events();
                let pre_loop = filter_trace(&traces, "pre_loop");
                let per_child = filter_trace(&traces, "per_child");
                let fallback_entry = filter_trace(&traces, "fallback_entry");
                let post_loop = filter_trace(&traces, "post_loop");
                let expected_next_marker = pre_loop.first().and_then(|e| e.next_marker);
                let observed_next_marker = per_child
                    .first()
                    .and_then(|e| e.observed_marker)
                    .or_else(|| post_loop.first().and_then(|e| e.observed_marker));
                let marker_940_actionable =
                    per_child.iter().any(|e| e.observed_marker == Some(940));
                let exact_parent = find_parent(&items, &parent_code);
                let fallback_parent = exact_parent.or_else(|| fallback_parent(&items));
                let selected = fallback_parent;

                let (status, parent_code_seen, mode, parsed_children, child_codes) =
                    if let Some(p) = selected {
                        let exact = exact_parent.is_some();
                        let status = if exact {
                            ProbeStatus::Ok
                        } else {
                            ProbeStatus::FallbackParent
                        };
                        (
                            status,
                            Some(p.code().trim().to_string()),
                            Some(p.header.mode.to_string()),
                            p.socketed_items.len(),
                            p.socketed_items
                                .iter()
                                .map(|child| child.code().trim().to_string())
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        (ProbeStatus::NoParent, None, None, 0, Vec::new())
                    };

                probes.push(ShiftProbe {
                    shift,
                    status,
                    parent_code: parent_code_seen,
                    mode,
                    parsed_children,
                    expected_children,
                    child_codes,
                    pre_loop,
                    per_child,
                    fallback_entry,
                    post_loop,
                    expected_next_marker,
                    observed_next_marker,
                    marker_940_actionable,
                    error: None,
                });
            }
            Err(e) => {
                let traces = take_socket_recovery_trace_events();
                let pre_loop = filter_trace(&traces, "pre_loop");
                let per_child = filter_trace(&traces, "per_child");
                let fallback_entry = filter_trace(&traces, "fallback_entry");
                let post_loop = filter_trace(&traces, "post_loop");
                let expected_next_marker = pre_loop.first().and_then(|evt| evt.next_marker);
                let observed_next_marker = per_child.first().and_then(|evt| evt.observed_marker);
                probes.push(ShiftProbe {
                    shift,
                    status: ProbeStatus::ParseError,
                    parent_code: None,
                    mode: None,
                    parsed_children: 0,
                    expected_children,
                    child_codes: Vec::new(),
                    pre_loop,
                    per_child,
                    fallback_entry,
                    post_loop,
                    expected_next_marker,
                    observed_next_marker,
                    marker_940_actionable: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    set_socket_recovery_trace_enabled(false);

    let payload = SocketFuzzerPayload {
        fixture: fixture_path,
        anchor_bit: anchor,
        jm_byte_pos,
        parent_code: parent_code.clone(),
        expected_children,
        shift_start,
        shift_end,
        seam,
        probes,
    };

    if use_json {
        let metadata = ReportMetadata::new("d2item_socket_fuzzer", &payload.fixture.to_string_lossy(), env!("CARGO_PKG_VERSION"));
        let report = Report::new(metadata, ReportStatus::Ok).with_results(payload);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("Fixture: {}", payload.fixture.display());
    println!(
        "Anchor (JM): {} (byte {}), Seam: {} (source: {})",
        payload.anchor_bit, payload.jm_byte_pos, payload.seam.bit_offset, payload.seam.source
    );
    println!(
        "Parent code: '{}' expected_children: {} shifts: {}..={}",
        payload.parent_code, payload.expected_children, payload.shift_start, payload.shift_end
    );
    println!(
        "{:>5} | {:>14} | {:>10} | {:>24} | {:>6}",
        "Shift", "Status", "Children", "Child codes", "Mode"
    );
    println!("{:-<76}", "");

    for probe in &payload.probes {
        let codes = if probe.child_codes.is_empty() {
            "-".to_string()
        } else {
            probe.child_codes.join(",")
        };
        let status = match probe.status {
            ProbeStatus::Ok => "ok",
            ProbeStatus::FallbackParent => "fallback",
            ProbeStatus::NoParent => "no_parent",
            ProbeStatus::ParseError => "parse_error",
        };
        let mode = probe.mode.as_deref().unwrap_or("-");
        println!(
            "{:>5} | {:>14} | {:>2}/{:<7} | {:>24} | {:>6}",
            probe.shift,
            status,
            probe.parsed_children,
            probe.expected_children,
            truncate_codes(&codes, 24),
            mode
        );
    }

    if let Some(best) = payload.probes.iter().find(|p| {
        matches!(p.status, ProbeStatus::Ok) && p.parsed_children == payload.expected_children
    }) {
        println!(
            "\nFirst exact match: shift={} children={}/{} codes={}",
            best.shift,
            best.parsed_children,
            best.expected_children,
            if best.child_codes.is_empty() {
                "-".to_string()
            } else {
                best.child_codes.join(",")
            }
        );
    }

    Ok(())
}

fn discover_seam(bits: &[bool], parent: &Item, anchor: u64) -> SeamDiscovery {
    let rel_parent_start = parent.range.start.saturating_sub(anchor);
    let search_start = rel_parent_start.saturating_add(100) as usize;

    let marker_seam = find_marker(bits, search_start).map(|bit| bit as u64);
    let term_seam = find_terminator_at(bits, search_start).map(|bit| bit as u64);

    let (bit_offset, source) = match (marker_seam, term_seam) {
        (Some(m), Some(t)) => {
            if m < t {
                (m, "marker_jm".to_string())
            } else {
                (t, "terminator_0x1ff".to_string())
            }
        }
        (Some(m), None) => (m, "marker_jm".to_string()),
        (None, Some(t)) => (t, "terminator_0x1ff".to_string()),
        (None, None) => (
            parent.range.end.saturating_sub(anchor),
            "item_range_end_fallback".to_string(),
        ),
    };

    SeamDiscovery {
        bit_offset,
        source,
        marker_bit: marker_seam,
        terminator_bit: term_seam,
        fallback_bit: parent.range.end.saturating_sub(anchor),
    }
}

fn select_parent<'a>(items: &'a [Item], parent_code: &str) -> Option<&'a Item> {
    find_parent(items, parent_code)
        .or_else(|| {
            items
                .iter()
                .find(|it| it.header.is_runeword && !it.socketed_items.is_empty())
        })
        .or_else(|| items.first())
}

fn fallback_parent<'a>(items: &'a [Item]) -> Option<&'a Item> {
    items
        .iter()
        .find(|it| it.header.is_runeword && !it.socketed_items.is_empty())
        .or_else(|| items.first())
}

fn find_parent<'a>(items: &'a [Item], parent_code: &str) -> Option<&'a Item> {
    items.iter().find(|it| code_matches(it.code(), parent_code))
}

fn code_matches(actual: &str, expected: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(expected.trim())
}

fn inject_bits(original: &[u8], at_bit: u64, count: u32) -> Vec<u8> {
    let mut bits = Vec::new();
    for &b in original {
        for i in 0..8 {
            bits.push((b >> i) & 1 == 1);
        }
    }

    let mut new_bits = Vec::new();
    if at_bit > bits.len() as u64 {
        return original.to_vec();
    }
    let at_bit = at_bit as usize;

    new_bits.extend_from_slice(&bits[..at_bit]);
    for _ in 0..count {
        new_bits.push(false); // Inject zeros
    }
    new_bits.extend_from_slice(&bits[at_bit..]);

    // Convert back to bytes
    let mut out_bytes = Vec::new();
    for chunk in new_bits.chunks(8) {
        let mut b = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                b |= 1 << i;
            }
        }
        out_bytes.push(b);
    }
    out_bytes
}

fn to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::new();
    for &b in bytes {
        for i in 0..8 {
            bits.push((b >> i) & 1 == 1);
        }
    }
    bits
}

fn find_marker(bits: &[bool], start_at: usize) -> Option<usize> {
    if bits.len() < 16 || start_at + 16 > bits.len() {
        return None;
    }
    // JM marker is 0x4A, 0x4D
    // In bits (LSB first):
    // 0x4A = 01001010 => 0, 1, 0, 1, 0, 0, 1, 0
    // 0x4D = 01001101 => 1, 0, 1, 1, 0, 0, 1, 0
    let marker_bits = [
        false, true, false, true, false, false, true, false, true, false, true, true, false, false,
        true, false,
    ];

    for i in (start_at..=(bits.len() - 16)).step_by(1) {
        if bits[i..i + 16] == marker_bits {
            return Some(i);
        }
    }
    None
}

fn find_terminator_at(bits: &[bool], start_at: usize) -> Option<usize> {
    if bits.len() < 9 || start_at + 9 > bits.len() {
        return None;
    }
    for i in start_at..=(bits.len() - 9) {
        let mut all_ones = true;
        for j in 0..9 {
            if !bits[i + j] {
                all_ones = false;
                break;
            }
        }
        if all_ones {
            return Some(i);
        }
    }
    None
}

fn truncate_codes(codes: &str, max_chars: usize) -> String {
    if codes.chars().count() <= max_chars {
        return codes.to_string();
    }

    let mut out = String::new();
    for ch in codes.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn filter_trace(
    traces: &[d2r_core::domain::stats::parser::SocketRecoveryTraceEvent],
    stage: &str,
) -> Vec<ProbeTraceEvent> {
    traces
        .iter()
        .filter(|t| t.stage == stage)
        .map(|t| ProbeTraceEvent {
            current_rel_pos: t.current_rel_pos,
            next_marker: t.next_marker,
            observed_marker: t.observed_marker,
            note: t.note.to_string(),
        })
        .collect()
}
