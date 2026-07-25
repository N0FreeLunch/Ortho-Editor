// d2item_cache_benchmark.rs — report-only ItemClassRegistry reuse benchmark harness
// Standalone verification tool for measuring ItemClassRegistry deduplication metrics on save corpus.

use d2r_core::item::{HuffmanTree, Item, ItemClassProjection, ItemClassRegistry};
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Serialize)]
struct IneligibleReasonCounts {
    compact: u32,
    opaque_semi_opaque_or_residue: u32,
    has_socketed_children: u32,
    has_defense: u32,
    has_max_durability: u32,
}

#[derive(Debug, Serialize)]
struct EligibleCodeQualityCount {
    code: String,
    quality: u8,
    instance_count: u32,
}

#[derive(Debug, Serialize)]
struct CacheBenchmarkReport {
    saves_dir: String,
    total_saves_scanned: u32,
    total_items_collected: u32,
    eligible_item_count: u32,
    ineligible_item_count: u32,
    unique_class_entries: usize,
    cache_hits: u32,
    cache_hit_ratio_percent: f64,
    collision_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    ineligible_reason_counts: Option<IneligibleReasonCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eligible_code_quality_distribution: Option<Vec<EligibleCodeQualityCount>>,
}

fn collect_items_recursive(item: &Item, items: &mut Vec<Item>) {
    items.push(item.clone());
    for socketed in &item.socketed_items {
        collect_items_recursive(socketed, items);
    }
}

fn main() {
    let mut parser = ArgParser::new("d2item_cache_benchmark")
        .description("Report-only harness measuring ItemClassRegistry reuse on save corpus");

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_flag(
            "json",
            "Emit cache benchmark report in JSON format to stdout and artifact paths",
        )
        .short('j')
        .long("json");

    parser
        .add_flag(
            "detailed",
            "Include detailed breakdowns of ineligible item reasons and eligible code/quality distribution",
        )
        .short('D')
        .long("detailed");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let saves_dir_str = parsed
        .get("saves-dir")
        .cloned()
        .unwrap_or_else(|| "tests/fixtures/savegames/modified".to_string());
    let is_json_requested = parsed.is_set("json");
    let is_detailed_requested = parsed.is_set("detailed");

    let saves_dir_path = Path::new(&saves_dir_str);
    if !saves_dir_path.exists() || !saves_dir_path.is_dir() {
        eprintln!(
            "[ERROR] Saves directory '{}' does not exist or is not a directory.",
            saves_dir_str
        );
        process::exit(1);
    }

    let mut save_files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(saves_dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("d2s") {
                save_files.push(path);
            }
        }
    }
    save_files.sort();

    let huffman = HuffmanTree::new();
    let mut all_items: Vec<Item> = Vec::new();
    let total_saves_scanned = save_files.len() as u32;

    for save_path in &save_files {
        let file_name = save_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown.d2s");

        let bytes = match fs::read(save_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[WARNING] Failed to read save file '{}': {}", file_name, e);
                continue;
            }
        };

        if bytes.len() < 8 {
            continue;
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105;

        if let Ok(top_items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
            for top_item in &top_items {
                collect_items_recursive(top_item, &mut all_items);
            }
        }
    }

    let total_items_collected = all_items.len() as u32;
    let mut registry = ItemClassRegistry::new();
    let mut eligible_item_count = 0u32;
    let mut ineligible_item_count = 0u32;
    let mut cache_hits = 0u32;

    let mut ineligible_compact = 0u32;
    let mut ineligible_opaque_semi_opaque_or_residue = 0u32;
    let mut ineligible_has_socketed_children = 0u32;
    let mut ineligible_has_defense = 0u32;
    let mut ineligible_has_max_durability = 0u32;

    let mut eligible_code_quality_map: BTreeMap<(String, u8), u32> = BTreeMap::new();

    for item in &all_items {
        if let Some(projection) = ItemClassProjection::extract(item) {
            eligible_item_count += 1;
            *eligible_code_quality_map
                .entry((projection.code.clone(), projection.quality))
                .or_default() += 1;

            let len_before = registry.len();
            registry.get_or_insert(projection);
            let len_after = registry.len();

            if len_before == len_after {
                cache_hits += 1;
            }
        } else {
            ineligible_item_count += 1;
            if item.header.is_compact {
                ineligible_compact += 1;
            }
            if item.is_opaque() || item.is_semi_opaque() || item.is_residue() {
                ineligible_opaque_semi_opaque_or_residue += 1;
            }
            if !item.socketed_items.is_empty() {
                ineligible_has_socketed_children += 1;
            }
            if item.defense().is_some() {
                ineligible_has_defense += 1;
            }
            if item.max_durability().is_some() {
                ineligible_has_max_durability += 1;
            }
        }
    }

    let unique_class_entries = registry.len();
    let collision_count = registry.collision_count();
    let cache_hit_ratio_percent = if eligible_item_count > 0 {
        (cache_hits as f64 / eligible_item_count as f64) * 100.0
    } else {
        0.0
    };

    let ineligible_reason_counts = if is_detailed_requested {
        Some(IneligibleReasonCounts {
            compact: ineligible_compact,
            opaque_semi_opaque_or_residue: ineligible_opaque_semi_opaque_or_residue,
            has_socketed_children: ineligible_has_socketed_children,
            has_defense: ineligible_has_defense,
            has_max_durability: ineligible_has_max_durability,
        })
    } else {
        None
    };

    let eligible_code_quality_distribution = if is_detailed_requested {
        Some(
            eligible_code_quality_map
                .into_iter()
                .map(|((code, quality), instance_count)| EligibleCodeQualityCount {
                    code,
                    quality,
                    instance_count,
                })
                .collect(),
        )
    } else {
        None
    };

    let report = CacheBenchmarkReport {
        saves_dir: saves_dir_str,
        total_saves_scanned,
        total_items_collected,
        eligible_item_count,
        ineligible_item_count,
        unique_class_entries,
        cache_hits,
        cache_hit_ratio_percent,
        collision_count,
        ineligible_reason_counts,
        eligible_code_quality_distribution,
    };

    let json_output =
        serde_json::to_string_pretty(&report).expect("Failed to serialize cache benchmark report");

    let artifact_paths = [
        PathBuf::from("agent_artifacts/d2item_cache_benchmark.json"),
        PathBuf::from("../d2r-spec/agent_artifacts/d2item_cache_benchmark.json"),
    ];

    for artifact_path in &artifact_paths {
        if let Some(parent) = artifact_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(artifact_path, &json_output) {
            eprintln!(
                "[WARNING] Could not write to artifact path '{:?}': {}",
                artifact_path, e
            );
        }
    }

    if is_json_requested {
        println!("{}", json_output);
    } else {
        println!("=== Report-Only ItemClassRegistry Cache Benchmark Summary ===");
        println!("Saves Scanned:            {}", report.total_saves_scanned);
        println!("Total Items Collected:   {}", report.total_items_collected);
        println!("Eligible Candidate Items:{}", report.eligible_item_count);
        println!("Ineligible Items:        {}", report.ineligible_item_count);
        println!("Unique Class Entries:    {}", report.unique_class_entries);
        println!("Cache Hits:              {}", report.cache_hits);
        println!("Cache Hit Ratio (%):     {:.2}%", report.cache_hit_ratio_percent);
        println!("Hash Collision Count:    {}", report.collision_count);

        if is_detailed_requested {
            println!("\n--- Ineligible Item Reason Counts (Overlap Permitted) ---");
            println!("  Compact:                         {}", ineligible_compact);
            println!(
                "  Opaque / Semi-Opaque / Residue:  {}",
                ineligible_opaque_semi_opaque_or_residue
            );
            println!("  Has Socketed Children:          {}", ineligible_has_socketed_children);
            println!("  Has Defense:                     {}", ineligible_has_defense);
            println!("  Has Max Durability:              {}", ineligible_has_max_durability);

            if let Some(dist) = &report.eligible_code_quality_distribution {
                println!("\n--- Eligible Item Code/Quality Distribution ---");
                for entry in dist {
                    println!(
                        "  Code: {:<4} | Quality: {:<2} | Count: {}",
                        entry.code, entry.quality, entry.instance_count
                    );
                }
            }
        }
    }
}

