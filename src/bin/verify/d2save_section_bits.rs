use d2r_core::verify::args::{ArgParser, ArgError};
use d2r_core::save::{map_core_sections, find_jm_markers};
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Serialize)]
struct BitDumpResult {
    anchor: String,
    anchor_bit_pos: u64,
    target_bit_start: u64,
    offset: i64,
    length: usize,
    bits: String,
    hex: String,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_section_bits")
        .description("Dump raw bits relative to section anchors (if, gf, JM)");

    parser.add_arg("save_file", "path to the save file (.d2s)");
    parser.add_opt("anchor", "anchor name (if, gf, jm0, jm1, ...)")
        .short('a')
        .long("anchor")
        .default("gf");
    parser.add_opt("offset", "bit offset relative to anchor")
        .short('o')
        .long("offset")
        .default("0");
    parser.add_opt("length", "number of bits to dump")
        .short('l')
        .long("length")
        .default("64");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let path = parsed.get("save_file").unwrap();
    let anchor_name = parsed.get("anchor").unwrap();
    let bit_offset: i64 = parsed.get("offset").unwrap().parse()?;
    let bit_length: usize = parsed.get("length").unwrap().parse()?;
    let is_json = parsed.is_json();

    let bytes = fs::read(path)?;
    let map = map_core_sections(&bytes).map_err(|e| anyhow::anyhow!("Failed to map sections: {}", e))?;
    let jm_markers = find_jm_markers(&bytes);

    let anchor_bit_pos = match anchor_name.to_lowercase().as_str() {
        "gf" => (map.gf_pos as u64) * 8,
        "if" => (map.if_pos as u64) * 8,
        "woo" => (map.woo_pos.unwrap_or(0) as u64) * 8,
        "ws" => (map.ws_pos.unwrap_or(0) as u64) * 8,
        "w4" => (map.w4_pos.unwrap_or(0) as u64) * 8,
        "jf" => (map.jf_pos.unwrap_or(0) as u64) * 8,
        "kf" => (map.kf_pos.unwrap_or(0) as u64) * 8,
        "lf" => (map.lf_pos.unwrap_or(0) as u64) * 8,
        name if name.starts_with("jm") => {
            let idx: usize = name[2..].parse().map_err(|_| anyhow::anyhow!("Invalid JM index: {}", name))?;
            let pos = jm_markers.get(idx).ok_or_else(|| anyhow::anyhow!("JM marker index {} not found", idx))?;
            (*pos as u64) * 8
        }
        _ => anyhow::bail!("Unknown anchor: {}", anchor_name),
    };

    let target_start = (anchor_bit_pos as i64 + bit_offset) as u64;

    let mut bits_str = String::new();
    let mut hex_bytes = Vec::new();
    let mut current_byte: u8 = 0;
    let mut bits_in_current_byte = 0;

    for i in 0..bit_length {
        let bit_pos = target_start + i as u64;
        let byte_idx = (bit_pos / 8) as usize;
        let bit_idx = (bit_pos % 8) as usize;

        if byte_idx >= bytes.len() {
            break;
        }

        let bit = (bytes[byte_idx] >> bit_idx) & 1;
        bits_str.push(if bit == 1 { '1' } else { '0' });

        current_byte |= bit << bits_in_current_byte;
        bits_in_current_byte += 1;

        if bits_in_current_byte == 8 {
            hex_bytes.push(current_byte);
            current_byte = 0;
            bits_in_current_byte = 0;
        }
    }
    if bits_in_current_byte > 0 {
        hex_bytes.push(current_byte);
    }

    if is_json {
        let result = BitDumpResult {
            anchor: anchor_name.clone(),
            anchor_bit_pos,
            target_bit_start: target_start,
            offset: bit_offset,
            length: bit_length,
            bits: bits_str,
            hex: hex::encode(hex_bytes),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Anchor: {} at bit {}", anchor_name, anchor_bit_pos);
        println!("Dumping {} bits from bit {} (relative {})...", bit_length, target_start, bit_offset);
        println!("{:-<60}", "");
        
        for (i, c) in bits_str.chars().enumerate() {
            print!("{}", c);
            if (i + 1) % 8 == 0 {
                print!(" ");
            }
            if (i + 1) % 64 == 0 {
                let bit_pos = target_start + i as u64;
                println!(" | bit {}", bit_pos);
            }
        }
        println!("\n{:-<60}", "");
        println!("Hex: {}", hex::encode(hex_bytes));
    }

    Ok(())
}
