use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{Cursor, Write};
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2item_bit_dump")
        .description("Dumps raw bits from a save file starting at an absolute bit offset with grouping and JSON support.");

    parser
        .add_opt("file", "Path to save file")
        .long("file")
        .required();
    parser
        .add_opt("offset", "Absolute bit offset")
        .long("offset")
        .required();
    parser
        .add_opt("len", "Number of bits to dump")
        .long("len")
        .required();
    parser
        .add_opt("group", "Bits per group (default: 8)")
        .long("group")
        .with_default("8");
    parser
        .add_opt("out", "Output file path (required if len > 2048)")
        .long("out");
    // --json is automatically handled by ArgParser, but we should make sure we use it if present.

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            process::exit(1);
        }
    };

    let path = parsed.get("file").unwrap();
    let offset: usize = parsed
        .get("offset")
        .unwrap()
        .parse()
        .expect("offset must be a number");
    let len: usize = parsed
        .get("len")
        .unwrap()
        .parse()
        .expect("len must be a number");
    let group: usize = parsed
        .get("group")
        .unwrap()
        .parse()
        .expect("group must be a number");
    let out_path = parsed.get("out");
    let is_json = parsed.is_json();

    if len > 2048 && out_path.is_none() && !is_json {
        eprintln!(
            "error: bit dump length ({}) exceeds terminal safe limit (2048).",
            len
        );
        eprintln!("Please provide an output file via --out <path> or use --json.");
        process::exit(1);
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: failed to read file {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);

    if reader.skip(offset as u32).is_err() {
        eprintln!(
            "error: cannot skip to bit {} (outside file boundaries).",
            offset
        );
        process::exit(1);
    }

    let mut bits = String::with_capacity(len);
    for _ in 0..len {
        match reader.read_bit() {
            Ok(bit) => bits.push(if bit { '1' } else { '0' }),
            Err(_) => break,
        }
    }

    if is_json {
        let output = json!({
            "offset": offset,
            "len": bits.len(),
            "bits": bits
        });
        if let Some(p) = out_path {
            fs::write(p, output.to_string()).expect("failed to write json to file");
        } else {
            println!("{}", output);
        }
    } else {
        let mut formatted = String::new();
        for (i, c) in bits.chars().enumerate() {
            if i > 0 && i % group == 0 {
                formatted.push(' ');
            }
            formatted.push(c);
        }

        let header = format!(
            "Bit Dump: file={}, offset={}, len={}, group={}",
            path,
            offset,
            bits.len(),
            group
        );

        if let Some(p) = out_path {
            let mut f = fs::File::create(p).expect("failed to create output file");
            writeln!(f, "{}", header).unwrap();
            writeln!(f, "{}", formatted).unwrap();
            println!("Dump written to {}", p);
        } else {
            println!("{}", header);
            println!("{}", formatted);
        }
    }
}
