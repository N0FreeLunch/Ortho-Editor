// d2r-core/src/bin/verify/d2save_bit_visualizer.rs

use anyhow::Context;
use d2r_core::verify::args::{ArgError, ArgParser};
use std::{env, fs, io::Write};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_bit_visualizer");
    parser.add_opt("start", "Start bit offset").short('s').long("start");
    parser.add_opt("width", "Bit width to visualize").short('w').long("width");
    parser.add_opt("output", "Output file path (mandatory for >2048 bits)").short('o').long("output");
    parser.add_arg("file", "Save file to visualize");

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let file_path = parsed.get("file").context("Missing save file argument")?;
    let start_bit: usize = parsed.get("start").and_then(|s| s.parse().ok()).unwrap_or(0);
    let width: usize = parsed.get("width").and_then(|w| w.parse().ok()).unwrap_or(256);
    let output_path = parsed.get("output");

    if width > 2048 && output_path.is_none() {
        anyhow::bail!("OOM Prevention: Width > 2048 bits requires --output <file_path> to prevent terminal overflow.");
    }

    let bytes = fs::read(file_path).context("Failed to read save file")?;
    
    // Simple bit visualization (Scaffold for Slice 1)
    let mut result = String::new();
    result.push_str(&format!("Visualizing bits from {} to {} (width: {})\n", start_bit, start_bit + width, width));
    
    for i in 0..width {
        let bit_idx = start_bit + i;
        let byte_idx = bit_idx / 8;
        let bit_in_byte = bit_idx % 8;
        
        if byte_idx >= bytes.len() {
            result.push_str("X"); // Out of bounds
            continue;
        }
        
        let bit = (bytes[byte_idx] >> bit_in_byte) & 1 == 1;
        
        // ANSI colors (Simple for now)
        if bit {
            result.push_str("\x1b[32m1\x1b[0m"); // Green for 1
        } else {
            result.push_str("\x1b[34m0\x1b[0m"); // Blue for 0
        }
        
        if (i + 1) % 8 == 0 { result.push(' '); }
        if (i + 1) % 64 == 0 { result.push('\n'); }
    }
    result.push('\n');

    if let Some(out) = output_path {
        let mut f = fs::File::create(out)?;
        f.write_all(result.as_bytes())?;
        println!("Visualization written to {}", out);
    } else {
        println!("{}", result);
    }

    Ok(())
}
