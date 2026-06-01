use d2r_core::engine::checksum::Checksum;
use d2r_core::verify::args::{ArgError, ArgParser};
use std::env;
use std::fs;
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2item_rhythm_enforcer").description(
        "Surgically inserts or removes padding bytes in a save file to realign bitstream rhythms.",
    );

    parser.add_arg("input", "Path to the input save file (.d2s)");
    parser.add_arg("output", "Path to the output save file");
    parser
        .add_opt(
            "insert",
            "Insert N bytes of 0x00 at OFFSET. Format: OFFSET[:COUNT]",
        )
        .short('i')
        .long("insert");
    parser
        .add_opt("remove", "Remove N bytes at OFFSET. Format: OFFSET[:COUNT]")
        .short('r')
        .long("remove");
    parser
        .add_flag(
            "fix-checksum",
            "Recalculate and update the D2S checksum after surgery",
        )
        .short('c')
        .long("fix-checksum");
    parser
        .add_flag(
            "fix-size",
            "Update the file size field in the D2S header after surgery",
        )
        .short('s')
        .long("fix-size");

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

    let input_path = parsed.get("input").unwrap();
    let output_path = parsed.get("output").unwrap();

    let mut bytes = fs::read(input_path).expect("Failed to read input file");

    let mut ops = Vec::new();

    if let Some(inserts) = parsed.get_all("insert") {
        for s in inserts {
            let parts: Vec<&str> = s.split(':').collect();
            let offset = parts[0].parse::<usize>().expect("Invalid insert offset");
            let count = if parts.len() > 1 {
                parts[1].parse::<usize>().expect("Invalid insert count")
            } else {
                1
            };
            ops.push((offset, count, true));
        }
    }

    if let Some(removes) = parsed.get_all("remove") {
        for s in removes {
            let parts: Vec<&str> = s.split(':').collect();
            let offset = parts[0].parse::<usize>().expect("Invalid remove offset");
            let count = if parts.len() > 1 {
                parts[1].parse::<usize>().expect("Invalid remove count")
            } else {
                1
            };
            ops.push((offset, count, false));
        }
    }

    // Sort operations by offset descending to avoid index shifting issues
    ops.sort_by(|a, b| b.0.cmp(&a.0));

    for (offset, count, is_insert) in ops {
        if is_insert {
            if offset > bytes.len() {
                eprintln!(
                    "[WARN] Insert offset {} is beyond file size {}, skipping.",
                    offset,
                    bytes.len()
                );
                continue;
            }
            let padding = vec![0u8; count];
            bytes.splice(offset..offset, padding);
            println!("[SURGERY] Inserted {} bytes at offset {}", count, offset);
        } else {
            if offset + count > bytes.len() {
                eprintln!(
                    "[WARN] Remove range {}-{} is beyond file size {}, skipping.",
                    offset,
                    offset + count,
                    bytes.len()
                );
                continue;
            }
            bytes.drain(offset..offset + count);
            println!("[SURGERY] Removed {} bytes at offset {}", count, offset);
        }
    }

    if parsed.is_set("fix-size") {
        let new_size = bytes.len() as u32;
        if bytes.len() >= 12 {
            bytes[8..12].copy_from_slice(&new_size.to_le_bytes());
            println!("[HEADER] Updated file size field to {} bytes", new_size);
        }
    }

    if parsed.is_set("fix-checksum") {
        Checksum::fix(&mut bytes);
        if bytes.len() >= 16 {
            let new_checksum = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
            println!("[CHECKSUM] Checksum updated to 0x{:08X}", new_checksum);
        }
    }

    fs::write(output_path, bytes).expect("Failed to write output file");
    println!("Surgery complete. Output saved to {}", output_path);
}
