use anyhow::{Context, Result};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::sba::SbaBaseline;
use std::collections::BTreeMap;
use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_atlas_diff")
        .description("Structural differential auditor for SBA baseline JSON files");

    parser.add_spec(
        ArgSpec::option("base", None, Some("base"), "Path to base SBA baseline JSON").required(),
    );
    parser.add_spec(
        ArgSpec::option(
            "target",
            None,
            Some("target"),
            "Path to target SBA baseline JSON",
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

    let base_path = parsed.get("base").unwrap();
    let target_path = parsed.get("target").unwrap();

    let base_content = fs::read_to_string(base_path)
        .with_context(|| format!("Failed to read base file: {}", base_path))?;
    let target_content = fs::read_to_string(target_path)
        .with_context(|| format!("Failed to read target file: {}", target_path))?;

    let base: SbaBaseline = serde_json::from_str(&base_content)
        .with_context(|| "Failed to parse base SBA baseline JSON")?;
    let target: SbaBaseline = serde_json::from_str(&target_content)
        .with_context(|| "Failed to parse target SBA baseline JSON")?;

    println!("SBA Diff Report");
    println!("Base:   {}", base.fixture);
    println!("Target: {}", target.fixture);
    println!("{:=<80}", "");

    let mut base_items = BTreeMap::new();
    for item in &base.items {
        base_items.insert((item.path.clone(), item.code.clone()), item);
    }

    let mut target_items = BTreeMap::new();
    for item in &target.items {
        target_items.insert((item.path.clone(), item.code.clone()), item);
    }

    let mut all_keys: Vec<_> = base_items.keys().chain(target_items.keys()).collect();
    all_keys.sort();
    all_keys.dedup();

    let mut total_diffs = 0;

    for key in all_keys {
        let (path, code) = key;
        let base_item = base_items.get(key);
        let target_item = target_items.get(key);

        match (base_item, target_item) {
            (Some(b), Some(t)) => {
                let mut item_diffs = Vec::new();

                if b.range.start != t.range.start || b.range.end != t.range.end {
                    item_diffs.push(format!(
                        "  [RANGE SHIFT] Start: {} -> {} ({:+}), End: {} -> {} ({:+})",
                        b.range.start,
                        t.range.start,
                        t.range.start as i64 - b.range.start as i64,
                        b.range.end,
                        t.range.end,
                        t.range.end as i64 - b.range.end as i64
                    ));
                }

                let max_segments = b.segments.len().max(t.segments.len());
                for i in 0..max_segments {
                    let b_seg = b.segments.get(i);
                    let t_seg = t.segments.get(i);

                    match (b_seg, t_seg) {
                        (Some(bs), Some(ts)) => {
                            let mut seg_changes = Vec::new();
                            if bs.label != ts.label {
                                seg_changes.push(format!("Label: {} -> {}", bs.label, ts.label));
                            }
                            if bs.start != ts.start {
                                seg_changes.push(format!(
                                    "Start: {} -> {} ({:+})",
                                    bs.start,
                                    ts.start,
                                    ts.start as i32 - bs.start as i32
                                ));
                            }
                            if bs.end != ts.end {
                                seg_changes.push(format!(
                                    "End: {} -> {} ({:+})",
                                    bs.end,
                                    ts.end,
                                    ts.end as i32 - bs.end as i32
                                ));
                            }

                            if !seg_changes.is_empty() {
                                item_diffs.push(format!(
                                    "  [SEGMENT DIFF] Index {}: {}",
                                    i,
                                    seg_changes.join(", ")
                                ));
                            }
                        }
                        (Some(bs), None) => {
                            item_diffs
                                .push(format!("  [SEGMENT REMOVED] Index {}: {}", i, bs.label));
                        }
                        (None, Some(ts)) => {
                            item_diffs.push(format!("  [SEGMENT NEW] Index {}: {}", i, ts.label));
                        }
                        (None, None) => unreachable!(),
                    }
                }

                if !item_diffs.is_empty() {
                    println!("Item: {} (Code: {}) [MODIFIED]", path, code);
                    for diff in item_diffs {
                        println!("{}", diff);
                    }
                    println!("{:-<40}", "");
                    total_diffs += 1;
                }
            }
            (Some(_), None) => {
                println!("Item: {} (Code: {}) [REMOVED]", path, code);
                println!("{:-<40}", "");
                total_diffs += 1;
            }
            (None, Some(_)) => {
                println!("Item: {} (Code: {}) [NEW]", path, code);
                println!("{:-<40}", "");
                total_diffs += 1;
            }
            (None, None) => unreachable!(),
        }
    }

    if total_diffs == 0 {
        println!("No differences found.");
    } else {
        println!("Total differences detected: {}", total_diffs);
    }

    Ok(())
}
