// d2item_cache_parallel_benchmark.rs — multi-threaded ItemClassRegistry reuse benchmark harness
// Standalone verification tool for measuring ConcurrentItemClassRegistry deduplication and thread performance on save corpus.

use d2r_core::item::{ConcurrentItemClassRegistry, HuffmanTree, Item, ItemClassProjection};
use d2r_core::verify::args::{ArgError, ArgParser};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

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
struct CacheParallelBenchmarkReport {
    saves_dir: String,
    worker_threads: u32,
    total_saves_scanned: u32,
    total_items_collected: u32,
    eligible_item_count: u32,
    ineligible_item_count: u32,
    unique_class_entries: usize,
    cache_hits: u32,
    cache_hit_ratio_percent: f64,
    collision_count: u64,
    elapsed_ms: u128,
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
    let mut parser = ArgParser::new("d2item_cache_parallel_benchmark")
        .description("Report-only harness measuring ConcurrentItemClassRegistry reuse on save corpus across multiple worker threads");

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_opt("threads", "Number of worker threads to use")
        .short('t')
        .long("threads")
        .default("4");

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
    let threads_str = parsed
        .get("threads")
        .cloned()
        .unwrap_or_else(|| "4".to_string());
    let worker_threads: u32 = threads_str.parse().unwrap_or(4);

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

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_threads as usize)
        .build()
        .ok();

    let start_time = Instant::now();
    let total_saves_scanned = save_files.len() as u32;

    let scan_action = || {
        save_files
            .par_iter()
            .map(|save_path| {
                let huffman = HuffmanTree::new();
                let mut items = Vec::new();
                let bytes = match fs::read(save_path) {
                    Ok(b) => b,
                    Err(_) => return items,
                };
                if bytes.len() < 8 {
                    return items;
                }
                let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
                let is_alpha = version == 105;
                if let Ok(top_items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
                    for top_item in &top_items {
                        collect_items_recursive(top_item, &mut items);
                    }
                }
                items
            })
            .flatten()
            .collect::<Vec<Item>>()
    };

    let all_items = if let Some(ref p) = pool {
        p.install(scan_action)
    } else {
        scan_action()
    };

    let total_items_collected = all_items.len() as u32;
    let registry = ConcurrentItemClassRegistry::new();
    let eligible_item_count = AtomicU32::new(0);
    let ineligible_item_count = AtomicU32::new(0);
    let cache_hits = AtomicU32::new(0);

    let ineligible_compact = AtomicU32::new(0);
    let ineligible_opaque = AtomicU32::new(0);
    let ineligible_socketed = AtomicU32::new(0);
    let ineligible_defense = AtomicU32::new(0);
    let ineligible_durability = AtomicU32::new(0);

    let code_quality_map: Mutex<BTreeMap<(String, u8), u32>> = Mutex::new(BTreeMap::new());

    let process_action = || {
        all_items.par_iter().for_each(|item| {
            if let Some(projection) = ItemClassProjection::extract(item) {
                eligible_item_count.fetch_add(1, Ordering::Relaxed);
                {
                    let mut guard = code_quality_map.lock().unwrap();
                    *guard
                        .entry((projection.code.clone(), projection.quality))
                        .or_default() += 1;
                }

                let len_before = registry.len();
                registry.get_or_insert(projection);
                let len_after = registry.len();

                if len_before == len_after {
                    cache_hits.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                ineligible_item_count.fetch_add(1, Ordering::Relaxed);
                if item.header.is_compact {
                    ineligible_compact.fetch_add(1, Ordering::Relaxed);
                }
                if item.is_opaque() || item.is_semi_opaque() || item.is_residue() {
                    ineligible_opaque.fetch_add(1, Ordering::Relaxed);
                }
                if !item.socketed_items.is_empty() {
                    ineligible_socketed.fetch_add(1, Ordering::Relaxed);
                }
                if item.defense().is_some() {
                    ineligible_defense.fetch_add(1, Ordering::Relaxed);
                }
                if item.max_durability().is_some() {
                    ineligible_durability.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    };

    if let Some(ref p) = pool {
        p.install(process_action);
    } else {
        process_action();
    }

    let elapsed_ms = start_time.elapsed().as_millis();
    let eligible_count = eligible_item_count.load(Ordering::Relaxed);
    let ineligible_count = ineligible_item_count.load(Ordering::Relaxed);
    let hits_count = cache_hits.load(Ordering::Relaxed);
    let unique_class_entries = registry.len();
    let collision_count = registry.collision_count();

    let cache_hit_ratio_percent = if eligible_count > 0 {
        (hits_count as f64 / eligible_count as f64) * 100.0
    } else {
        0.0
    };

    let ineligible_reason_counts = if is_detailed_requested {
        Some(IneligibleReasonCounts {
            compact: ineligible_compact.load(Ordering::Relaxed),
            opaque_semi_opaque_or_residue: ineligible_opaque.load(Ordering::Relaxed),
            has_socketed_children: ineligible_socketed.load(Ordering::Relaxed),
            has_defense: ineligible_defense.load(Ordering::Relaxed),
            has_max_durability: ineligible_durability.load(Ordering::Relaxed),
        })
    } else {
        None
    };

    let eligible_code_quality_distribution = if is_detailed_requested {
        let guard = code_quality_map.into_inner().unwrap();
        Some(
            guard
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

    let report = CacheParallelBenchmarkReport {
        saves_dir: saves_dir_str,
        worker_threads,
        total_saves_scanned,
        total_items_collected,
        eligible_item_count: eligible_count,
        ineligible_item_count: ineligible_count,
        unique_class_entries,
        cache_hits: hits_count,
        cache_hit_ratio_percent,
        collision_count,
        elapsed_ms,
        ineligible_reason_counts,
        eligible_code_quality_distribution,
    };

    let json_output = serde_json::to_string_pretty(&report)
        .expect("Failed to serialize parallel cache benchmark report");

    let artifact_paths = [
        PathBuf::from("agent_artifacts/d2item_cache_parallel_benchmark.json"),
        PathBuf::from("../d2r-spec/agent_artifacts/d2item_cache_parallel_benchmark.json"),
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
        println!("=== ConcurrentItemClassRegistry Multi-Thread Cache Benchmark Summary ===");
        println!("Worker Threads:           {}", report.worker_threads);
        println!("Saves Scanned:            {}", report.total_saves_scanned);
        println!("Total Items Collected:   {}", report.total_items_collected);
        println!("Eligible Candidate Items:{}", report.eligible_item_count);
        println!("Ineligible Items:        {}", report.ineligible_item_count);
        println!("Unique Class Entries:    {}", report.unique_class_entries);
        println!("Cache Hits:              {}", report.cache_hits);
        println!("Cache Hit Ratio (%):     {:.2}%", report.cache_hit_ratio_percent);
        println!("Hash Collision Count:    {}", report.collision_count);
        println!("Elapsed Time (ms):       {} ms", report.elapsed_ms);

        if is_detailed_requested {
            if let Some(reasons) = &report.ineligible_reason_counts {
                println!("\n--- Ineligible Item Reason Counts (Overlap Permitted) ---");
                println!("  Compact:                         {}", reasons.compact);
                println!(
                    "  Opaque / Semi-Opaque / Residue:  {}",
                    reasons.opaque_semi_opaque_or_residue
                );
                println!("  Has Socketed Children:          {}", reasons.has_socketed_children);
                println!("  Has Defense:                     {}", reasons.has_defense);
                println!("  Has Max Durability:              {}", reasons.has_max_durability);
            }

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
