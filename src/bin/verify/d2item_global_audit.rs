use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, SymmetryOptions};
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let mut parser = ArgParser::new("d2item_global_audit");
    parser.add_spec(ArgSpec::positional("target_dir", "Directory containing .d2s files").optional());
    
    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let target_dir = parsed
        .get("target_dir")
        .map(|s| s.as_str())
        .unwrap_or("tests/fixtures/savegames/original");

    let path = Path::new(target_dir);
    if !path.is_dir() {
        eprintln!("Error: target path '{}' is not a directory.", target_dir);
        std::process::exit(1);
    }

    let mut entries: Vec<_> = match fs::read_dir(path) {
        Ok(read_dir) => read_dir
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file() 
                && e.path().extension().map_or(false, |ext| ext == "d2s")
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", target_dir, e);
            std::process::exit(1);
        }
    };

    // Deterministic sort by filename
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No .d2s files found in {}", target_dir);
        return;
    }

    println!("Global Item Symmetry Audit: {}", target_dir);
    println!("{:-<100}", "");
    println!(
        "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
        "Status", "Filename", "Items", "Fidelity", "Hint"
    );
    println!("{:-<100}", "");

    let mut total_files = 0;
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut cumulative_fidelity = 0.0;
    let mut total_items = 0;

    for entry in entries {
        let file_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        total_files += 1;

        let bytes = match fs::read(&file_path) {
            Ok(b) => b,
            Err(e) => {
                println!(
                    "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
                    "[ERROR]", file_name, "-", "-", format!("Read error: {}", e)
                );
                total_fail += 1;
                continue;
            }
        };

        let options = SymmetryOptions {
            roundtrip: true,
            target_index: None,
            fail_fast: false,
        };

        match calculate_symmetry_diff(&bytes, None, options) {
            Ok(report) => {
                let status = if report.success { "[PASS]" } else { "[FAIL]" };
                if report.success {
                    total_pass += 1;
                } else {
                    total_fail += 1;
                }

                let item_count = report.items.len();
                total_items += item_count;
                
                let avg_fidelity = if item_count > 0 {
                    let sum: f32 = report.items.iter().map(|it| it.fidelity_score).sum();
                    sum / item_count as f32
                } else {
                    100.0
                };
                cumulative_fidelity += avg_fidelity;

                let hint = if !report.success {
                    report.items.iter()
                        .find(|it| !it.is_match)
                        .map(|it| {
                            it.mismatch_type.as_deref().unwrap_or("Mismatch")
                        })
                        .unwrap_or("Unknown failure")
                } else {
                    ""
                };

                println!(
                    "{:<8} | {:<40} | {:>8} | {:>9.2}% | {:<20}",
                    status, file_name, item_count, avg_fidelity, hint
                );
            }
            Err(e) => {
                println!(
                    "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
                    "[FAIL]", file_name, "-", "-", format!("Audit error: {}", e)
                );
                total_fail += 1;
            }
        }
    }

    let global_avg_fidelity = if total_files > 0 {
        cumulative_fidelity / total_files as f32
    } else {
        0.0
    };

    println!("{:-<100}", "");
    println!("SUMMARY:");
    println!("  Total Files:       {}", total_files);
    println!("  Total Pass:        {}", total_pass);
    println!("  Total Fail:        {}", total_fail);
    println!("  Total Items:       {}", total_items);
    println!("  Global Fidelity:   {:.2}%", global_avg_fidelity);
    println!("{:-<100}", "");

    if total_fail > 0 {
        std::process::exit(1);
    }
}
