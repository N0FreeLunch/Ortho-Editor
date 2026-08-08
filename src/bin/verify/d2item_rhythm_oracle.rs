use bitstream_io::LittleEndian;
use d2r_core::data::stat_costs::STAT_COSTS;
use d2r_core::domain::item::quality::ItemQuality;
use d2r_core::domain::stats::StatsAxiom;
use d2r_core::verify::args::{ArgError, ArgParser};

#[derive(Clone, serde::Serialize)]
struct Candidate {
    offset: u64,
    end_offset: u64,
    stats_read: usize,
    terminator_found: bool,
    parity_gap: u64,
    score: i32,
    status: &'static str,
    trace: Vec<String>,
}

#[derive(serde::Serialize)]
struct ScanInput<'a> {
    file: &'a str,
    depth: usize,
    version: u8,
    alpha: bool,
    runeword: bool,
    code: &'a str,
}

#[derive(serde::Serialize)]
struct ScanWindow {
    start_bit: u64,
    end_bit_exclusive: u64,
}

#[derive(serde::Serialize)]
struct ScanReport<'a> {
    schema_version: u32,
    input: ScanInput<'a>,
    scan: ScanWindow,
    best_candidate: Option<Candidate>,
    candidates: Vec<Candidate>,
}

fn write_json_output(
    value: &impl serde::Serialize,
    output_path: Option<&String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(value)?;
    if let Some(path) = output_path {
        std::fs::write(path, json)?;
    } else {
        println!("{}", json);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = ArgParser::new("d2item_rhythm_oracle")
        .description("Scores bitstreams by validating 9-bit stat ID rhythms and searching for the 0x1FF terminator.");

    parser
        .add_arg("file", "The save file to analyze")
        .required();
    parser
        .add_opt("offset", "The bit offset to start analysis")
        .short('o')
        .long("offset")
        .default("0");
    parser
        .add_opt("depth", "Maximum number of stats to read")
        .short('d')
        .long("depth")
        .default("32");
    parser
        .add_opt("version", "The game version (default 5 for Alpha)")
        .short('v')
        .long("version")
        .default("5");
    parser
        .add_flag("alpha", "Whether to use Alpha mode")
        .short('a')
        .long("alpha")
        .default("true");
    parser
        .add_flag("runeword", "Whether the item is a runeword")
        .short('r')
        .long("runeword");
    parser
        .add_flag("scan", "Scan a range of offsets for the best score")
        .short('s')
        .long("scan");
    parser
        .add_opt("range", "Range to scan (e.g. 100)")
        .short('R')
        .long("range")
        .default("100");
    parser
        .add_opt("code", "The item code (for context-aware rhythms)")
        .short('c')
        .long("code")
        .default("Opaque");

    let args = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(a) => a,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let file_path = args.get("file").unwrap();
    let start_bit_offset: u64 = args.get("offset").unwrap().parse()?;
    let max_depth: usize = args.get("depth").unwrap().parse()?;
    let version: u8 = args.get("version").unwrap().parse()?;
    let is_alpha = args.is_set("alpha");
    let is_runeword = args.is_set("runeword");
    let is_scan = args.is_set("scan");
    let scan_range: u64 = args.get("range").unwrap().parse()?;
    let item_code = args.get("code").unwrap();

    let file_bytes = std::fs::read(file_path)?;

    let axiom = StatsAxiom::new(version, ItemQuality::Normal, is_alpha).with_code(item_code);

    let mut best_score = -1;
    let mut best_offset = 0;
    let mut results = Vec::new();

    let offsets = if is_scan {
        start_bit_offset..(start_bit_offset + scan_range)
    } else {
        start_bit_offset..(start_bit_offset + 1)
    };
    let scan_end_exclusive = is_scan.then_some(start_bit_offset + scan_range);

    for offset in offsets {
        let mut score = 0;
        let mut terminator_found = false;
        let mut stats_read = 0;
        let mut trace = Vec::new();

        // Simple bit reader for the loop
        let read_bits_fn = |pos: &mut u64, count: u32, data: &[u8]| -> Option<u32> {
            let end = pos.checked_add(count as u64)?;
            if scan_end_exclusive.is_some_and(|limit| end > limit) {
                return None;
            }
            let mut val: u32 = 0;
            for i in 0..count {
                let abs = *pos + i as u64;
                let byte = (abs / 8) as usize;
                let bit = (abs % 8) as u8;
                if byte >= data.len() {
                    return None;
                }
                if (data[byte] & (1 << bit)) != 0 {
                    val |= 1 << i;
                }
            }
            *pos += count as u64;
            Some(val)
        };

        let mut temp_pos = offset;
        for _ in 0..max_depth {
            let id_bits = 9;
            let start_pos = temp_pos;
            let stat_id = match read_bits_fn(&mut temp_pos, id_bits, &file_bytes) {
                Some(id) => id,
                None => break,
            };

            if stat_id == 511 {
                terminator_found = true;
                score += 100;
                trace.push(format!("Terminator (0x1FF) at bit {}", start_pos));
                break;
            }

            if stat_id > 511 {
                break;
            }

            let effective_id = axiom.map_alpha_id(stat_id);
            let (val_bits, param_bits) =
                if let Some(stat) = STAT_COSTS.iter().find(|s| s.id == effective_id) {
                    let rhythm = axiom.property_rhythm(is_runeword, false, false, stat_id);
                    let v = rhythm.value_bits.unwrap_or(stat.save_bits as u32);
                    (v, stat.save_param_bits as u32)
                } else {
                    (9, 0)
                };

            if let None = read_bits_fn(&mut temp_pos, param_bits, &file_bytes) {
                break;
            }
            if let None = read_bits_fn(&mut temp_pos, val_bits, &file_bytes) {
                break;
            }

            stats_read += 1;
            score += 10;
            trace.push(format!(
                "Stat ID {} ({}) at bit {}",
                stat_id, effective_id, start_pos
            ));
        }

        let parity_gap = temp_pos % 8;
        if terminator_found && parity_gap == 0 {
            score += 50;
        }

        if score > best_score {
            best_score = score;
            best_offset = offset;
        }

        let status = if !terminator_found || score < 50 {
            "Likely Ghost Code"
        } else {
            "Valid Rhythm"
        };

        results.push(Candidate {
            offset,
            end_offset: temp_pos,
            stats_read,
            terminator_found,
            parity_gap,
            score,
            status,
            trace,
        });
    }

    if is_scan && args.is_json() {
        let best_candidate = results
            .iter()
            .find(|candidate| candidate.offset == best_offset)
            .cloned();
        let report = ScanReport {
            schema_version: 1,
            input: ScanInput {
                file: file_path,
                depth: max_depth,
                version,
                alpha: is_alpha,
                runeword: is_runeword,
                code: item_code,
            },
            scan: ScanWindow {
                start_bit: start_bit_offset,
                end_bit_exclusive: start_bit_offset + scan_range,
            },
            best_candidate,
            candidates: results,
        };
        write_json_output(&report, args.get("output"))?;
        return Ok(());
    }

    if is_scan {
        println!(
            "Scan results for range {}..{}:",
            start_bit_offset,
            start_bit_offset + scan_range
        );
        println!("Best offset: {} (Score: {})", best_offset, best_score);
        println!("------------------------------------");
    }

    for candidate in results {
        if is_scan && candidate.score < best_score && !candidate.terminator_found {
            continue;
        }

        if args.is_json() {
            write_json_output(&candidate, args.get("output"))?;
        } else {
            println!("Analysis for offset: {}", candidate.offset);
            println!("------------------------------------");
            println!("Stats Read:       {}", candidate.stats_read);
            println!("Terminator Found: {}", candidate.terminator_found);
            println!("Parity Gap:       {}", candidate.parity_gap);
            println!("Fidelity Score:   {}", candidate.score);
            println!("Status:           {}", candidate.status);
            println!("End Offset:       {}", candidate.end_offset);
            println!("\nTrace:");
            for t in candidate.trace {
                println!("  {}", t);
            }
            println!("\n");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> ScanReport<'static> {
        let candidate = Candidate {
            offset: 9674,
            end_offset: 9683,
            stats_read: 0,
            terminator_found: true,
            parity_gap: 3,
            score: 160,
            status: "Valid Rhythm",
            trace: vec!["Terminator (0x1FF) at bit 9674".to_string()],
        };
        ScanReport {
            schema_version: 1,
            input: ScanInput {
                file: "fixture.d2s",
                depth: 16,
                version: 5,
                alpha: true,
                runeword: false,
                code: "wyws",
            },
            scan: ScanWindow {
                start_bit: 9674,
                end_bit_exclusive: 9752,
            },
            best_candidate: Some(candidate.clone()),
            candidates: vec![candidate],
        }
    }

    #[test]
    fn scan_report_json_has_required_contract() {
        let value = serde_json::to_value(sample_report()).expect("report should serialize");
        for key in [
            "schema_version",
            "input",
            "scan",
            "best_candidate",
            "candidates",
        ] {
            assert!(value.get(key).is_some(), "missing top-level key: {key}");
        }
        assert_eq!(value["scan"]["start_bit"], 9674);
        assert_eq!(value["scan"]["end_bit_exclusive"], 9752);

        let candidate = &value["candidates"][0];
        for key in [
            "offset",
            "end_offset",
            "stats_read",
            "terminator_found",
            "parity_gap",
            "score",
            "status",
            "trace",
        ] {
            assert!(candidate.get(key).is_some(), "missing candidate key: {key}");
        }
    }

    #[test]
    fn write_json_output_creates_parseable_file() {
        let path = std::env::temp_dir().join(format!(
            "d2item-rhythm-oracle-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should follow Unix epoch")
                .as_nanos()
        ));
        let path_string = path.to_string_lossy().into_owned();

        write_json_output(&sample_report(), Some(&path_string)).expect("output should be written");
        let bytes = std::fs::read(&path).expect("output file should exist");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("output should contain valid JSON");
        assert_eq!(value["best_candidate"]["offset"], 9674);
        assert_eq!(value["best_candidate"]["score"], 160);

        std::fs::remove_file(path).expect("temporary output should be removable");
    }
}
