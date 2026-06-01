use anyhow::{Context, Result};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::sba::SbaBaseline;
use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_atlas")
        .description("Human-readable structural viewer for SBA baseline JSON");

    parser.add_spec(
        ArgSpec::option(
            "baseline",
            Some('b'),
            Some("baseline"),
            "Path to SBA baseline JSON file",
        )
        .required(),
    );

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let baseline_path = parsed.get("baseline").unwrap();
    let content = fs::read_to_string(baseline_path)
        .with_context(|| format!("Failed to read baseline file: {}", baseline_path))?;

    let baseline: SbaBaseline =
        serde_json::from_str(&content).with_context(|| "Failed to parse SBA baseline JSON")?;

    println!("SBA Atlas: {}", baseline.fixture);
    println!("Total Items: {}", baseline.items.len());
    println!("{:-<80}", "");

    for item in &baseline.items {
        println!("Item: {} (Code: {})", item.path, item.code);
        println!(
            "  Range: {} - {} (total {} bits)",
            item.range.start,
            item.range.end,
            item.range.end - item.range.start
        );

        for segment in &item.segments {
            let indent = "  ".repeat(segment.depth + 1);
            println!(
                "{}[{:>4} - {:>4}] {}",
                indent, segment.start, segment.end, segment.label
            );
        }
        println!("{:-<40}", "");
    }

    Ok(())
}
