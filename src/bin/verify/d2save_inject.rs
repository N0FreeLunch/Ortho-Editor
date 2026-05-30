use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use d2r_core::engine::checksum::Checksum;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;

/// Reads the entire .d2i file as raw bytes and returns a bit vector.
/// Item::from_reader stops at 70 bits, but the actual in-game scroll uses 72 bits,
/// so raw bit loading is required for the DLC editor parser to work correctly.
fn load_item_bits_from_d2i(d2i_path: &str) -> Vec<bool> {
    let bytes = fs::read(d2i_path).unwrap_or_else(|e| {
        eprintln!("[ERROR] Cannot read '{}': {}", d2i_path, e);
        process::exit(1);
    });

    let mut bits = Vec::new();
    for &b in &bytes {
        for i in 0..8 {
            bits.push((b >> i) & 1 == 1);
        }
    }
    println!("  Loaded {} bits (raw) from '{}'", bits.len(), d2i_path);
    bits
}

fn main() {
    let mut parser = ArgParser::new("d2save_inject").description(
        "Injects multiple copies of an item template (.d2i) into a D2R save file (.d2s)",
    );

    parser.add_spec(ArgSpec::positional(
        "input_d2s",
        "path to the source D2R save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "item_d2i",
        "path to the item template file (.d2i)",
    ));
    parser.add_spec(ArgSpec::positional("count", "number of copies to inject"));
    parser.add_spec(ArgSpec::positional(
        "output_d2s",
        "path to the output save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::flag(
        "no-align",
        Some('n'),
        Some("no-align"),
        "disable byte-alignment per item (bit-packed mode)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}\n\n{}", e, parser.usage());
            process::exit(1);
        }
    };

    let input_path = parsed.get("input_d2s").unwrap();
    let d2i_path = parsed.get("item_d2i").unwrap();
    let count: usize = parsed
        .get("count")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("[ERROR] count must be a positive integer");
            process::exit(1);
        });
    let output_path = parsed.get("output_d2s").unwrap();
    let no_align = parsed.is_set("no-align");

    println!("=== d2save_inject ===");
    println!("  Input:  {}", input_path);
    println!("  Item:   {}", d2i_path);
    println!("  Count:  {}", count);
    println!("  Output: {}", output_path);
    if no_align {
        println!("  Mode:   Bit-packed (no alignment)");
    } else {
        println!("  Mode:   Byte-aligned (per item)");
    }
    println!();

    // Load the save file
    let bytes = fs::read(input_path).unwrap_or_else(|e| {
        eprintln!("[ERROR] Cannot read '{}': {}", input_path, e);
        process::exit(1);
    });

    // Find JM1 (player items) and JM2 (corpse items)
    let mut jm_positions: Vec<usize> = Vec::new();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'J' && bytes[i + 1] == b'M' {
            jm_positions.push(i);
            if jm_positions.len() == 2 {
                break;
            }
        }
    }

    if jm_positions.len() < 2 {
        eprintln!("[ERROR] Could not find two JM markers in '{}'", input_path);
        process::exit(1);
    }

    let jm1 = jm_positions[0];
    let jm2 = jm_positions[1];
    let original_count = u16::from_le_bytes([bytes[jm1 + 2], bytes[jm1 + 3]]);

    println!("  JM1 at byte {} (bit {})", jm1, jm1 * 8);
    println!("  JM2 at byte {} (bit {})", jm2, jm2 * 8);
    println!("  Original item count: {}", original_count);

    // Load item bits from .d2i
    let item_bits = load_item_bits_from_d2i(d2i_path);

    // Original item payload (between JM1+4 header and JM2)
    let original_item_bytes = &bytes[jm1 + 4..jm2];

    // Assemble the new file
    let mut writer = BitWriter::endian(Vec::new(), LittleEndian);

    // Write header up to JM1
    for &b in &bytes[..jm1] {
        writer.write::<8, u8>(b).unwrap();
    }

    // Write JM1 marker + new count
    writer.write::<8, u8>(b'J').unwrap();
    writer.write::<8, u8>(b'M').unwrap();
    writer
        .write::<16, u16>(original_count + count as u16)
        .unwrap();

    // Write original items as-is (bit stream)
    let mut item_reader = BitReader::endian(Cursor::new(original_item_bytes), LittleEndian);
    while let Ok(bit) = item_reader.read_bit() {
        writer.write_bit(bit).unwrap();
    }

    // Byte-align before writing new items
    if !no_align {
        writer.byte_align().unwrap();
    }

    // Inject N copies of the item
    for _ in 0..count {
        for &bit in &item_bits {
            writer.write_bit(bit).unwrap();
        }
        if !no_align {
            writer.byte_align().unwrap();
        }
    }

    // Final byte-align
    if !no_align {
        writer.byte_align().unwrap();
    }

    // Write JM2 and remainder verbatim (Footer: Corpse, Merc, Golem, etc.)
    for &b in &bytes[jm2..] {
        writer.write::<8, u8>(b).unwrap();
    }

    let mut result_bytes = writer.into_writer();

    // Fix file size in header (offset 8)
    let file_size = result_bytes.len() as u32;
    result_bytes[8..12].copy_from_slice(&file_size.to_le_bytes());

    // Fix checksum
    Checksum::fix(&mut result_bytes);

    // Write output
    fs::create_dir_all(
        std::path::Path::new(output_path)
            .parent()
            .unwrap_or(std::path::Path::new(".")),
    )
    .ok();
    fs::write(output_path, &result_bytes).unwrap_or_else(|e| {
        eprintln!("[ERROR] Cannot write '{}': {}", output_path, e);
        process::exit(1);
    });

    println!();
    println!(
        "[OK] Injected {} item(s). Output: {} ({} bytes)",
        count,
        output_path,
        result_bytes.len()
    );
}
