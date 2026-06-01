use bitstream_io::{BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2item_brute_len").description(
        "Scans for a 9-bit Terminator (511) in a range of bits from a base bit offset.",
    );
    parser.add_spec(ArgSpec::positional("file", "Path to save file"));
    parser.add_spec(ArgSpec::positional("base_bit", "Starting bit offset"));
    parser.add_spec(
        ArgSpec::positional("min_len", "Minimum length to scan (default: 50)")
            .optional()
            .with_default("50"),
    );
    parser.add_spec(
        ArgSpec::positional("max_len", "Maximum length to scan (default: 300)")
            .optional()
            .with_default("300"),
    );

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
    let base_bit: usize = parsed
        .get("base_bit")
        .unwrap()
        .parse()
        .expect("base_bit must be a number");
    let min_len: usize = parsed
        .get("min_len")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let max_len: usize = parsed
        .get("max_len")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let bytes = fs::read(path).expect("failed to read save file");

    println!(
        "Scanning 9-bit Terminator (511) starting from {}...",
        base_bit
    );
    println!("Range: {} to {} bits from base", min_len, max_len);

    let mut found = 0;
    for len in min_len..=max_len {
        let target = base_bit + len;
        if check(target, &bytes) {
            println!(
                "  [MATCH] Found 511 at Bit Offset {} (Length: {} bits from base)",
                target, len
            );
            found += 1;
        }
    }

    if found == 0 {
        println!("No terminator found in the specified range.");
    } else {
        println!("Scan complete. Found {} potential terminators.", found);
    }
}

fn check(start: usize, bytes: &[u8]) -> bool {
    let reader = IoBitReader::endian(Cursor::new(bytes), LittleEndian);
    let mut recorder = BitCursor::new(reader);
    if recorder.skip_and_record(start as u32).is_err() {
        return false;
    }

    match recorder.read_bits::<u32>(9) {
        Ok(511) => true,
        _ => false,
    }
}
