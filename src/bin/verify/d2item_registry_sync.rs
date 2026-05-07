use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_registry_sync");
    parser.add_spec(ArgSpec::option("stat", Some('s'), Some("stat"), "Target stat or mapping key"));
    parser.add_spec(ArgSpec::option("width", Some('w'), Some("width"), "New width (for stats)"));
    parser.add_spec(ArgSpec::option("save-bits", Some('b'), Some("save-bits"), "New save_bits (for mappings)"));
    parser.add_spec(ArgSpec::option("description", Some('d'), Some("description"), "New description"));
    parser.add_spec(ArgSpec::option("fidelity-hint", Some('f'), Some("fidelity-hint"), "New fidelity_hint"));
    parser.add_spec(ArgSpec::option("registry", None, Some("registry"), "Registry JSON path"));
    parser.add_spec(ArgSpec::flag("dry-run", None, Some("dry-run"), "Dry run mode"));

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

    let stat_id = parsed.get("stat").expect("Missing --stat");
    let width = parsed.get("width");
    let save_bits = parsed.get("save-bits");
    let description = parsed.get("description");
    let fidelity_hint = parsed.get("fidelity-hint");
    let dry_run = parsed.is_set("dry-run");

    let registry_path = if let Some(path) = parsed.get("registry") {
        PathBuf::from(path)
    } else {
        let base_data_path = env::var("D2R_DATA_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("../d2r-data"));
        base_data_path.join("constants/alpha_v105_forensics.json")
    };

    let content = fs::read_to_string(&registry_path)?;
    let mut registry: Value = serde_json::from_str(&content)?;

    let mut patched = false;

    // 1. Try to patch mappings
    if let Some(mappings) = registry["mappings"].as_object_mut() {
        if let Some(mapping) = mappings.get_mut(stat_id) {
            if let Some(sb) = save_bits {
                mapping["save_bits"] = if sb == "null" { Value::Null } else { Value::from(sb.parse::<u32>()?) };
                patched = true;
            }
            if let Some(desc) = description {
                mapping["description"] = Value::from(desc.as_str());
                patched = true;
            }
            if let Some(fh) = fidelity_hint {
                mapping["fidelity_hint"] = Value::from(fh.as_str());
                patched = true;
            }
        }
    }

    // 2. Try to patch stats
    if let Some(stats) = registry["stats"].as_object_mut() {
        if let Some(stat) = stats.get_mut(stat_id) {
            if let Some(w) = width {
                stat["width"] = Value::from(w.parse::<u32>()?);
                patched = true;
            }
            if let Some(desc) = description {
                stat["description"] = Value::from(desc.as_str());
                patched = true;
            }
            if let Some(fh) = fidelity_hint {
                stat["fidelity_hint"] = Value::from(fh.as_str());
                patched = true;
            }
        } else if !patched && (width.is_some() || description.is_some() || fidelity_hint.is_some()) {
            // 3. Add to stats if it doesn't exist and we have relevant fields
            let mut new_stat = serde_json::Map::new();
            new_stat.insert("name".to_string(), Value::from(format!("fuzzed_stat_{}", stat_id)));
            if let Some(w) = width {
                new_stat.insert("width".to_string(), Value::from(w.parse::<u32>()?));
            }
            if let Some(desc) = description {
                new_stat.insert("description".to_string(), Value::from(desc.as_str()));
            }
            if let Some(fh) = fidelity_hint {
                new_stat.insert("fidelity_hint".to_string(), Value::from(fh.as_str()));
            }
            stats.insert(stat_id.to_string(), Value::Object(new_stat));
            patched = true;
        }
    }

    let output_json = serde_json::to_string_pretty(&registry)?;

    if dry_run {
        println!("{}", output_json);
    } else if patched {
        let temp_path = registry_path.with_extension("tmp");
        fs::write(&temp_path, output_json)?;
        fs::rename(&temp_path, &registry_path)?;
        println!(r#"{{"patched": true, "stat_id": "{}", "registry": "{}"}}"#, stat_id, registry_path.display());
    } else {
        println!(r#"{{"patched": false, "reason": "no matches", "stat_id": "{}"}}"#, stat_id);
    }

    Ok(())
}
