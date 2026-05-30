use anyhow::{Context, Result};
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::symmetry_analyzer::SymmetryReport;
use std::io::Read;
use std::{env, fs, io};

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2save_taxonomy_harvester");
    parser
        .add_opt("input", "Path to a symmetry-json report (use - for stdin)")
        .short('i')
        .long("input");

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
            return Ok(());
        }
    };

    let input_path = parsed.get("input").context("Missing --input argument")?;

    let report_content = if input_path == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(input_path).with_context(|| format!("Failed to read {}", input_path))?
    };

    let report: SymmetryReport =
        serde_json::from_str(&report_content).context("Failed to parse SymmetryReport JSON")?;

    let yaml_entry = generate_taxonomy_yaml(&report);
    println!("{}", yaml_entry);

    Ok(())
}

fn generate_taxonomy_yaml(report: &SymmetryReport) -> String {
    let id = format!(
        "{}-{:03}",
        report.dna_class.replace('_', "-"),
        report.mismatch_offset % 1000
    );

    format!(
        "- id: \"{}\"\n  name: \"{} (Auto-Harvested)\"\n  dna_class: \"{}\"\n  rupture_field: \"{}\"\n  pattern: \"...\"\n  prescription: \"{}\"",
        id, report.rupture_field, report.dna_class, report.rupture_field, report.prescription
    )
}
