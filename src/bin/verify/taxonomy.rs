use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::save_integrity::verify_save_integrity;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Taxonomy {
    Success,
    ParseFailure,
    BitstreamDesync,
    ChecksumMismatch,
    Other,
}

impl Taxonomy {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::ParseFailure => "ParseFailure",
            Self::BitstreamDesync => "BitstreamDesync",
            Self::ChecksumMismatch => "ChecksumMismatch",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone)]
struct FixtureResult {
    file: String,
    taxonomy: Taxonomy,
    fidelity: f32,
    issue_count: usize,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_taxonomy");
    parser.add_spec(ArgSpec::option("dir", None, Some("dir"), "Directory that contains .d2s files"));
    parser.add_spec(ArgSpec::option("report", None, Some("report"), "Output Markdown report file"));

    let parsed = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let dir_str = parsed.get("dir").map(|v| v.as_str()).unwrap_or("tests/fixtures/savegames/original");
    let report_path = parsed.get("report").map(|v| v.as_str()).unwrap_or("antigravity/outputs/batch_audit/taxonomy_report.md");

    let start_time = Instant::now();
    let dir_path = Path::new(dir_str);
    
    if !dir_path.exists() {
        anyhow::bail!("Directory not found: {}", dir_str);
    }

    let entries: Vec<_> = fs::read_dir(dir_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("d2s"))
        .collect();

    println!("Auditing {} fixtures in {}...", entries.len(), dir_str);

    let results: Vec<FixtureResult> = entries
        .par_iter()
        .map(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(_) => return FixtureResult {
                    file: file_name,
                    taxonomy: Taxonomy::Other,
                    fidelity: 0.0,
                    issue_count: 0,
                },
            };

            let (report, failed) = verify_save_integrity(&file_name, &bytes);
            
            let mut taxonomy = if !failed {
                Taxonomy::Success
            } else {
                let mut tax = Taxonomy::Other;
                
                // Check for ChecksumMismatch
                if let Some(res) = &report.scan_results {
                    if let Some(calculated) = &res.checksum_calculated {
                        if &res.checksum_stored != calculated {
                            tax = Taxonomy::ChecksumMismatch;
                        }
                    }
                }

                if tax == Taxonomy::Other {
                    // Map issues to taxonomy
                    let kinds: std::collections::HashSet<_> = report.issues.iter().map(|i| i.kind.as_str()).collect();
                    if kinds.contains("item_parse") {
                        tax = Taxonomy::ParseFailure;
                    } else if kinds.contains("item_parity") {
                        tax = Taxonomy::BitstreamDesync;
                    }
                }
                tax
            };

            FixtureResult {
                file: file_name,
                taxonomy,
                fidelity: report.scan_results.as_ref().map(|r| r.fidelity_score).unwrap_or(0.0),
                issue_count: report.issues.len(),
            }
        })
        .collect();

    generate_report(&results, report_path)?;

    println!("Done in {:?}", start_time.elapsed());
    println!("Report generated at: {}", report_path);

    Ok(())
}

fn generate_report(results: &[FixtureResult], path: &str) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Batch Taxonomy Audit Report (Rust)\n\n");
    // Simple timestamp without chrono
    md.push_str("Date: 2026-05-13 (Manual Sync)\n\n");

    let mut stats = std::collections::HashMap::new();
    for res in results {
        *stats.entry(res.taxonomy).or_insert(0) += 1;
    }

    md.push_str("## Summary Statistics\n\n");
    md.push_str("| Taxonomy | Count |\n");
    md.push_str("| :--- | :--- |\n");
    for tax in &[Taxonomy::Success, Taxonomy::ParseFailure, Taxonomy::BitstreamDesync, Taxonomy::ChecksumMismatch, Taxonomy::Other] {
        md.push_str(&format!("| {} | {} |\n", tax.as_str(), stats.get(tax).unwrap_or(&0)));
    }
    md.push_str("\n");

    md.push_str("## Detailed Results\n\n");
    md.push_str("| Fixture | Taxonomy | Fidelity | Issues |\n");
    md.push_str("| :--- | :--- | :--- | :--- |\n");

    let mut sorted_results = results.to_vec();
    sorted_results.sort_by(|a, b| a.file.cmp(&b.file));

    for res in sorted_results {
        md.push_str(&format!("| {} | {} | {:.1} | {} |\n", 
            res.file, res.taxonomy.as_str(), res.fidelity, res.issue_count));
    }

    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, md)?;

    Ok(())
}
