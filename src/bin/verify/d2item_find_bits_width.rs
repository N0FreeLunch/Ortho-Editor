use bitstream_io::{BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let mut parser = ArgParser::new("d2item_find_bits_width")
        .description("Probes bit widths (1 to 32) at a given bit offset in a save file.");
    parser.add_spec(ArgSpec::positional("file", "Path to save file"));
    parser.add_spec(ArgSpec::positional("bit_offset", "Starting bit offset"));
    parser.add_spec(
        ArgSpec::positional("count", "Number of bits to probe (default: 10)")
            .optional()
            .with_default("10"),
    );

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

    let file_path = parsed.get("file").unwrap();
    let offset = parsed
        .get("bit_offset")
        .unwrap()
        .parse::<u64>()
        .expect("invalid offset");
    let _count = parsed
        .get("count")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    let bytes = fs::read(file_path).expect("failed to read save file");

    let mut reader =
        IoBitReader::endian(Cursor::new(&bytes[(offset / 8) as usize..]), LittleEndian);
    let mut recorder = BitCursor::new(&mut reader);
    let _ = recorder.skip_and_record((offset % 8) as u32);

    println!("Probing bit widths at offset {}:", offset);
    for width in 1..=32 {
        let checkpoint = recorder.checkpoint();
        let val: u32 = recorder.read_bits::<u32>(width as u32).unwrap_or(0);
        println!("  Width {:2}: {} (0x{:X})", width, val, val);
        recorder.rollback(checkpoint);
    }
}
