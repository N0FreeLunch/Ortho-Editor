use anyhow::{Result, anyhow, Context};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus, ReportIssue};
use serde::Serialize;
use std::env;
use std::fs;
use std::process;

#[derive(Serialize)]
struct DsaReport {
    file_a: String,
    file_b: String,
    allowed_bits: Vec<usize>,
    identical: bool,
    violations: Vec<BitViolation>,
}

#[derive(Serialize, Clone)]
struct BitViolation {
    abs_bit: usize,
    byte_offset: usize,
    bit_in_byte: usize,
    val_a: u8,
    val_b: u8,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2save_dsa")
        .description("Domain Symmetry Auditor: Validates bit-level symmetry between two save files with allowed drift rules.");

    parser.add_spec(ArgSpec::positional(
        "file_a",
        "path to the first save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "file_b",
        "path to the second save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::option(
        "allowed-bits",
        None,
        Some("allowed-bits"),
        "comma-separated list of allowed bit offsets (e.g. 81,96,108)",
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

    let mut om = OutputManager::new("d2save_dsa", &parsed);
    let path_a = parsed.get("file_a").unwrap();
    let path_b = parsed.get("file_b").unwrap();
    let allowed_bits_str = parsed.get("allowed-bits").map(|s| s.as_str()).unwrap_or("");

    let mut allowed_bits: Vec<usize> = Vec::new();
    if !allowed_bits_str.is_empty() {
        for part in allowed_bits_str.split(',') {
            let part = part.trim();
            if part.contains('-') {
                let bounds: Vec<&str> = part.split('-').collect();
                if bounds.len() != 2 {
                    return Err(anyhow!("Invalid range format: '{}'. Expected 'start-end'.", part));
                }
                let start_str = bounds[0].trim();
                let end_str = bounds[1].trim();
                let start = start_str
                    .parse::<usize>()
                    .map_err(|_| anyhow!("Invalid bit offset: '{}' in range '{}'.", start_str, part))?;
                let end = end_str
                    .parse::<usize>()
                    .map_err(|_| anyhow!("Invalid bit offset: '{}' in range '{}'.", end_str, part))?;

                if start > end {
                    return Err(anyhow!("Reverse range is not allowed: '{}' ({} > {}).", part, start, end));
                }
                for bit in start..=end {
                    allowed_bits.push(bit);
                }
            } else {
                let bit = part
                    .parse::<usize>()
                    .map_err(|_| anyhow!("Invalid bit offset: '{}'.", part))?;
                allowed_bits.push(bit);
            }
        }
        allowed_bits.sort_unstable();
        allowed_bits.dedup();
    }

    let bytes_a = fs::read(path_a).with_context(|| format!("Cannot read '{}'", path_a))?;
    let bytes_b = fs::read(path_b).with_context(|| format!("Cannot read '{}'", path_b))?;

    if bytes_a.len() != bytes_b.len() {
        let msg = format!("Length mismatch: A={} bytes, B={} bytes", bytes_a.len(), bytes_b.len());
        if om.is_json() {
            let report = Report::<()>::new(
                ReportMetadata::new("d2save_dsa", path_a, env!("CARGO_PKG_VERSION")),
                ReportStatus::Fail,
            )
            .with_issues(vec![ReportIssue {
                kind: "LengthMismatch".to_string(),
                message: msg,
                bit_offset: None,
            }])
            .with_forensic_context();
            om.json(&serde_json::to_string_pretty(&report)?);
            process::exit(1);
        } else {
            return Err(anyhow!("{}", msg));
        }
    }

    let mut violations = Vec::new();
    for (i, (&a, &e)) in bytes_a.iter().zip(bytes_b.iter()).enumerate() {
        if a != e {
            let diff = a ^ e;
            for bit in 0..8 {
                if (diff >> bit) & 1 == 1 {
                    let bit_offset = i * 8 + bit;
                    if !allowed_bits.contains(&bit_offset) {
                        violations.push(BitViolation {
                            abs_bit: bit_offset,
                            byte_offset: i,
                            bit_in_byte: bit,
                            val_a: (a >> bit) & 1,
                            val_b: (e >> bit) & 1,
                        });
                    }
                }
            }
        }
    }

    if om.is_json() {
        let status = if violations.is_empty() { ReportStatus::Ok } else { ReportStatus::Fail };
        let mut report = Report::new(
            ReportMetadata::new("d2save_dsa", path_a, env!("CARGO_PKG_VERSION")),
            status,
        )
        .with_results(DsaReport {
            file_a: path_a.clone(),
            file_b: path_b.clone(),
            allowed_bits,
            identical: violations.is_empty(),
            violations: violations.clone(),
        })
        .with_forensic_context();
        
        if !violations.is_empty() {
            report = report.with_issues(vec![ReportIssue {
                kind: "BitViolation".to_string(),
                message: format!("{} unauthorized bit violations found.", violations.len()),
                bit_offset: Some(violations[0].abs_bit as u64),
            }]);
        }
        
        om.json(&serde_json::to_string_pretty(&report)?);
    } else {
        om.println("=== Domain Symmetry Auditor (DSA) ===");
        om.println(&format!("  A: {}", path_a));
        om.println(&format!("  B: {}", path_b));
        om.println(&format!("  Allowed bits: {:?}", allowed_bits));

        if violations.is_empty() {
            om.summary("Bitwise symmetry verified.");
        } else {
            om.println(&format!(
                "\n[FAILURE] {} unauthorized bit violations found:",
                violations.len()
            ));
            for v in violations.iter().take(20) {
                om.println(&format!(
                    "  Bit {:>6} (Byte {:>5}, Bit {}): A={} B={}",
                    v.abs_bit, v.byte_offset, v.bit_in_byte, v.val_a, v.val_b
                ));
            }
            if violations.len() > 20 {
                om.println(&format!("  ... and {} more violations", violations.len() - 20));
            }
            om.summary(&format!("{} violations found.", violations.len()));
            process::exit(1);
        }
    }
    
    Ok(())
}
