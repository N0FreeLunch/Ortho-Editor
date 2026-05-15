use anyhow::Context;
use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::save_integrity::verify_save_integrity;
use std::{env, fs, io::Cursor, process};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_verify");
    parser.add_opt("dump-bits", "Dump raw bits from start <bit> and count <bits>")
        .long("dump-bits")
        .value_count(2);
    parser.add_flag("fix", "Automatically fix checksums if mismatch is detected")
        .short('f')
        .long("fix");
    parser.add_arg("files", "Save files to verify")
        .repeated();

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            eprintln!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let files = parsed.get_vec("files").cloned().unwrap_or_default();
    let fix_mode = parsed.is_set("fix");
    let mut om = d2r_core::verify::OutputManager::new("d2save_verify", &parsed);
    let is_json = om.is_json();

    if let Some(bits_args) = parsed.get_vec("dump-bits") {
        if files.is_empty() {
            anyhow::bail!("Error: No file provided for --dump-bits");
        }
        let start_bit: u64 = bits_args[0].parse().unwrap_or(0);
        let count: u64 = bits_args[1].parse().unwrap_or(0);
        dump_bits(&mut om, &files[0], start_bit, count)?;
        return Ok(());
    }

    if files.is_empty() {
        anyhow::bail!("{}", parser.usage());
    }

    let mut all_ok = true;
    for path in &files {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                if is_json {
                    let report = d2r_core::verify::Report::<serde_json::Value>::new(
                        d2r_core::verify::ReportMetadata::new("d2save_verify", path, "unknown"),
                        d2r_core::verify::ReportStatus::Fail,
                    )
                    .with_forensic_context()
                    .with_issues(vec![d2r_core::verify::ReportIssue {
                        kind: "io".to_string(),
                        message: format!("Cannot read file: {}", e),
                        bit_offset: None,
                    }]);
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    om.println(&format!("=== {} ===\n  [ERROR] Cannot read file: {}", path, e));
                }
                all_ok = false;
                continue;
            }
        };

        let (report, failed) = verify_save_integrity(path, &bytes);

        if fix_mode && failed {
            let mut bytes_to_fix = bytes.clone();
            d2r_core::engine::checksum::Checksum::fix(&mut bytes_to_fix);
            fs::write(path, &bytes_to_fix).context(format!("Failed to write fixed {}", path))?;
            om.summary(&format!("  [FIXED] Checksum updated for {}", path));
        }

        if is_json {
            let report_json = report.with_forensic_context();
            om.json(&serde_json::to_string(&report_json)?);
        } else {
            om.println(&format!("=== {} ===", path));
            om.println(&format!("  status: {:?}", report.status));
            if let Some(results) = report.scan_results.as_ref() {
                om.println(&format!(
                    "  version=0x{:04X} alpha={} size={} checksum={}",
                    results.header_version, results.alpha_mode, results.file_size_actual, results.checksum_stored
                ));
                
                om.println(&format!("  Fidelity Score: {:.1}% ({:?})", results.fidelity_score * 100.0, results.forensic_audit.combined_confidence));
                
                if let Ok(prog_res) = d2r_core::domain::progression::Progression::from_bytes(&bytes, results.alpha_mode).value {
                    let diff_str = match prog_res.difficulty {
                        0 => "Normal",
                        1 => "Nightmare",
                        2 => "Hell",
                        _ => "Unknown",
                    };
                    om.println(&format!("  Difficulty: {}", diff_str));
                }

                if results.fidelity_score < 1.0 || results.alpha_mode {
                    om.println("  [FORENSIC RATIONALE]");
                    let mut unique_findings = std::collections::HashSet::new();
                    for finding in &results.forensic_audit.findings {
                        // For Alpha, show everything. For others, show only speculative/fragile.
                        if results.alpha_mode || finding.confidence < d2r_core::domain::item::axiom_meta::Confidence::VerifiedTruth {
                            let line = format!("    - [{:?}] [{:?}] {}", finding.confidence, finding.intentionality, finding.rationale);
                            if unique_findings.insert(line.clone()) {
                                om.println(&line);
                            }
                        }
                    }
                }

            }
            for issue in &report.issues {
                om.println(&format!(
                    "  [{}] {}{}",
                    issue.kind,
                    issue.message,
                    issue
                        .bit_offset
                        .map(|b| format!(" (bit {})", b))
                        .unwrap_or_default()
                ));
            }
            for action in &report.suggested_actions {
                om.println(&format!("  [ACTION] ({:.2}) {}: {}", action.confidence, action.kind, action.command));
            }
            om.println("");
        }
        if failed {
            all_ok = false;
        }
    }

    if all_ok {
        om.summary("Verification complete: OK");
        Ok(())
    } else {
        om.summary("Verification complete: FAILED");
        process::exit(1);
    }
}

fn dump_bits(om: &mut d2r_core::verify::OutputManager, path: &str, start_bit: u64, count: u64) -> anyhow::Result<()> {
    let bytes = fs::read(path)?;
    om.summary(&format!("Dumping {} bits starting at {}:", count, start_bit));
    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    reader.skip(start_bit as u32)?;
    let mut line = String::new();
    for i in 0..count {
        let bit = if reader.read_bit()? { '1' } else { '0' };
        line.push(bit);
        if (i + 1) % 8 == 0 {
            line.push(' ');
        }
        if (i + 1) % 64 == 0 {
            om.println(&line);
            line.clear();
        }
    }
    if !line.is_empty() {
        om.println(&line);
    }
    om.println("");
    Ok(())
}
