use bitstream_io::{LittleEndian};
use d2r_core::verify::args::{ArgParser, ArgError};
use d2r_core::data::stat_costs::STAT_COSTS;
use d2r_core::domain::stats::StatsAxiom;
use d2r_core::domain::item::quality::ItemQuality;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = ArgParser::new("d2item_rhythm_oracle")
        .description("Scores bitstreams by validating 9-bit stat ID rhythms and searching for the 0x1FF terminator.");
    
    parser.add_arg("file", "The save file to analyze").required();
    parser.add_opt("offset", "The bit offset to start analysis").short('o').long("offset").default("0");
    parser.add_opt("depth", "Maximum number of stats to read").short('d').long("depth").default("32");
    parser.add_opt("version", "The game version (default 5 for Alpha)").short('v').long("version").default("5");
    parser.add_flag("alpha", "Whether to use Alpha mode").short('a').long("alpha").default("true");
    parser.add_flag("runeword", "Whether the item is a runeword").short('r').long("runeword");
    parser.add_flag("scan", "Scan a range of offsets for the best score").short('s').long("scan");
    parser.add_opt("range", "Range to scan (e.g. 100)").short('R').long("range").default("100");
    parser.add_opt("code", "The item code (for context-aware rhythms)").short('c').long("code").default("Opaque");

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
    
    let axiom = StatsAxiom::new(version, ItemQuality::Normal, is_alpha)
        .with_code(item_code);

    let mut best_score = -1;
    let mut best_offset = 0;
    let mut results = Vec::new();

    let offsets = if is_scan {
        start_bit_offset..(start_bit_offset + scan_range)
    } else {
        start_bit_offset..(start_bit_offset + 1)
    };

    for offset in offsets {
        let mut score = 0;
        let mut terminator_found = false;
        let mut stats_read = 0;
        let mut trace = Vec::new();

        // Simple bit reader for the loop
        let read_bits_fn = |pos: &mut u64, count: u32, data: &[u8]| -> Option<u32> {
            let mut val: u32 = 0;
            for i in 0..count {
                let abs = *pos + i as u64;
                let byte = (abs / 8) as usize;
                let bit = (abs % 8) as u8;
                if byte >= data.len() { return None; }
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
            let (val_bits, param_bits) = if let Some(stat) = STAT_COSTS.iter().find(|s| s.id == effective_id) {
                let rhythm = axiom.property_rhythm(is_runeword, false, false, stat_id);
                let v = rhythm.value_bits.unwrap_or(stat.save_bits as u32);
                (v, stat.save_param_bits as u32)
            } else {
                (9, 0)
            };

            if let None = read_bits_fn(&mut temp_pos, param_bits, &file_bytes) { break; }
            if let None = read_bits_fn(&mut temp_pos, val_bits, &file_bytes) { break; }

            stats_read += 1;
            score += 10;
            trace.push(format!("Stat ID {} ({}) at bit {}", stat_id, effective_id, start_pos));
        }

        let parity_gap = temp_pos % 8;
        if terminator_found && parity_gap == 0 {
            score += 50;
        }

        if score > best_score {
            best_score = score;
            best_offset = offset;
        }

        if !is_scan || terminator_found {
            results.push((offset, temp_pos, stats_read, terminator_found, parity_gap, score, trace));
        }
    }

    if is_scan {
        println!("Scan results for range {}..{}:", start_bit_offset, start_bit_offset + scan_range);
        println!("Best offset: {} (Score: {})", best_offset, best_score);
        println!("------------------------------------");
    }

    for (offset, end_pos, stats_read, terminator_found, parity_gap, score, trace) in results {
        if is_scan && score < best_score && !terminator_found { continue; }
        
        let is_ghost = if !terminator_found || score < 50 {
            "Likely Ghost Code"
        } else {
            "Valid Rhythm"
        };

        if args.is_json() {
            let output = serde_json::json!({
                "offset": offset,
                "end_offset": end_pos,
                "stats_read": stats_read,
                "terminator_found": terminator_found,
                "parity_gap": parity_gap,
                "score": score,
                "status": is_ghost,
                "trace": trace,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("Analysis for offset: {}", offset);
            println!("------------------------------------");
            println!("Stats Read:       {}", stats_read);
            println!("Terminator Found: {}", terminator_found);
            println!("Parity Gap:       {}", parity_gap);
            println!("Fidelity Score:   {}", score);
            println!("Status:           {}", is_ghost);
            println!("End Offset:       {}", end_pos);
            println!("\nTrace:");
            for t in trace {
                println!("  {}", t);
            }
            println!("\n");
        }
    }

    Ok(())
}
