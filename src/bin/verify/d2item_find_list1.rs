use bitstream_io::{BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let mut parser = ArgParser::new("d2item_find_list1").description(
        "Scans for List 1 properties (9-bit IDs) after a header/code at a given bit offset.",
    );
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::positional(
        "item_start_bit",
        "Starting bit offset of the item section",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("save_file").unwrap();
    let start_bit: u64 = parsed
        .get("item_start_bit")
        .unwrap()
        .parse()
        .expect("invalid start bit");

    let bytes = fs::read(path).expect("failed to read save file");

    let mut reader = IoBitReader::endian(
        Cursor::new(&bytes[(start_bit / 8) as usize..]),
        LittleEndian,
    );
    let mut recorder = BitCursor::new(&mut reader);
    let _ = recorder.skip_and_record((start_bit % 8) as u32);

    println!(
        "Scanning for List 1 properties (9-bit IDs) after header/code at {}:",
        start_bit
    );
    // Assume header + code is roughly 100-150 bits
    for skip in (80..200).step_by(1) {
        let checkpoint = recorder.checkpoint();
        let _ = recorder.skip_and_record(skip as u32);

        let id: u32 = recorder.read_bits::<u32>(9).unwrap_or(0);
        if id < 511 && id > 0 {
            println!(
                "  [Skip {}] Potential Stat ID: {} at bit {}",
                skip,
                id,
                start_bit + skip as u64
            );
        }
        recorder.rollback(checkpoint);
    }
}
