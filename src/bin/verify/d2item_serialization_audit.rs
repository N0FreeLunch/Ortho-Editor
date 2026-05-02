use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use d2r_core::verify::symmetry::calculate_symmetry_diff;
use std::env;
use std::fs;

fn main() {
    let mut parser = ArgParser::new("d2item_serialization_audit");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));

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

    let path = parsed.get("save_file").unwrap();
    let use_json = parsed.is_set("json");

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", path, e);
            std::process::exit(1);
        }
    };

    match calculate_symmetry_diff(&bytes, None, true) {
        Ok(report) => {
            if use_json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("Serialization Audit for: {}", path);
                println!("{:-<80}", "");
                println!("{:>5} | {:<10} | {:>8} | {:>8} | {:<5}", "Idx", "Code", "OrigLen", "SerLen", "Match");
                println!("{:-<80}", "");
                
                for (i, item) in report.items.iter().enumerate() {
                    println!("{:5} | {:10} | {:8} | {:8} | {:5}",
                        i, item.code, item.original_len, item.target_len,
                        if item.is_match { "OK" } else { "FAIL" }
                    );
                    if !item.is_match {
                        if let Some(m_type) = &item.mismatch_type {
                            println!("      [REASON] {}", m_type);
                        }
                        if let Some(seg) = &item.segment {
                            println!("      [SEGMENT] {}", seg);
                        }
                        if let Some(offset) = item.first_mismatch_offset {
                            println!("      [OFFSET] bit {}", offset);
                        }
                    }
                }
                println!("{:-<80}", "");
                
                if report.success {
                    println!("MATCH: 100% fidelity");
                } else {
                    println!("FAIL: Mismatches detected.");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error during symmetry audit: {}", e);
            std::process::exit(1);
        }
    }
}
