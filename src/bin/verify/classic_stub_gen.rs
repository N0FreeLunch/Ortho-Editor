use anyhow::Context;
use d2r_core::domain::header::axiom::EXPANSION_FLAG_OFFSET;
use d2r_core::engine::checksum::Checksum;
use d2r_core::verify::args::{ArgError, ArgParser};
use std::{env, fs};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("classic_stub_gen");
    parser
        .add_opt("input", "Input D2S save file (Expansion)")
        .short('i')
        .long("input")
        .required();
    parser
        .add_opt("output", "Output D2S save file (Classic Mock)")
        .short('o')
        .long("output")
        .required();

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            eprintln!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let input_path = parsed.get("input").context("Input path is required")?;
    let output_path = parsed.get("output").context("Output path is required")?;

    let mut bytes = fs::read(input_path)
        .with_context(|| format!("Failed to read input file: {}", input_path))?;

    let original_len = bytes.len();

    // 1. Clear expansion flag (offset 271, bit 0x20)
    if bytes.len() > EXPANSION_FLAG_OFFSET {
        let old_byte = bytes[EXPANSION_FLAG_OFFSET];
        println!("[ClassicStub] Offset {}: 0x{:02X} (binary: {:08b})", EXPANSION_FLAG_OFFSET, old_byte, old_byte);
        if (old_byte & 0x20) != 0 {
            println!("[ClassicStub] Clearing expansion flag (0x20)...");
            bytes[EXPANSION_FLAG_OFFSET] &= !0x20;
        } else {
            println!("[ClassicStub] Expansion flag (0x20) is already clear.");
        }
    } else {
        anyhow::bail!("Input file is too small to contain expansion flag offset ({})", EXPANSION_FLAG_OFFSET);
    }

    // 2. Recompute Checksum
    println!("[ClassicStub] Recomputing checksum...");
    Checksum::fix(&mut bytes);

    // 3. Same-budget check
    if bytes.len() != original_len {
        anyhow::bail!("Same-budget contract violated: output length ({}) != input length ({})", bytes.len(), original_len);
    }

    // Ensure parent directory exists for output
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output_path, &bytes)
        .with_context(|| format!("Failed to write output file: {}", output_path))?;

    println!("[ClassicStub] Successfully generated classic mock: {}", output_path);

    Ok(())
}
