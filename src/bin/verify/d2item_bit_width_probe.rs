use anyhow::{Context, Result};
use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::stat_costs::STAT_COSTS;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::item::quality::ItemQuality;
use d2r_core::domain::stats::StatsAxiom;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone, Copy)]
enum WidthSource {
    ItemOverrides,
    Default9ShortCircuit,
    MappingSaveBits,
    StatsWidth,
    DefaultWidth,
}

impl WidthSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::ItemOverrides => "item_overrides",
            Self::Default9ShortCircuit => "default_9_short_circuit",
            Self::MappingSaveBits => "mapping_save_bits",
            Self::StatsWidth => "stats_width",
            Self::DefaultWidth => "default_width",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct WidthProbeRow {
    item_label: String,
    item_code: String,
    bit_offset: u64,
    raw_stat_id: u32,
    chosen_width: u32,
    winning_source: String,
    override_width: Option<u32>,
    mapping_width: Option<u32>,
    stats_width: Option<u32>,
    default_width: u32,
}

#[derive(Debug, Clone, Serialize)]
struct WidthProbeReport {
    fixture: String,
    base_bit: u64,
    window_bits: u64,
    window_start_bit: u64,
    window_end_bit: u64,
    max_depth: usize,
    selected_item_count: usize,
    candidate_row_count: usize,
    emitted_row_count: usize,
    mismatching_stat_ids: Vec<u32>,
    winning_source_tally: BTreeMap<String, usize>,
    rows: Vec<WidthProbeRow>,
}

#[derive(Debug, Clone)]
struct WidthResolution {
    chosen_width: u32,
    winning_source: WidthSource,
    override_width: Option<u32>,
    mapping_width: Option<u32>,
    stats_width: Option<u32>,
}

#[derive(Debug, Clone)]
struct WidthProbeSummary {
    report: WidthProbeReport,
    summary_path: PathBuf,
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if is_report_invocation(&args) {
        run_report_mode(args)
    } else {
        run_legacy_mode(args)
    }
}

fn is_report_invocation(args: &[std::ffi::OsString]) -> bool {
    args.iter().any(|arg| {
        let s = arg.to_string_lossy();
        s == "--file"
            || s.starts_with("--file=")
            || s == "-f"
            || s == "--base-bit"
            || s.starts_with("--base-bit=")
            || s == "--window"
            || s.starts_with("--window=")
            || s == "--max-depth"
            || s.starts_with("--max-depth=")
            || s == "--output"
            || s.starts_with("--output=")
            || s == "--json"
    })
}

fn run_report_mode(args: Vec<std::ffi::OsString>) -> Result<()> {
    let mut parser = ArgParser::new("d2item_bit_width_probe").description(
        "Hardened Alpha v105 bit-width probe that emits a machine-readable width-source report.",
    );
    parser.add_spec(
        ArgSpec::option(
            "file",
            Some('f'),
            Some("file"),
            "Path to the .d2s save file",
        )
        .required(),
    );
    parser.add_spec(
        ArgSpec::option(
            "base_bit",
            None,
            Some("base-bit"),
            "Absolute bit offset of the frontier",
        )
        .required(),
    );
    parser.add_spec(
        ArgSpec::option("window_bits", None, Some("window"), "Window size in bits")
            .with_default("96"),
    );
    parser.add_spec(
        ArgSpec::option("max_depth", None, Some("max-depth"), "Maximum rows to emit")
            .with_default("8"),
    );
    parser.add_spec(ArgSpec::option(
        "output",
        Some('o'),
        Some("output"),
        "Path to write the JSON report",
    ));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Emit machine-readable JSON to stdout when no output file is supplied",
    ));

    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            process::exit(1);
        }
    };

    let file = parsed.get("file").unwrap().to_string();
    let base_bit: u64 = parsed
        .get("base_bit")
        .unwrap()
        .parse()
        .context("base-bit must be numeric")?;
    let window_bits: u64 = parsed
        .get("window_bits")
        .unwrap()
        .parse()
        .context("window must be numeric")?;
    let max_depth: usize = parsed
        .get("max_depth")
        .unwrap()
        .parse()
        .context("max-depth must be numeric")?;
    let json = parsed.is_json();
    let user_supplied_output = parsed.get("output").is_some();
    let output_path = parsed.get("output").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("agent_artifacts/2026-06-02-2853-width-source-probe.json")
    });

    let report = build_width_source_report(&file, base_bit, window_bits, max_depth, &output_path)?;
    write_report_artifacts(&report, &output_path, json, user_supplied_output)?;

    if json && !user_supplied_output {
        println!("{}", serde_json::to_string_pretty(&report.report)?);
    } else {
        println!(
            "Wrote width-source report to {} (rows: {}, mismatching ids: {})",
            output_path.display(),
            report.report.emitted_row_count,
            report.report.mismatching_stat_ids.len()
        );
    }

    Ok(())
}

fn write_report_artifacts(
    summary: &WidthProbeSummary,
    output_path: &Path,
    _json_to_stdout: bool,
    _user_supplied_output: bool,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(&summary.report)?;
    fs::write(output_path, json.as_bytes())
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    let summary_text = render_summary_markdown(&summary.report);
    if let Some(parent) = summary.summary_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::write(&summary.summary_path, summary_text.as_bytes())
        .with_context(|| format!("Failed to write {}", summary.summary_path.display()))?;

    Ok(())
}

fn render_summary_markdown(report: &WidthProbeReport) -> String {
    let mut out = String::new();
    out.push_str("# d2item_bit_width_probe width-source report\n\n");
    out.push_str(&format!("- Fixture: `{}`\n", report.fixture));
    out.push_str(&format!(
        "- Probed window: base bit `{}`, window `{}` bits (`{}`..=`{}`)\n",
        report.base_bit, report.window_bits, report.window_start_bit, report.window_end_bit
    ));
    out.push_str(&format!("- Max depth: `{}`\n", report.max_depth));
    out.push_str(&format!(
        "- Selected items: `{}`, candidate rows: `{}`, emitted rows: `{}`\n",
        report.selected_item_count, report.candidate_row_count, report.emitted_row_count
    ));
    out.push_str("- Mismatching stat ids: ");
    if report.mismatching_stat_ids.is_empty() {
        out.push_str("`none`\n");
    } else {
        let ids = report
            .mismatching_stat_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("`{}`\n", ids));
    }
    out.push_str("\n## Winning-source Tally\n");
    for (source, count) in &report.winning_source_tally {
        out.push_str(&format!("- `{}`: `{}`\n", source, count));
    }
    out.push_str("\n## Deferred Action\n");
    out.push_str(
        "Registry patching is deferred for the next slice; this report only exposes the live width-precedence branch.\n",
    );
    out
}

fn build_width_source_report(
    file_path: &str,
    base_bit: u64,
    window_bits: u64,
    max_depth: usize,
    output_path: &Path,
) -> Result<WidthProbeSummary> {
    let bytes =
        fs::read(file_path).with_context(|| format!("Failed to read file: {}", file_path))?;
    let huffman = HuffmanTree::new();
    let is_alpha = bytes.get(4..8) == Some(&[0x69, 0, 0, 0]);
    let items = Item::read_player_items(&bytes, &huffman, is_alpha)
        .with_context(|| format!("Failed to read player items from {}", file_path))?;

    let window_start_bit = base_bit.saturating_sub(window_bits);
    let window_end_bit = base_bit.saturating_add(window_bits);

    let frontier_candidates: Vec<&Item> = items
        .iter()
        .filter(|item| matches!(item.code().trim(), "wsp" | "ww" | "buc"))
        .collect();
    let selected_item_count = frontier_candidates.len();
    let target_item = frontier_candidates
        .iter()
        .min_by_key(|item| range_distance(base_bit, item.range.start, item.range.end))
        .copied()
        .or_else(|| {
            items
                .iter()
                .filter(|item| {
                    ranges_overlap(
                        item.range.start,
                        item.range.end,
                        window_start_bit,
                        window_end_bit,
                    )
                })
                .min_by_key(|item| range_distance(base_bit, item.range.start, item.range.end))
        })
        .or_else(|| items.first())
        .context("No item context available for width-source probe")?;

    let item_code = target_item.code().trim().to_string();
    let item_label = if item_code.is_empty() {
        format!("item@{}", target_item.range.start)
    } else {
        item_code.clone()
    };
    let axiom = StatsAxiom::new(
        target_item.header.version,
        target_item.header.quality.unwrap_or(ItemQuality::Normal),
        target_item.header.save_is_alpha,
    )
    .with_compact(target_item.header.is_compact)
    .with_code(target_item.code());

    let mut paths = Vec::new();
    for id_bits in [9, 10, 11] {
        let start_range = if base_bit > window_bits {
            base_bit - window_bits
        } else {
            0
        };
        let end_range = base_bit + window_bits;

        for bit in start_range..=end_range {
            let path = explore_path(&bytes, bit, id_bits, max_depth)?;
            if !path.stats.is_empty() {
                paths.push(path);
            }
        }
    }

    paths.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.stats.len().cmp(&a.stats.len()))
    });

    let best_path = paths
        .first()
        .cloned()
        .context("No viable probe path found in the requested window")?;

    let candidate_row_count = best_path.stats.len();
    let rows: Vec<WidthProbeRow> = best_path
        .stats
        .into_iter()
        .take(max_depth)
        .map(|stat| {
            let default_width = compute_default_width(stat.id);
            let resolution = resolve_width_source(&axiom, stat.id, default_width);
            WidthProbeRow {
                item_label: item_label.clone(),
                item_code: item_code.clone(),
                bit_offset: stat.bit_offset,
                raw_stat_id: stat.id,
                chosen_width: resolution.chosen_width,
                winning_source: resolution.winning_source.as_str().to_string(),
                override_width: resolution.override_width,
                mapping_width: resolution.mapping_width,
                stats_width: resolution.stats_width,
                default_width,
            }
        })
        .collect();

    let mut winning_source_tally = BTreeMap::new();
    let mut mismatch_ids = BTreeSet::new();
    for row in &rows {
        *winning_source_tally
            .entry(row.winning_source.clone())
            .or_insert(0) += 1;
        if row.chosen_width != row.default_width {
            mismatch_ids.insert(row.raw_stat_id);
        }
    }

    let report = WidthProbeReport {
        fixture: file_path.to_string(),
        base_bit,
        window_bits,
        window_start_bit,
        window_end_bit,
        max_depth,
        selected_item_count,
        candidate_row_count,
        emitted_row_count: rows.len(),
        mismatching_stat_ids: mismatch_ids.into_iter().collect(),
        winning_source_tally,
        rows,
    };

    let summary_path = summary_path_for(output_path);
    Ok(WidthProbeSummary {
        report,
        summary_path,
    })
}

fn summary_path_for(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("width-source-probe");
    output_path.with_file_name(format!("{}-summary.md", stem))
}

fn compute_default_width(raw_id: u32) -> u32 {
    STAT_COSTS
        .iter()
        .find(|s| s.id == raw_id)
        .map(|s| s.save_bits as u32)
        .unwrap_or(9)
}

fn resolve_width_source(axiom: &StatsAxiom, raw_id: u32, default_width: u32) -> WidthResolution {
    let reg = get_registry();
    let trimmed = axiom.code.trim();

    let override_width = reg
        .item_overrides
        .as_ref()
        .and_then(|overrides| overrides.get(trimmed))
        .and_then(|item_map| item_map.get(&raw_id.to_string()).copied());
    let mapping_width = axiom
        .lookup_alpha_map_by_raw(raw_id)
        .and_then(|m| m.save_bits);
    let stats_width = reg.stats.get(&raw_id.to_string()).map(|s| s.width);

    if let Some(width) = override_width {
        return WidthResolution {
            chosen_width: width,
            winning_source: WidthSource::ItemOverrides,
            override_width,
            mapping_width,
            stats_width,
        };
    }

    if default_width == 9 && trimmed != "acww" {
        return WidthResolution {
            chosen_width: 9,
            winning_source: WidthSource::Default9ShortCircuit,
            override_width,
            mapping_width,
            stats_width,
        };
    }

    if let Some(width) = mapping_width {
        return WidthResolution {
            chosen_width: width,
            winning_source: WidthSource::MappingSaveBits,
            override_width,
            mapping_width,
            stats_width,
        };
    }

    if let Some(width) = stats_width {
        return WidthResolution {
            chosen_width: width,
            winning_source: WidthSource::StatsWidth,
            override_width,
            mapping_width,
            stats_width,
        };
    }

    WidthResolution {
        chosen_width: default_width,
        winning_source: WidthSource::DefaultWidth,
        override_width,
        mapping_width,
        stats_width,
    }
}

fn ranges_overlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> bool {
    a_start <= b_end && a_end >= b_start
}

fn range_distance(bit: u64, start: u64, end: u64) -> u64 {
    if bit < start {
        start - bit
    } else if bit > end {
        bit - end
    } else {
        0
    }
}

fn run_legacy_mode(args: Vec<std::ffi::OsString>) -> Result<()> {
    let save_file = args
        .first()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if args.len() < 3 {
        println!("Usage: d2item_bit_width_probe <save_file> debug_mana");
        println!("Usage: d2item_bit_width_probe <save_file> <search_id> [id_bits]");
        println!("Usage: d2item_bit_width_probe <save_file> <base_bit> [window_bits] [max_depth]");
        return Ok(());
    }

    let bytes = fs::read(&save_file)?;

    let second = args[1].to_string_lossy().to_string();
    if second == "debug_mana" {
        if let Some(cost) = STAT_COSTS.iter().find(|s| s.id == 8) {
            println!("ID 8 (Mana): save_bits = {}", cost.save_bits);
        }
        if let Some(cost) = STAT_COSTS.iter().find(|s| s.id == 9) {
            println!("ID 9 (Max Mana): save_bits = {}", cost.save_bits);
        }
        return Ok(());
    }

    let val2: u64 = second.parse().context("Invalid numeric argument")?;

    if val2 < 1024 {
        let search_id = val2 as u32;
        let id_bits: u32 = args
            .get(2)
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(9);
        println!("Searching for ID {} with {} bits...", search_id, id_bits);

        for bit in 7000..(bytes.len() as u64 * 8) {
            let start_byte = (bit / 8) as usize;
            let bit_offset = (bit % 8) as u32;
            if start_byte + 4 >= bytes.len() {
                break;
            }

            let mut reader = BitReader::endian(Cursor::new(&bytes[start_byte..]), LittleEndian);
            for _ in 0..bit_offset {
                let _ = reader.read_bit()?;
            }

            let id = read_bits(&mut reader, id_bits)?;
            if id == search_id {
                let path = explore_path(&bytes, bit, id_bits, 10)?;
                println!(
                    "Match at bit {}: Score {}, Stats {:?}",
                    bit,
                    path.score,
                    path.stats.iter().map(|s| &s.name).collect::<Vec<_>>()
                );
            }
        }
    } else {
        let base_bit = val2;
        let window: u64 = args
            .get(2)
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(16);
        let max_depth: usize = args
            .get(3)
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        println!("=== D2R Item Bit Width Probe Tool ===");
        println!("Target File: {}", save_file);
        println!(
            "Base Bit: {}, Window: +/-, Max Depth: {}\n",
            base_bit, window
        );

        let mut paths = Vec::new();

        for id_bits in [9, 10, 11] {
            let start_range = if base_bit > window {
                base_bit - window
            } else {
                0
            };
            let end_range = base_bit + window;

            for bit in start_range..=end_range {
                let path = explore_path(&bytes, bit, id_bits, max_depth)?;
                if !path.stats.is_empty() {
                    paths.push(path);
                }
            }
        }

        paths.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.stats.len().cmp(&a.stats.len()))
        });

        println!(
            "{:<5} | {:<7} | {:<7} | {:<7} | {:<10} | Stats",
            "Rank", "Score", "ID Bits", "Offset", "Term?"
        );
        println!(
            "{:-<5}-|-{:-<7}-|-{:-<7}-|-{:-<7}-|-{:-<10}-|-------",
            "", "", "", "", ""
        );

        for (i, path) in paths.iter().take(20).enumerate() {
            let stat_ids: Vec<String> = path
                .stats
                .iter()
                .map(|s| format!("{} ({})={}", s.id, s.name, s.value))
                .collect();
            println!(
                "{:<5} | {:<7} | {:<7} | {:<7} | {:<10} | [{}]",
                i + 1,
                path.score,
                path.id_bits,
                path.start_bit,
                if path.terminated { "YES" } else { "NO" },
                stat_ids.join(", ")
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct StatRead {
    id: u32,
    name: String,
    value: u32,
    bit_offset: u64,
}

#[derive(Debug, Clone)]
struct ProbePath {
    start_bit: u64,
    id_bits: u32,
    stats: Vec<StatRead>,
    score: i32,
    terminated: bool,
}

fn read_bits<R: BitRead>(reader: &mut R, n: u32) -> io::Result<u32> {
    let mut value = 0u32;
    for i in 0..n {
        if reader.read_bit()? {
            value |= 1 << i;
        }
    }
    Ok(value)
}

fn is_value_suspicious(id: u32, val: u32) -> bool {
    match id {
        92 => val > 110,
        12 => val > 511,
        7 => val > 1000,
        91 => val > 2000,
        16 | 25 | 31 => val > 1000,
        _ => false,
    }
}

fn get_signature_bonus(id: u32) -> i32 {
    match id {
        16 | 17 | 25 | 31 | 105 | 111 | 135 | 136 | 89 | 83 | 8 => 60,
        _ => 0,
    }
}

fn explore_path(
    bytes: &[u8],
    start_bit: u64,
    id_bits: u32,
    max_depth: usize,
) -> io::Result<ProbePath> {
    let start_byte = (start_bit / 8) as usize;
    let bit_offset = (start_bit % 8) as u32;

    if start_byte + 4 >= bytes.len() {
        return Ok(ProbePath {
            start_bit,
            id_bits,
            stats: Vec::new(),
            score: 0,
            terminated: false,
        });
    }

    let mut reader = BitReader::endian(Cursor::new(&bytes[start_byte..]), LittleEndian);
    for _ in 0..bit_offset {
        let _ = reader.read_bit()?;
    }
    let mut current_bit = start_bit + bit_offset as u64;

    let mut path = ProbePath {
        start_bit,
        id_bits,
        stats: Vec::new(),
        score: 0,
        terminated: false,
    };

    for _ in 0..max_depth {
        let stat_bit_offset = current_bit;
        let Ok(id) = read_bits(&mut reader, id_bits) else {
            break;
        };
        current_bit += id_bits as u64;

        let terminator = (1 << id_bits) - 1;
        if id == terminator {
            path.score += 100;
            path.terminated = true;
            break;
        }

        let maybe_cost = STAT_COSTS.iter().find(|s| s.id == id);
        if let Some(cost) = maybe_cost {
            path.score += 30;
            path.score += get_signature_bonus(id);

            let val = if cost.save_bits > 0 {
                read_bits(&mut reader, cost.save_bits as u32).unwrap_or(0)
            } else {
                0
            };
            current_bit += cost.save_bits as u64;
            let suspicious = is_value_suspicious(id, val);
            if suspicious {
                path.score += 5;
            }

            path.stats.push(StatRead {
                id,
                name: cost.name.to_string(),
                value: val,
                bit_offset: stat_bit_offset,
            });
        } else {
            break;
        }
    }

    Ok(path)
}
