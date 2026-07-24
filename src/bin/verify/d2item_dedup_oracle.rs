// d2item_dedup_oracle.rs — report-only physical item identity census oracle
// Standalone verification tool for per-item physical identity census and duplicate distribution analysis.

use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::Serialize;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone)]
struct ItemRecord {
    save_file: String,
    item_code: String,
    item_index: u32,
    is_socketed: bool,
    total_bits: u64,
    bits: Vec<bool>,
}

#[derive(Debug, Serialize)]
struct SkippedItemInfo {
    save_file: String,
    item_code: String,
    item_index: u32,
    reason: String,
}

#[derive(Debug, Serialize)]
struct DuplicateItemLocation {
    save_file: String,
    item_code: String,
    item_index: u32,
    is_socketed: bool,
}

#[derive(Debug, Serialize)]
struct DuplicateGroupInfo {
    digest_hex: String,
    count: u32,
    total_bits: u64,
    bit_count: usize,
    items: Vec<DuplicateItemLocation>,
}

#[derive(Debug, Serialize)]
struct CensusReport {
    saves_dir: String,
    total_saves_scanned: u32,
    total_items_collected: u32,
    unreadable_skipped_count: u32,
    unique_physical_identities: u32,
    physical_duplicate_count: u32,
    digest_collision_distinct_identity_count: u32,
    skipped_items: Vec<SkippedItemInfo>,
    duplicate_groups: Vec<DuplicateGroupInfo>,
}

fn pack_bits(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((bits.len() + 7) / 8);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &b) in chunk.iter().enumerate() {
            if b {
                byte |= 1 << i;
            }
        }
        bytes.push(byte);
    }
    bytes
}

fn compute_digest(total_bits: u64, bits: &[bool]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(total_bits);
    let packed = pack_bits(bits);
    hasher.write(&packed);
    format!("{:016x}", hasher.finish())
}

fn collect_items_recursive(
    item: &Item,
    save_filename: &str,
    item_index: &mut u32,
    is_socketed: bool,
    records: &mut Vec<ItemRecord>,
) {
    let current_index = *item_index;
    *item_index += 1;

    records.push(ItemRecord {
        save_file: save_filename.to_string(),
        item_code: item.code.trim().to_string(),
        item_index: current_index,
        is_socketed,
        total_bits: item.total_bits,
        bits: item.bits.iter().map(|b| b.bit).collect(),
    });

    for socketed in &item.socketed_items {
        collect_items_recursive(socketed, save_filename, item_index, true, records);
    }
}

fn main() {
    let mut parser = ArgParser::new("d2item_dedup_oracle")
        .description("Report-only oracle scanning save files for physical item identity deduplication census");

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_flag("json", "Emit census report in JSON format to stdout and agent_artifacts/d2item_dedup_census.json")
        .short('j')
        .long("json");

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

    let saves_dir_path = Path::new(&saves_dir_str);
    if !saves_dir_path.exists() || !saves_dir_path.is_dir() {
        eprintln!("[ERROR] Saves directory '{}' does not exist or is not a directory.", saves_dir_str);
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
    let mut all_records: Vec<ItemRecord> = Vec::new();
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
            let mut item_counter = 0u32;
            for top_item in &top_items {
                collect_items_recursive(top_item, file_name, &mut item_counter, false, &mut all_records);
            }
        }
    }

    let total_items_collected = all_records.len() as u32;
    let mut skipped_items: Vec<SkippedItemInfo> = Vec::new();

    // Group items by digest
    let mut digest_groups: HashMap<String, Vec<ItemRecord>> = HashMap::new();

    for record in all_records {
        if record.bits.is_empty() || record.total_bits == 0 {
            let reason = if record.bits.is_empty() && record.total_bits == 0 {
                "Empty bits vector and zero total_bits".to_string()
            } else if record.bits.is_empty() {
                "Empty bits vector".to_string()
            } else {
                "Zero total_bits".to_string()
            };

            skipped_items.push(SkippedItemInfo {
                save_file: record.save_file,
                item_code: record.item_code,
                item_index: record.item_index,
                reason,
            });
            continue;
        }

        let digest = compute_digest(record.total_bits, &record.bits);
        digest_groups.entry(digest).or_default().push(record);
    }

    let unreadable_skipped_count = skipped_items.len() as u32;

    let mut duplicate_groups: Vec<DuplicateGroupInfo> = Vec::new();
    let mut physical_duplicate_count = 0u32;
    let mut digest_collision_distinct_identity_count = 0u32;
    let mut unique_physical_identities = 0u32;

    for (digest, group_records) in digest_groups {
        // Sub-group by exact identity tuple (total_bits, bits) to verify digest collisions
        let mut identity_subgroups: Vec<(u64, Vec<bool>, Vec<&ItemRecord>)> = Vec::new();

        for rec in &group_records {
            if let Some(sub) = identity_subgroups
                .iter_mut()
                .find(|(tb, b, _)| *tb == rec.total_bits && *b == rec.bits)
            {
                sub.2.push(rec);
            } else {
                identity_subgroups.push((rec.total_bits, rec.bits.clone(), vec![rec]));
            }
        }

        unique_physical_identities += identity_subgroups.len() as u32;

        if identity_subgroups.len() > 1 {
            // Multiple distinct identities hashed to the same digest -> collision detected
            digest_collision_distinct_identity_count += (identity_subgroups.len() - 1) as u32;
        }

        for (total_bits, bits, recs) in identity_subgroups {
            if recs.len() > 1 {
                physical_duplicate_count += recs.len() as u32;

                let locations: Vec<DuplicateItemLocation> = recs
                    .iter()
                    .map(|r| DuplicateItemLocation {
                        save_file: r.save_file.clone(),
                        item_code: r.item_code.clone(),
                        item_index: r.item_index,
                        is_socketed: r.is_socketed,
                    })
                    .collect();

                duplicate_groups.push(DuplicateGroupInfo {
                    digest_hex: digest.clone(),
                    count: recs.len() as u32,
                    total_bits,
                    bit_count: bits.len(),
                    items: locations,
                });
            }
        }
    }

    duplicate_groups.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.digest_hex.cmp(&b.digest_hex)));

    let report = CensusReport {
        saves_dir: saves_dir_str,
        total_saves_scanned,
        total_items_collected,
        unreadable_skipped_count,
        unique_physical_identities,
        physical_duplicate_count,
        digest_collision_distinct_identity_count,
        skipped_items,
        duplicate_groups,
    };

    let json_output = serde_json::to_string_pretty(&report).expect("Failed to serialize census report");

    // Output to agent_artifacts/d2item_dedup_census.json
    let artifact_paths = [
        PathBuf::from("agent_artifacts/d2item_dedup_census.json"),
        PathBuf::from("../d2r-spec/agent_artifacts/d2item_dedup_census.json"),
    ];

    for artifact_path in &artifact_paths {
        if let Some(parent) = artifact_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(artifact_path, &json_output) {
            eprintln!("[WARNING] Could not write to artifact path '{:?}': {}", artifact_path, e);
        }
    }

    if is_json_requested {
        println!("{}", json_output);
    } else {
        println!("=== Physical Item Identity Census Summary ===");
        println!("Saves Scanned:                   {}", report.total_saves_scanned);
        println!("Total Items Collected:          {}", report.total_items_collected);
        println!("Unreadable / Skipped Items:     {}", report.unreadable_skipped_count);
        println!("Unique Physical Identities:     {}", report.unique_physical_identities);
        println!("Items Sharing Duplicate Identity:{}", report.physical_duplicate_count);
        println!("Digest Collision Count:         {}", report.digest_collision_distinct_identity_count);
        println!("Duplicate Groups Found:         {}", report.duplicate_groups.len());
    }
}
