use d2r_core::algo::alignment::{AlignmentResult, BitAligner};
use d2r_core::save::{gf_payload_range, map_core_sections};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io;
use std::process;

fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        for i in 0..8 {
            bits.push((byte >> i) & 1 != 0);
        }
    }
    bits
}

fn main() -> io::Result<()> {
    let mut parser = ArgParser::new("d2gf_bit_diff")
        .description("Performs bit-level alignment and diffing of character status sections between two D2R save files");

    parser.add_spec(ArgSpec::positional(
        "file_a",
        "path to the first save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "file_b",
        "path to the second save file (.d2s)",
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

    let path1 = parsed.get("file_a").unwrap();
    let path2 = parsed.get("file_b").unwrap();

    let bytes1 = fs::read(path1)?;
    let bytes2 = fs::read(path2)?;

    let map1 =
        map_core_sections(&bytes1).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let map2 =
        map_core_sections(&bytes2).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let range1 = gf_payload_range(&map1);
    let range2 = gf_payload_range(&map2);

    let payload1 = &bytes1[range1.start..range1.end];
    let payload2 = &bytes2[range2.start..range2.end];

    let bits1 = bytes_to_bits(payload1);
    let bits2 = bytes_to_bits(payload2);

    println!("=== GF Bit Diff ===");
    println!("File 1: {} ({} bits)", path1, bits1.len());
    println!("File 2: {} ({} bits)", path2, bits2.len());
    println!();

    // Standard scoring for bit-level alignment
    let aligner = BitAligner::new(2, -1, -3, -1);
    let result = aligner.align(&bits1, &bits2);

    println!("Alignment Score : {}", result.score);
    println!("Similarity      : {:.2}%", result.similarity_pct());
    println!("Gap Count       : {}", result.gap_indices.len());
    println!();

    println!("--- Aligned View (First 500 bits) ---");
    let preview_len = 500.min(result.actual_aligned.len());
    let sub_actual: Vec<Option<bool>> = result
        .actual_aligned
        .iter()
        .take(preview_len)
        .cloned()
        .collect();
    let sub_expected: Vec<Option<bool>> = result
        .expected_aligned
        .iter()
        .take(preview_len)
        .cloned()
        .collect();

    let sub_result = AlignmentResult {
        score: 0,
        actual_aligned: sub_actual,
        expected_aligned: sub_expected,
        gap_indices: Vec::new(),
    };

    println!("{}", sub_result.pretty_print());

    if result.actual_aligned.len() > 500 {
        println!("... (truncated)");
    }

    if !result.gap_indices.is_empty() {
        println!("\n--- Gap Indices ---");
        for &idx in result.gap_indices.iter().take(10) {
            println!("Gap at bit index {}", idx);
        }
        if result.gap_indices.len() > 10 {
            println!("... and {} more gaps", result.gap_indices.len() - 10);
        }
    }

    Ok(())
}
