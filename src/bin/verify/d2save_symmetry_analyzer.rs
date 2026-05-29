use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::symmetry_analyzer::analyze_symmetry;
use std::{env, process};

fn main() {
    let mut parser = ArgParser::new("d2save_symmetry_analyzer");
    parser.add_flag("json", "Output results in JSON format")
        .long("json");
    parser.add_opt("diff-json", "Path to the diff JSON file")
        .long("diff-json");
    parser.add_opt("timeline-json", "Path to the timeline JSON file")
        .long("timeline-json");

    let args = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(a) => a,
        Err(ArgError::Help(_)) => {
            println!("d2save_symmetry_analyzer - Automated Frameshift Diagnosis Tool");
            println!("{}", parser.usage());
            return;
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
            process::exit(1);
        }
    };

    let diff_path = args.get("diff-json").expect("diff-json is required");
    let timeline_path = args.get("timeline-json").expect("timeline-json is required");
    let is_json = args.is_set("json");

    match analyze_symmetry(diff_path, timeline_path) {
        Ok(report) => {
            if is_json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("=== Symmetry Analysis Report ===");
                println!("Mismatch Offset : {}", report.mismatch_offset);
                println!("Rupture Field   : {}", report.rupture_field);
                println!("DNA Class       : {}", report.dna_class);
                println!("Prescription    : {}", report.prescription);
                println!("================================");
            }
        }
        Err(e) => {
            eprintln!("Analysis failed: {:?}", e);
            process::exit(1);
        }
    }
}
