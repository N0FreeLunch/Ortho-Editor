use anyhow::{Context, Result};
use std::env;
use std::fs;

use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::Save;
use d2r_core::verify::args::{ArgParser, ArgSpec};

use d2r_core::verify::sba::{SbaBaseline, SbaJsonPayload, flatten_item, verify_baseline};
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};

fn main() -> Result<()> {
    let mut parser = ArgParser::new("sba");
    parser.add_spec(
        ArgSpec::option(
            "fixture",
            None,
            Some("fixture"),
            "Path to the savegame fixture (.d2s)",
        )
        .required(),
    );
    parser.add_spec(
        ArgSpec::option(
            "baseline",
            None,
            Some("baseline"),
            "Path to the JSON baseline file",
        )
        .required(),
    );
    parser.add_spec(ArgSpec::flag(
        "generate",
        None,
        Some("generate"),
        "Generate a new baseline from the fixture",
    ));
    parser.add_spec(ArgSpec::flag(
        "verify",
        None,
        Some("verify"),
        "Verify the fixture against an existing baseline",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(args) {
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

    let fixture_path = parsed.get("fixture").cloned().unwrap();
    let baseline_path = parsed.get("baseline").cloned().unwrap();
    let is_generate = parsed.is_set("generate");
    let is_verify = parsed.is_set("verify");
    let is_json = parsed.is_set("json");

    if !is_generate && !is_verify {
        anyhow::bail!("Must specify either --generate or --verify");
    }

    // Enable tracing via environment variable to trigger BitCursor segment recording
    unsafe {
        env::set_var("D2R_ITEM_TRACE", "1");
    }

    let bytes = fs::read(&fixture_path)
        .with_context(|| format!("Failed to read fixture: {}", fixture_path))?;

    let save = Save::from_bytes(&bytes).context("Failed to parse save header")?;

    let huffman = HuffmanTree::new();
    let is_alpha = save.header.version == 105;

    let items = Item::read_player_items(&bytes, &huffman, is_alpha)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("Failed to read items")?;

    let mut flattened_items = Vec::new();
    for (i, item) in items.iter().enumerate() {
        flatten_item(item, &i.to_string(), &mut flattened_items);
    }

    let current_baseline = SbaBaseline {
        fixture: fixture_path.clone(),
        items: flattened_items,
    };

    let mut issues = Vec::new();

    if is_generate {
        let json = serde_json::to_string_pretty(&current_baseline)?;
        fs::write(&baseline_path, json)
            .with_context(|| format!("Failed to write baseline: {}", baseline_path))?;
        if !is_json {
            println!(
                "Generated baseline for {} items to {}",
                current_baseline.items.len(),
                baseline_path
            );
        }
    } else if is_verify {
        let baseline_json = fs::read_to_string(&baseline_path)
            .with_context(|| format!("Failed to read baseline: {}", baseline_path))?;
        let expected_baseline: SbaBaseline = serde_json::from_str(&baseline_json)?;

        if let Err(e) = verify_baseline(&expected_baseline, &current_baseline, &mut issues) {
            if !is_json {
                return Err(e);
            }
        }

        if !is_json {
            println!(
                "Verification successful: 0 segment mismatches across {} items.",
                current_baseline.items.len()
            );
        }
    }

    if is_json {
        let status = if issues.is_empty() {
            ReportStatus::Ok
        } else {
            ReportStatus::Fail
        };
        let version = if is_alpha {
            "105".to_string()
        } else {
            format!("0x{:04X}", save.header.version)
        };
        let metadata = ReportMetadata::new("sba", &fixture_path, &version);
        let payload = SbaJsonPayload {
            fixture: current_baseline.fixture,
            baseline: baseline_path,
            mode: if is_generate {
                "generate".to_string()
            } else {
                "verify".to_string()
            },
            item_count: current_baseline.items.len(),
            mismatch_count: issues.len(),
        };
        let report = Report::new(metadata, status)
            .with_results(payload)
            .with_issues(issues);

        println!("{}", serde_json::to_string(&report)?);
    }

    Ok(())
}
