use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, SymmetryOptions, ItemDiff};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FailureFamily {
    Geometry,
    RWSet,
    Stat,
    Nudge,
    Unknown,
}

impl FailureFamily {
    fn as_tag(&self) -> String {
        format!("[{}]", match self {
            Self::Geometry => "Geometry",
            Self::RWSet => "RW/Set",
            Self::Stat => "Stat",
            Self::Nudge => "Nudge",
            Self::Unknown => "Unknown",
        })
    }
}

fn classify_failure(diff: &ItemDiff) -> FailureFamily {
    let mismatch_type = diff.mismatch_type.as_deref().unwrap_or("");
    let offset = diff.first_mismatch_offset.unwrap_or(0);
    let version = diff.version;
    let flags = diff.flags;

    // Alpha v105 specific RW/Shadow check (approximation)
    let is_rw_or_shadow = if version == 5 || version == 1 {
        let is_shadow = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
        let is_rw = !is_shadow && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0);
        is_rw || is_shadow
    } else {
        (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0
    };

    if mismatch_type == "Length" {
        let diff_bits = (diff.original_len as i64 - diff.target_len as i64).abs();
        if offset < 100 {
            FailureFamily::Geometry
        } else if diff_bits <= 7 {
            FailureFamily::Nudge
        } else {
            FailureFamily::Geometry
        }
    } else if mismatch_type.contains("Gap") {
        FailureFamily::Geometry
    } else if mismatch_type == "Content" {
        if is_rw_or_shadow {
            FailureFamily::RWSet
        } else if offset >= 100 {
            FailureFamily::Stat
        } else {
            FailureFamily::Geometry
        }
    } else {
        FailureFamily::Unknown
    }
}

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
    let mut failure_breakdown: HashMap<FailureFamily, usize> = HashMap::new();

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
                    if let Some(first_fail) = report.items.iter().find(|it| !it.is_match) {
                        let family = classify_failure(first_fail);
                        *failure_breakdown.entry(family).or_insert(0) += 1;
                        format!("{} {}", family.as_tag(), first_fail.mismatch_type.as_deref().unwrap_or("Mismatch"))
                    } else {
                        "Unknown failure".to_string()
                    }
                } else {
                    "".to_string()
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
                *failure_breakdown.entry(FailureFamily::Unknown).or_insert(0) += 1;
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
    
    if !failure_breakdown.is_empty() {
        println!("\nFAILURE BREAKDOWN:");
        let mut families: Vec<_> = failure_breakdown.keys().collect();
        families.sort_by_key(|f| f.as_tag());
        for family in families {
            println!("  {:<12}: {}", family.as_tag(), failure_breakdown[family]);
        }
    }
    println!("{:-<100}", "");

    if total_fail > 0 {
        std::process::exit(1);
    }
}
