use anyhow::{Context, Result};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut parser = ArgParser::new("v105_bit_arch");
    parser.add_spec(
        ArgSpec::option("file", Some('f'), Some("file"), "Path to the .d2s file").required(),
    );
    parser.add_spec(
        ArgSpec::option(
            "start",
            Some('s'),
            Some("start"),
            "Start byte offset (default 835 for gf)",
        )
        .with_default("835"),
    );
    parser.add_spec(
        ArgSpec::option(
            "offset",
            Some('o'),
            Some("offset"),
            "Bit offset from start byte",
        )
        .with_default("0"),
    );
    parser.add_spec(
        ArgSpec::option("len", Some('l'), Some("len"), "Number of bits to dump")
            .with_default("100"),
    );

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
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

    let file_path = parsed.get("file").cloned().unwrap();
    let start_byte: usize = parsed
        .get("start")
        .unwrap()
        .parse()
        .context("Invalid start byte")?;
    let bit_offset: usize = parsed
        .get("offset")
        .unwrap()
        .parse()
        .context("Invalid bit offset")?;
    let length: usize = parsed
        .get("len")
        .unwrap()
        .parse()
        .context("Invalid length")?;

    let bytes =
        fs::read(&file_path).with_context(|| format!("Failed to read file: {}", file_path))?;

    let bitstream = get_bits(&bytes, start_byte, bit_offset, length);

    println!("Bits @ {} (File: {}):", bit_offset, file_path);
    println!("{}", bitstream);

    Ok(())
}

fn get_bits(bytes: &[u8], start_byte: usize, bit_offset: usize, length: usize) -> String {
    let mut bits = String::with_capacity(length);
    for i in 0..length {
        let total_bit = (start_byte * 8) + bit_offset + i;
        let byte_idx = total_bit / 8;
        let bit_on_byte = total_bit % 8;

        if byte_idx < bytes.len() {
            if (bytes[byte_idx] & (1 << bit_on_byte)) != 0 {
                bits.push('1');
            } else {
                bits.push('0');
            }
        } else {
            break;
        }
    }
    bits
}
