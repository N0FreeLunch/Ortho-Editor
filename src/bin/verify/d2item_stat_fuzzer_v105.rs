use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
struct FuzzResult {
    stat_id: u32,
    candidates: Vec<Candidate>,
    winner: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Candidate {
    bits: u32,
    audit_exit_code: i32,
    #[serde(rename = "match")]
    is_match: bool,
    fidelity_score: f32,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_stat_fuzzer_v105");
    parser.add_spec(ArgSpec::option("fixture", Some('f'), Some("fixture"), "Path to the save game fixture"));
    parser.add_spec(ArgSpec::option("stat", Some('s'), Some("stat"), "Stat ID to fuzz"));
    parser.add_spec(ArgSpec::option("range", Some('r'), Some("range"), "Bit width range (e.g. 8..32)"));
    parser.add_spec(ArgSpec::option("item-index", Some('i'), Some("item-index"), "Target specific item index"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixture_path = parsed.get("fixture").expect("Missing --fixture");
    let stat_id_str = parsed.get("stat").expect("Missing --stat");
    let stat_id: u32 = stat_id_str.parse().expect("Invalid stat ID");
    let range_str = parsed.get("range").map(|s| s.as_str()).unwrap_or("8..32");
    let item_index = parsed.get("item-index");

    let range: Vec<u32> = if let Some((start_str, end_str)) = range_str.split_once("..") {
        let start: u32 = start_str.parse().expect("Invalid range start");
        let end: u32 = end_str.parse().expect("Invalid range end");
        (start..=end).collect()
    } else {
        vec![range_str.parse().expect("Invalid range value")]
    };

    println!("Fuzzing stat {} over range {:?} using fixture {}", stat_id, range, fixture_path);

    let mut results = FuzzResult {
        stat_id,
        candidates: Vec::new(),
        winner: None,
    };

    // Locate original registry
    let base_data_path = env::var("D2R_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../d2r-data"));
    let original_registry_path = base_data_path.join("constants/alpha_v105_forensics.json");
    let original_content = fs::read_to_string(&original_registry_path)?;
    let registry_json: Value = serde_json::from_str(&original_content)?;

    for &width in &range {
        print!("Trying width {}... ", width);
        std::io::Write::flush(&mut std::io::stdout())?;

        // Patch registry
        let mut temp_registry = registry_json.clone();
        patch_registry(&mut temp_registry, stat_id, width);

        // Create temp data dir
        let temp_dir = PathBuf::from("tmp").join(format!("fuzz_stat_{}_{}", stat_id, width));
        let temp_constants_dir = temp_dir.join("constants");
        fs::create_dir_all(&temp_constants_dir)?;
        let temp_json_path = temp_constants_dir.join("alpha_v105_forensics.json");
        fs::write(&temp_json_path, serde_json::to_string_pretty(&temp_registry)?)?;

        // Run audit
        let mut cmd = Command::new("cargo");
        cmd.arg("run")
            .arg("--quiet")
            .arg("--bin")
            .arg("d2item_serialization_audit")
            .arg("--")
            .arg(fixture_path)
            .arg("--json");
        
        if let Some(idx) = item_index {
            cmd.arg("--target").arg(idx);
        }

        cmd.env("D2R_DATA_PATH", &temp_dir);

        let output = cmd.output()?;
        let exit_code = output.status.code().unwrap_or(-1);
        
        let mut is_match = false;
        let mut fidelity = 0.0;

        if output.status.success() || !output.stdout.is_empty() {
            if let Ok(report) = serde_json::from_slice::<Value>(&output.stdout) {
                is_match = report["success"].as_bool().unwrap_or(false);
                
                // If targeting an item, check if THAT item matches
                if item_index.is_some() {
                    if let Some(items) = report["items"].as_array() {
                        if let Some(item) = items.first() {
                            is_match = item["is_match"].as_bool().unwrap_or(false);
                            fidelity = item["fidelity_score"].as_f64().unwrap_or(0.0) as f32;
                        }
                    }
                }
            }
        }

        println!("Match: {}, Fidelity: {}", is_match, fidelity);

        results.candidates.push(Candidate {
            bits: width,
            audit_exit_code: exit_code,
            is_match,
            fidelity_score: fidelity,
        });

        // Cleanup temp dir
        let _ = fs::remove_dir_all(&temp_dir);

        if is_match {
            results.winner = Some(width);
            break;
        }
    }

    println!("{}", serde_json::to_string_pretty(&results)?);

    Ok(())
}

fn patch_registry(registry: &mut Value, stat_id: u32, width: u32) {
    let stat_id_str = stat_id.to_string();

    // 1. Try to patch mappings first if it exists
    if let Some(mappings) = registry["mappings"].as_object_mut() {
        if let Some(mapping) = mappings.get_mut(&stat_id_str) {
            mapping["save_bits"] = Value::from(width);
            return;
        }
    }

    // 2. Fallback to stats
    if let Some(stats) = registry["stats"].as_object_mut() {
        if let Some(stat) = stats.get_mut(&stat_id_str) {
            stat["width"] = Value::from(width);
            return;
        }
    }

    // 3. If neither exists, add to stats as a new entry
    if let Some(stats) = registry["stats"].as_object_mut() {
        let mut new_stat = serde_json::Map::new();
        new_stat.insert("name".to_string(), Value::from(format!("fuzzed_stat_{}", stat_id)));
        new_stat.insert("width".to_string(), Value::from(width));
        stats.insert(stat_id_str, Value::Object(new_stat));
    }
}
