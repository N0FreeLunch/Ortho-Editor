use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
struct BinMapping {
    name: String,
    path: String,
    exists: bool,
    is_mismatch: bool,
}

#[derive(Serialize)]
struct AuditResults {
    explicit_mappings: Vec<BinMapping>,
    unregistered_sources: Vec<String>,
    orphaned_entries: Vec<String>,
}

fn main() {
    let parser = ArgParser::new("d2_binary_target_auditor")
        .description("Audits Cargo binary targets and maps them to source files.");

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let cargo_toml_path = "Cargo.toml";
    let cargo_content = fs::read_to_string(cargo_toml_path).expect("failed to read Cargo.toml");

    let mut explicit_mappings = Vec::new();
    let mut current_name = String::new();
    let mut current_path = String::new();

    // Simple line-based parser for [[bin]] blocks
    for line in cargo_content.lines() {
        let line = line.trim();
        if line == "[[bin]]" {
            if !current_name.is_empty() && !current_path.is_empty() {
                explicit_mappings.push(create_mapping(&current_name, &current_path));
            }
            current_name.clear();
            current_path.clear();
        } else if line.starts_with("name =") {
            current_name = line.split('"').nth(1).unwrap_or("").to_string();
        } else if line.starts_with("path =") {
            current_path = line.split('"').nth(1).unwrap_or("").to_string();
        }
    }
    // Last one
    if !current_name.is_empty() && !current_path.is_empty() {
        explicit_mappings.push(create_mapping(&current_name, &current_path));
    }

    // Scan directories
    let mut unregistered_sources = Vec::new();
    let bin_dirs = vec!["src/bin", "src/bin/verify"];
    for dir in bin_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    let path_str = path.to_str().unwrap().replace("\\", "/");
                    // Check if it's a registered file
                    if !explicit_mappings.iter().any(|m| m.path == path_str) {
                        unregistered_sources.push(path_str);
                    }
                }
            }
        }
    }

    let orphaned_entries: Vec<String> = explicit_mappings
        .iter()
        .filter(|m| !m.exists)
        .map(|m| format!("{} ({})", m.name, m.path))
        .collect();

    let results = AuditResults {
        explicit_mappings,
        unregistered_sources,
        orphaned_entries,
    };

    if parsed.is_json() {
        let metadata = ReportMetadata::new("d2_binary_target_auditor", cargo_toml_path, "0.1.0");
        let report = Report::new(metadata, ReportStatus::Ok).with_results(results);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Binary Target Audit Report\n==========================");
        println!("\nExplicit Mappings:");
        for m in &results.explicit_mappings {
            let status = if !m.exists {
                " [MISSING]"
            } else if m.is_mismatch {
                " [MISMATCH]"
            } else {
                ""
            };
            println!("  {} -> {}{}", m.name, m.path, status);
        }

        if !results.unregistered_sources.is_empty() {
            println!("\nUnregistered Sources (implicit or missing from Cargo.toml):");
            for s in &results.unregistered_sources {
                println!("  {}", s);
            }
        }

        if !results.orphaned_entries.is_empty() {
            println!("\nOrphaned Entries (defined in Cargo.toml but file missing):");
            for o in &results.orphaned_entries {
                println!("  {}", o);
            }
        }
    }
}

fn create_mapping(name: &str, path: &str) -> BinMapping {
    let exists = Path::new(path).exists();
    let file_stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let is_mismatch = file_stem != name;

    BinMapping {
        name: name.to_string(),
        path: path.to_string(),
        exists,
        is_mismatch,
    }
}
