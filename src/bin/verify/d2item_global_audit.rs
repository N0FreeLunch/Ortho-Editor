use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, SymmetryOptions, ItemDiff};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "geometry" => Some(Self::Geometry),
            "rwset" | "rw" | "set" => Some(Self::RWSet),
            "stat" => Some(Self::Stat),
            "nudge" => Some(Self::Nudge),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct MismatchRow {
    item_label: String,
    code: String,
    mismatch_type: String,
    segment: String,
    first_mismatch_offset: Option<usize>,
}

#[derive(Serialize)]
struct AuditResult {
    status: String,
    filename: String,
    item_count: usize,
    avg_fidelity: f32,
    hint: String,
    family: Option<FailureFamily>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mismatch_rows: Vec<MismatchRow>,
}

#[derive(Serialize)]
struct GlobalAuditReport {
    target_dir: String,
    total_files: usize,
    total_pass: usize,
    total_fail: usize,
    total_items: usize,
    global_avg_fidelity: f32,
    failure_breakdown: HashMap<String, usize>,
    results: Vec<AuditResult>,
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

fn generate_markdown_report(report: &GlobalAuditReport) -> String {
    let mut md = String::new();
    md.push_str(&format!("# Global Item Symmetry Audit: {}\n\n", report.target_dir));
    
    md.push_str("## SUMMARY\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("| :--- | :--- |\n");
    md.push_str(&format!("| Total Files | {} |\n", report.total_files));
    md.push_str(&format!("| Total Pass | {} |\n", report.total_pass));
    md.push_str(&format!("| Total Fail | {} |\n", report.total_fail));
    md.push_str(&format!("| Total Items | {} |\n", report.total_items));
    md.push_str(&format!("| Global Fidelity | {:.2}% |\n\n", report.global_avg_fidelity));

    if !report.failure_breakdown.is_empty() {
        md.push_str("## FAILURE BREAKDOWN\n\n");
        md.push_str("| Family | Count |\n");
        md.push_str("| :--- | :--- |\n");
        let mut families: Vec<_> = report.failure_breakdown.keys().collect();
        families.sort();
        for family in families {
            md.push_str(&format!("| {} | {} |\n", family, report.failure_breakdown[family]));
        }
        md.push_str("\n");
    }

    md.push_str("## DETAILED RESULTS\n\n");
    md.push_str("| Status | Filename | Items | Fidelity | Hint |\n");
    md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
    for res in &report.results {
        md.push_str(&format!(
            "| {} | {} | {} | {:.2}% | {} |\n",
            res.status, res.filename, res.item_count, res.avg_fidelity, res.hint
        ));
    }
    
    md
}

fn main() {
    let mut parser = ArgParser::new("d2item_global_audit");
    parser.add_spec(ArgSpec::positional("target_dir", "Directory containing .d2s files").optional());
    parser.add_spec(ArgSpec::option("filter", None, Some("filter"), "Filter failures by family (Geometry, RWSet, Stat, Nudge, Unknown)"));
    parser.add_spec(ArgSpec::flag("summary-only", None, Some("summary-only"), "Show only the summary block"));
    parser.add_spec(ArgSpec::flag("detailed", Some('d'), Some("detailed"), "Report all mismatches in a file, not just the first one"));
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));
    parser.add_spec(ArgSpec::option("output", Some('o'), Some("output"), "Save execution output to a file"));
    
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

    let filter_family = parsed.get("filter").and_then(|s| FailureFamily::from_str(s));
    let summary_only = parsed.is_set("summary-only");
    let detailed = parsed.is_set("detailed");
    let output_json_flag = parsed.is_set("json");
    let output_path = parsed.get("output");

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

    let mut total_files = 0;
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut cumulative_fidelity = 0.0;
    let mut total_items = 0;
    let mut failure_breakdown: HashMap<FailureFamily, usize> = HashMap::new();
    let mut results: Vec<AuditResult> = Vec::new();

    if output_path.is_none() && !output_json_flag && !summary_only {
        println!("Global Item Symmetry Audit: {}", target_dir);
        println!("{:-<100}", "");
        println!(
            "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
            "Status", "Filename", "Items", "Fidelity", "Hint"
        );
        println!("{:-<100}", "");
    }

    for entry in entries {
        let file_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        total_files += 1;

        let bytes = match fs::read(&file_path) {
            Ok(b) => b,
            Err(e) => {
                let err_msg = format!("Read error: {}", e);
                if output_path.is_none() && !output_json_flag {
                    println!(
                        "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
                        "[ERROR]", file_name, "-", "-", err_msg
                    );
                }
                results.push(AuditResult {
                    status: "[ERROR]".to_string(),
                    filename: file_name,
                    item_count: 0,
                    avg_fidelity: 0.0,
                    hint: err_msg,
                    family: Some(FailureFamily::Unknown),
                    mismatch_rows: Vec::new(),
                });
                total_fail += 1;
                continue;
            }
        };

        let options = SymmetryOptions {
            roundtrip: true,
            target_index: None,
            fail_fast: !detailed,
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

                let mut mismatch_rows = Vec::new();
                let mut first_fail_family = None;

                let hint = if !report.success {
                    if detailed {
                        for (i, it) in report.items.iter().enumerate() {
                            if !it.is_match {
                                let family = classify_failure(it);
                                if first_fail_family.is_none() {
                                    first_fail_family = Some(family);
                                }
                                *failure_breakdown.entry(family).or_insert(0) += 1;
                                mismatch_rows.push(MismatchRow {
                                    item_label: format!("Item {}", i),
                                    code: it.code.clone(),
                                    mismatch_type: it.mismatch_type.clone().unwrap_or_default(),
                                    segment: it.segment.clone().unwrap_or_default(),
                                    first_mismatch_offset: it.first_mismatch_offset.map(|o| o as usize),
                                });
                            }
                        }
                        format!("{} failures detected", mismatch_rows.len())
                    } else if let Some(first_fail) = report.items.iter().find(|it| !it.is_match) {
                        let family = classify_failure(first_fail);
                        first_fail_family = Some(family);
                        *failure_breakdown.entry(family).or_insert(0) += 1;
                        format!("{} {}", family.as_tag(), first_fail.mismatch_type.as_deref().unwrap_or("Mismatch"))
                    } else {
                        "Unknown failure".to_string()
                    }
                } else {
                    "".to_string()
                };

                // Filter logic
                if let Some(f) = filter_family {
                    if report.success || first_fail_family != Some(f) {
                        continue;
                    }
                }

                results.push(AuditResult {
                    status: status.to_string(),
                    filename: file_name.clone(),
                    item_count,
                    avg_fidelity,
                    hint: hint.clone(),
                    family: first_fail_family,
                    mismatch_rows,
                });

                if output_path.is_none() && !output_json_flag && !summary_only {
                    println!(
                        "{:<8} | {:<40} | {:>8} | {:>9.2}% | {:<20}",
                        status, file_name, item_count, avg_fidelity, hint
                    );
                }
            }
            Err(e) => {
                let err_msg = format!("Audit error: {}", e);
                if output_path.is_none() && !output_json_flag {
                    println!(
                        "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
                        "[FAIL]", file_name, "-", "-", err_msg
                    );
                }
                results.push(AuditResult {
                    status: "[FAIL]".to_string(),
                    filename: file_name,
                    item_count: 0,
                    avg_fidelity: 0.0,
                    hint: err_msg,
                    family: Some(FailureFamily::Unknown),
                    mismatch_rows: Vec::new(),
                });
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

    let mut breakdown_str = HashMap::new();
    for (f, count) in failure_breakdown.iter() {
        breakdown_str.insert(format!("{:?}", f), *count);
    }

    let global_report = GlobalAuditReport {
        target_dir: target_dir.to_string(),
        total_files,
        total_pass,
        total_fail,
        total_items,
        global_avg_fidelity,
        failure_breakdown: breakdown_str,
        results,
    };

    if let Some(out) = output_path {
        let content = if out.ends_with(".json") || output_json_flag {
            serde_json::to_string_pretty(&global_report).unwrap()
        } else {
            generate_markdown_report(&global_report)
        };
        
        if let Some(parent) = Path::new(out).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).expect("Failed to create output directory");
            }
        }
        fs::write(out, content).expect("Failed to write output file");
        println!("Report saved to: {}", out);
    } else if output_json_flag {
        println!("{}", serde_json::to_string_pretty(&global_report).unwrap());
    } else {
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
    }

    if total_fail > 0 {
        std::process::exit(1);
    }
}
