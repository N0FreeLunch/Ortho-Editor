use std::fs;
use anyhow::{bail, Context};
use d2r_core::verify::args::{ArgParser, ArgSpec};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_poison");
    parser.add_spec(ArgSpec::option("input", Some('i'), Some("input"), "Input save file (.d2s)").required());
    parser.add_spec(ArgSpec::option("bit-offset", Some('b'), Some("bit-offset"), "Bit offset to flip (absolute)").required());
    parser.add_spec(ArgSpec::option("output", Some('o'), Some("output"), "Output save file (.d2s)").required());

    let parsed = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(d2r_core::verify::args::ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(d2r_core::verify::args::ArgError::Error(e)) => {
            bail!("error: {}\n\n{}", e, parser.usage());
        }
    };

    let input_path = parsed.get("input").unwrap();
    let output_path = parsed.get("output").unwrap();
    let bit_offset_raw = parsed.get("bit-offset").unwrap();
    let bit_offset: u64 = bit_offset_raw.parse().context("Invalid bit-offset format")?;

    let mut bytes = fs::read(input_path).context("Failed to read input file")?;
    let total_bits = (bytes.len() as u64) * 8;

    if bit_offset >= total_bits {
        bail!("Bit offset {} is out of bounds (0..{})", bit_offset, total_bits);
    }

    let byte_idx = (bit_offset / 8) as usize;
    let bit_idx = (bit_offset % 8) as u8;

    let old_byte = bytes[byte_idx];
    // LSB-first: bit 0 is 0x01
    let mask = 1u8 << bit_idx;
    let new_byte = old_byte ^ mask;
    bytes[byte_idx] = new_byte;

    // Ensure output directory exists if it's a nested path
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).context("Failed to create output directory")?;
        }
    }

    fs::write(output_path, &bytes).context("Failed to write output file")?;

    println!("Mutation Summary:");
    println!("  Input:      {}", input_path);
    println!("  Output:     {}", output_path);
    println!("  BitOffset:  {}", bit_offset);
    println!("  ByteIndex:  {}", byte_idx);
    println!("  BitIndex:   {}", bit_idx);
    println!("  ByteChange: 0x{:02X} -> 0x{:02X}", old_byte, new_byte);
    println!("Successfully flipped 1 bit.");

    Ok(())
}
