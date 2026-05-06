use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::calculate_symmetry_diff;
use std::{env, fs, process};

fn main() -> anyhow::Result<()> {
    unsafe { std::env::set_var("D2R_ITEM_TRACE", "1") };
    let mut parser = ArgParser::new("SymmetryBitDiff")
        .description("Compares item-by-item bitstream symmetry. Supports memory roundtrip for a single file.");
    parser.add_spec(ArgSpec::positional("file_a", "path to the save file (.d2s)"));
    parser.add_spec(ArgSpec::positional("file_b", "path to the second save file (.d2s)").optional());
    parser.add_spec(ArgSpec::flag("roundtrip", Some('r'), Some("roundtrip"), "if set, compares file_a with its own reserialized items"));
    parser.add_spec(ArgSpec::flag("json", Some('j'), Some("json"), "if set, outputs results in JSON format"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("Error: {}", e),
    };

    let path_a = parsed.get("file_a").ok_or_else(|| anyhow::anyhow!("file_a is required"))?;
    let path_b = parsed.get("file_b");
    let roundtrip = parsed.is_set("roundtrip");
    let json = parsed.is_set("json");

    let bytes_a = fs::read(path_a)?;
    let bytes_b = match path_b {
        Some(p) => Some(fs::read(p)?),
        None => None,
    };

    let report = calculate_symmetry_diff(
        &bytes_a,
        bytes_b.as_deref(),
        d2r_core::verify::symmetry::SymmetryOptions::roundtrip(roundtrip),
    )?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "operation={} success={} items_a={} items_b={}",
            report.operation, report.success, report.item_count_a, report.item_count_b
        );
        for item in &report.items {
            if !item.is_match {
                println!(
                    "DIFF {} code={} kind={:?} bit={:?} segment={:?}",
                    item.label, item.code, item.mismatch_type, item.first_mismatch_offset, item.segment
                );
            }
        }
    }

    if report.success {
        Ok(())
    } else {
        process::exit(1);
    }
}
