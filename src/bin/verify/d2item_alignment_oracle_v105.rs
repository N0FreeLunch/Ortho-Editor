use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::domain::forensic::registry::{get_registry, AlphaForensics};
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Serialize, Debug)]
struct AlignmentPath {
    index: u8,
    offset: usize,
    stat_ids: Vec<u16>,
    terminated: bool,
}

fn read_bits(bytes: &[u8], start_bit: u64, bit_count: u64) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bit_count as usize);
    for i in 0..bit_count {
        let bit_pos = start_bit + i;
        let byte_idx = (bit_pos / 8) as usize;
        let bit_idx = (bit_pos % 8) as usize;
        if byte_idx < bytes.len() {
            let bit = (bytes[byte_idx] & (1 << bit_idx)) != 0;
            bits.push(bit);
        } else {
            break;
        }
    }
    bits
}

fn get_stat_width(reg: &AlphaForensics, stat_id: u16) -> u32 {
    let key = stat_id.to_string();
    if let Some(m) = reg.mappings.get(&key) {
        if let Some(bits) = m.save_bits {
            return bits;
        }
    }
    if let Some(s) = reg.stats.get(&key) {
        return s.width;
    }
    0
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_alignment_oracle_v105");
    parser.add_spec(ArgSpec::option(
        "fixture",
        Some('f'),
        Some("fixture"),
        "Path to the save game fixture",
    ));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixture_path = parsed.get("fixture").expect("Missing --fixture");
    let use_json = parsed.is_set("json");

    let bytes = fs::read(fixture_path)?;
    let reg = get_registry();

    let targets = vec![
        (4u8, 7661u64, 168u64),
        (5u8, 7829u64, 201u64),
        (8u8, 8752u64, 149u64),
        (14u8, 9674u64, 78u64),
    ];

    let mut all_paths = Vec::new();

    for &(index, start, len) in &targets {
        let payload_bits = read_bits(&bytes, start, len);
        
        for offset in 0..=64 {
            if offset >= payload_bits.len() {
                break;
            }
            
            let mut c = offset;
            let mut decoded_stat_ids = Vec::new();
            let mut terminated = false;
            let mut valid = true;

            while c < payload_bits.len() {
                if c + 9 > payload_bits.len() {
                    // Not enough bits to read next stat ID
                    break;
                }

                // Read 9-bit stat ID
                let mut stat_id = 0u16;
                for i in 0..9 {
                    if payload_bits[c + i] {
                        stat_id |= 1 << i;
                    }
                }

                if stat_id == 511 {
                    terminated = true;
                    break;
                }

                // Check valid Stat ID (excluding 511, so < 511)
                if stat_id >= 511 {
                    valid = false;
                    break;
                }

                decoded_stat_ids.push(stat_id);
                let val_width = get_stat_width(reg, stat_id);
                c += 9 + val_width as usize;
            }

            if valid && (!decoded_stat_ids.is_empty() || terminated) {
                all_paths.push(AlignmentPath {
                    index,
                    offset,
                    stat_ids: decoded_stat_ids,
                    terminated,
                });
            }
        }
    }

    if use_json {
        println!("{}", serde_json::to_string_pretty(&all_paths)?);
    } else {
        for path in &all_paths {
            println!(
                "Index {}: Offset {}, terminated={}, stats={:?}",
                path.index, path.offset, path.terminated, path.stat_ids
            );
        }
    }

    Ok(())
}
