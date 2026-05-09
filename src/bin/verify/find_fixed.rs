use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::fs;
use std::io::Cursor;
use std::env;
use std::process;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn read_bits<R: BitRead>(reader: &mut R, n: u32) -> u32 {
    let mut value = 0u32;
    for i in 0..n {
        if let Ok(b) = reader.read_bit() {
            if b {
                value |= 1 << i;
            }
        }
    }
    value
}

fn main() {
    let mut parser = ArgParser::new("d2item_find_fixed")
        .description("Exploratory tool to sweep start_bit and id_bits for fixed-width properties (Total 24bits)");

    parser.add_arg("save_file", "path to the save file to inspect");
    parser.add_arg("start_bit_min", "inclusive lower bound for the sweep");
    parser.add_arg("start_bit_max", "inclusive upper bound for the sweep");
    parser.add_opt("id_bits_min", "optional lower bound for stat-id width (default: 7)");
    parser.add_opt("id_bits_max", "optional upper bound for stat-id width (default: 11)");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let save_file = parsed.get("save_file").unwrap();
    let start_bit_min: u32 = parsed.get("start_bit_min").unwrap().parse().expect("start_bit_min must be a number");
    let start_bit_max: u32 = parsed.get("start_bit_max").unwrap().parse().expect("start_bit_max must be a number");
    let id_bits_min: u32 = parsed.get("id_bits_min").map(|s| s.parse().expect("id_bits_min must be a number")).unwrap_or(7);
    let id_bits_max: u32 = parsed.get("id_bits_max").map(|s| s.parse().expect("id_bits_max must be a number")).unwrap_or(11);

    let bytes = fs::read(save_file).unwrap_or_else(|e| {
        eprintln!("Error reading save file {}: {}", save_file, e);
        process::exit(1);
    });

    println!("--- Testing Variable Fixed (Total 24bits) ---");
    println!("File: {}", save_file);
    println!("Start bits: {}..={}", start_bit_min, start_bit_max);
    println!("ID bits: {}..={}", id_bits_min, id_bits_max);

    for id_bits in id_bits_min..=id_bits_max {
        let v_bits = 24 - id_bits;
        for start_bit in start_bit_min..=start_bit_max {
            let byte_offset = start_bit / 8;
            let bit_offset = start_bit % 8;
            
            if byte_offset as usize >= bytes.len() {
                continue;
            }

            let mut reader =
                BitReader::endian(Cursor::new(&bytes[byte_offset as usize..]), LittleEndian);
            for _ in 0..bit_offset {
                let _ = reader.read_bit().ok();
            }

            for _ in 0..10 {
                let _id = read_bits(&mut reader, id_bits);
                let val = read_bits(&mut reader, v_bits);
                if val == 14 || val == 310 || val == 31 {
                    println!(
                        "HIT! ID bits: {}, Val bits: {}, Start: {}, Value: {}",
                        id_bits, v_bits, start_bit, val
                    );
                }
            }
        }
    }
}
