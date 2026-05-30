use anyhow::{Context, Result};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
struct Mismatch {
    bit_offset: usize,
    original: u8,
    actual: u8,
}

#[derive(Debug, Serialize)]
struct AuditReport {
    is_match: bool,
    mismatch_count: usize,
    noise_count: usize,
    mismatches: Vec<Mismatch>,
    ignored_mismatches: Vec<Mismatch>,
    error: Option<String>,
}

fn is_justified(bit_offset: usize) -> bool {
    // D2S Header Checksum: bits 96 to 127 (bytes 12-15)
    (96..=127).contains(&bit_offset)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let json_mode = args.iter().any(|arg| arg == "--json");

    // Filter out flags to get positional args
    let positional_args: Vec<&String> = args[1..]
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .collect();

    if positional_args.len() < 2 {
        if json_mode {
            let report = AuditReport {
                is_match: false,
                mismatch_count: 0,
                noise_count: 0,
                mismatches: vec![],
                ignored_mismatches: vec![],
                error: Some(
                    "Usage: d2save_baseline_audit <original.d2s> <reconstructed.d2s> [--json]"
                        .to_string(),
                ),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Usage: d2save_baseline_audit <original.d2s> <reconstructed.d2s> [--json]");
        }
        std::process::exit(1);
    }

    let original_path = Path::new(positional_args[0]);
    let reconstructed_path = Path::new(positional_args[1]);

    let original_bytes = fs::read(original_path)
        .with_context(|| format!("Failed to read original file: {:?}", original_path))?;
    let reconstructed_bytes = fs::read(reconstructed_path).with_context(|| {
        format!(
            "Failed to read reconstructed file: {:?}",
            reconstructed_path
        )
    })?;

    let mut mismatches = Vec::new();
    let mut ignored_mismatches = Vec::new();
    let mut mismatch_count = 0;
    let mut noise_count = 0;

    if original_bytes.len() != reconstructed_bytes.len() {
        if json_mode {
            let report = AuditReport {
                is_match: false,
                mismatch_count: 1, // Treat size mismatch as a major error
                noise_count: 0,
                mismatches: vec![],
                ignored_mismatches: vec![],
                error: Some(format!(
                    "File size mismatch: original={} bytes, reconstructed={} bytes",
                    original_bytes.len(),
                    reconstructed_bytes.len()
                )),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("FAIL: File size mismatch.");
            println!("  Original:      {} bytes", original_bytes.len());
            println!("  Reconstructed: {} bytes", reconstructed_bytes.len());
        }
        std::process::exit(1);
    }

    for (i, (&orig, &recon)) in original_bytes
        .iter()
        .zip(reconstructed_bytes.iter())
        .enumerate()
    {
        if orig != recon {
            let xor = orig ^ recon;
            for bit in 0..8 {
                if (xor >> bit) & 1 != 0 {
                    let bit_offset = i * 8 + bit;
                    let m = Mismatch {
                        bit_offset,
                        original: (orig >> bit) & 1,
                        actual: (recon >> bit) & 1,
                    };

                    if is_justified(bit_offset) {
                        noise_count += 1;
                        if ignored_mismatches.len() < 10 {
                            ignored_mismatches.push(m);
                        }
                    } else {
                        mismatch_count += 1;
                        if mismatches.len() < 10 {
                            mismatches.push(m);
                        }
                    }
                }
            }
        }
    }

    let is_match = mismatch_count == 0;

    if json_mode {
        let report = AuditReport {
            is_match,
            mismatch_count,
            noise_count,
            mismatches,
            ignored_mismatches,
            error: None,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if is_match {
            if noise_count > 0 {
                println!("[OK] Baseline integrity verified (with justified noise).");
                println!("  Justified Noise Count: {}", noise_count);
            } else {
                println!("[OK] Baseline integrity verified. Files are bit-identical.");
            }
        } else {
            println!("[FAIL] Bitwise divergence detected!");
            println!("Total unjustified mismatches: {}", mismatch_count);
            println!("Total justified noise: {}", noise_count);
            println!("First 10 unjustified mismatches:");
            for m in &mismatches {
                println!(
                    "  Bit Offset {:<8} | Original: {} | Actual: {}",
                    m.bit_offset, m.original, m.actual
                );
            }
            println!(
                "\n[CRITICAL] Semantic Shift suspected (reconstruction diverged from baseline)."
            );
        }
    }

    if !is_match {
        std::process::exit(1);
    }

    Ok(())
}
