use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};

#[derive(Debug, Deserialize)]
struct Recommendation {
    stat_id: u32,
    recommended_width: Option<u32>,
    #[serde(alias = "recommended_action")]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecommendationReport {
    recommendations: Vec<Recommendation>,
}

fn main() {
    let mut parser = ArgParser::new("d2item_registry_patcher");
    parser.add_spec(ArgSpec::option("input", Some('i'), Some("input"), "Path to recommendations JSON"));
    parser.add_spec(ArgSpec::option("registry", None, Some("registry"), "Path to alpha_v105_forensics.json"));
    parser.add_spec(ArgSpec::flag("dry-run", None, Some("dry-run"), "Show changes without writing"));

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

    let input_path = parsed.get("input").map(PathBuf::from).expect("Input path required via --input");
    let registry_path = if let Some(reg) = parsed.get("registry") {
        PathBuf::from(reg)
    } else {
        let base_path = std::env::var("D2R_DATA_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("../d2r-data"));
        base_path.join("constants/alpha_v105_forensics.json")
    };
    let dry_run = parsed.is_set("dry-run");

    println!("--- Alpha v105 Registry Patcher ---");
    println!("Input: {:?}", input_path);
    println!("Registry: {:?}", registry_path);
    if dry_run {
        println!("Mode: DRY RUN (no files will be modified)");
    }

    // Read recommendations
    let rec_content = fs::read_to_string(&input_path).expect("Failed to read input recommendations JSON");
    let report: RecommendationReport = serde_json::from_str(&rec_content).expect("Failed to parse recommendations JSON");

    // Read registry
    let reg_content = fs::read_to_string(&registry_path).expect("Failed to read forensic registry JSON");
    let mut registry: Value = serde_json::from_str(&reg_content).expect("Failed to parse forensic registry JSON");

    let mut patch_count = 0;

    // 1. Update stats section
    if let Some(stats) = registry.get_mut("stats").and_then(|v| v.as_object_mut()) {
        for rec in &report.recommendations {
            let target_width = match rec.recommended_width {
                Some(w) => w,
                None => continue,
            };
            let key = rec.stat_id.to_string();
            if let Some(stat_val) = stats.get_mut(&key) {
                if let Some(width_val) = stat_val.get_mut("width") {
                    let old_width = width_val.as_u64().unwrap_or(0);
                    if old_width != target_width as u64 {
                        println!("  [PATCH] stats.{}: width {} -> {} ({:?})", key, old_width, target_width, rec.reason);
                        *width_val = Value::from(target_width);
                        patch_count += 1;
                    }
                }
            }
        }
    }

    // 2. Update mappings section
    if let Some(mappings) = registry.get_mut("mappings").and_then(|v| v.as_object_mut()) {
        for rec in &report.recommendations {
            let target_width = match rec.recommended_width {
                Some(w) => w,
                None => continue,
            };
            let key = rec.stat_id.to_string();
            if let Some(map_val) = mappings.get_mut(&key) {
                if let Some(bits_val) = map_val.get_mut("save_bits") {
                    let old_bits = bits_val.as_u64();
                    if old_bits != Some(target_width as u64) {
                        println!("  [PATCH] mappings.{}: save_bits {:?} -> {} ({:?})", key, old_bits, target_width, rec.reason);
                        *bits_val = Value::from(target_width);
                        patch_count += 1;
                    }
                } else if let Some(obj) = map_val.as_object_mut() {
                    if obj.get("save_bits").is_none() {
                        println!("  [PATCH] mappings.{}: setting save_bits to {} ({:?})", key, target_width, rec.reason);
                        obj.insert("save_bits".to_string(), Value::from(target_width));
                        patch_count += 1;
                    }
                }
            }
        }
    }

    if patch_count == 0 {
        println!("\nNo applicable recommendations found or registry already up-to-date.");
        return;
    }

    if dry_run {
        println!("\nDry-run complete: {} potential patches identified.", patch_count);
    } else {
        // Create backup
        let backup_path = registry_path.with_extension("json.bak");
        fs::copy(&registry_path, &backup_path).expect("Failed to create registry backup (*.bak)");
        
        // Serialize with pretty printer (2-space indent as per original)
        let mut formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut ser = serde_json::Serializer::with_formatter(Vec::new(), formatter);
        registry.serialize(&mut ser).expect("Failed to serialize patched registry");
        let out_content = String::from_utf8(ser.into_inner()).expect("Serialized content is not valid UTF-8");
        
        fs::write(&registry_path, out_content).expect("Failed to write patched registry to disk");
        println!("\nSuccessfully patched {} entries in registry.", patch_count);
        println!("Backup saved to: {:?}", backup_path);
    }
}
