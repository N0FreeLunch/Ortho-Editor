use anyhow::{Context, Result};
use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::io::Cursor;
use std::process;

#[derive(Serialize, Clone)]
struct BveCandidate {
    width: u32,
    raw_value: u32,
    score: i32,
    reasons: Vec<String>,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_bve").description(
        "Explores bit-field value candidates at a given offset in a hex binary string",
    );

    parser.add_spec(ArgSpec::positional(
        "hex_input",
        "hex-encoded binary string (e.g., 0x55AA)",
    ));
    parser.add_spec(ArgSpec::positional(
        "bit_offset",
        "bit offset to start reading from",
    ));

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

    let hex_input = parsed.get("hex_input").unwrap();
    let bit_offset: u64 = parsed
        .get("bit_offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("Error: Invalid bit offset. Must be a non-negative integer.");
            process::exit(1);
        });
    let use_json = parsed.is_json();

    let hex_clean = hex_input.trim_start_matches("0x");
    let bytes = hex::decode(hex_clean).context("Invalid hex input")?;

    let mut candidates = Vec::new();

    for width in 1..=32 {
        if let Ok(candidate) = evaluate_width(&bytes, bit_offset, width) {
            candidates.push(candidate);
        }
    }

    // Sort by score descending, then by width ascending
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.width.cmp(&b.width)));

    if use_json {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
    } else {
        println!("BVE - Bit-Field Value Explorer");
        println!("Offset: {} bits", bit_offset);
        println!();
        println!(
            "+-------+------------+-------+----------------------------------------------------+"
        );
        println!(
            "| Width | Value      | Score | Reasons                                            |"
        );
        println!(
            "+-------+------------+-------+----------------------------------------------------+"
        );
        for cand in candidates.iter().take(10) {
            let reasons = cand.reasons.join(", ");
            println!(
                "| {:<5} | {:<10} | {:<5} | {:<50} |",
                cand.width, cand.raw_value, cand.score, reasons
            );
        }
        println!(
            "+-------+------------+-------+----------------------------------------------------+"
        );
    }

    Ok(())
}

fn evaluate_width(bytes: &[u8], offset: u64, width: u32) -> Result<BveCandidate> {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    reader.skip(offset as u32).context("Offset out of bounds")?;

    // Read bits one by one to support dynamic width without const generic issues
    let mut val: u32 = 0;
    for i in 0..width {
        let bit = reader.read_bit().context("Bit read failed")?;
        if bit {
            val |= 1 << i;
        }
    }

    let mut score = 0;
    let mut reasons = Vec::new();

    // 1. Common D2 Widths
    let common_widths = vec![7, 9, 10, 12, 21, 32];
    if common_widths.contains(&width) {
        score += 10;
        reasons.push(format!("Common width ({})", width));
    }

    // 2. Value 0 is very common for optional/padding fields
    if val == 0 {
        score += 5;
        reasons.push("Value is 0".to_string());
    }

    // 3. Power of 2 (Possible flag or enum)
    if val > 0 && (val & (val - 1)) == 0 {
        score += 5;
        reasons.push("Power of 2".to_string());
    }

    // 4. Plausible ID ranges
    if width >= 9 && val < 1024 {
        score += 3;
        reasons.push("Plausible ID range".to_string());
    }

    // 5. Plausible Stat range
    if width <= 10 && val < 256 {
        score += 3;
        reasons.push("Plausible small value range".to_string());
    }

    // 6. Typical property values (usually not huge)
    if width > 1 && val < (1 << (width - 1)) {
        score += 2;
        reasons.push("Value doesn't fill width".to_string());
    }

    Ok(BveCandidate {
        width,
        raw_value: val,
        score,
        reasons,
    })
}
