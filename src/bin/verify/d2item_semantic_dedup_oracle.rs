// d2item_semantic_dedup_oracle.rs — report-only semantic item identity census oracle
// Standalone verification tool for per-item semantic identity candidate census and ineligible item classification.

use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone)]
struct ScannedItemRecord {
    save_file: String,
    item_code: String,
    item_index: u32,
    physical_digest: String,
    is_ineligible: bool,
    ineligible_reasons: Vec<String>,
    semantic_key: Option<SemanticItemKey>,
    is_opaque: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct PropertyTuple {
    stat_id: u32,
    param: u32,
    raw_value: i32,
    value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SemanticItemKey {
    code: String,
    quality: String,
    magic_prefix: Option<u16>,
    magic_suffix: Option<u16>,
    rare_name_1: Option<u8>,
    rare_name_2: Option<u8>,
    rare_affixes: [Option<u16>; 6],
    unique_id: Option<u16>,
    runeword_id: Option<u16>,
    runeword_level: Option<u8>,
    is_ethereal: bool,
    properties: Vec<PropertyTuple>,
    set_attributes: Vec<Vec<PropertyTuple>>,
    runeword_attributes: Vec<PropertyTuple>,
}

#[derive(Debug, Serialize)]
struct PhysicalIdentitySummary {
    unique_physical_identities: u32,
    physical_duplicate_count: u32,
    digest_collision_distinct_identity_count: u32,
}

#[derive(Debug, Serialize)]
struct SemanticCandidateSummary {
    eligible_item_count: u32,
    ineligible_item_count: u32,
    semantic_class_groups_count: u32,
    distinct_physical_witnesses_count: u32,
    unresolved_opaque_item_count: u32,
}

#[derive(Debug, Serialize)]
struct IneligibleItemInfo {
    save_file: String,
    item_code: String,
    item_index: u32,
    reason: String,
}

#[derive(Debug, Serialize)]
struct SemanticCandidateLocation {
    save_file: String,
    item_code: String,
    item_index: u32,
    physical_digest: String,
}

#[derive(Debug, Serialize)]
struct SemanticCandidateGroupInfo {
    semantic_key_digest: String,
    item_code: String,
    quality: String,
    physical_witness_count: u32,
    items: Vec<SemanticCandidateLocation>,
}

#[derive(Debug, Serialize)]
struct SemanticCensusReport {
    saves_dir: String,
    total_saves_scanned: u32,
    total_items_collected: u32,
    physical_identity_summary: PhysicalIdentitySummary,
    semantic_candidate_summary: SemanticCandidateSummary,
    ineligible_items: Vec<IneligibleItemInfo>,
    semantic_candidate_groups: Vec<SemanticCandidateGroupInfo>,
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

fn compute_physical_digest(total_bits: u64, bits: &[bool]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(total_bits);
    let packed = pack_bits(bits);
    hasher.write(&packed);
    format!("{:016x}", hasher.finish())
}

fn compute_semantic_key_digest(key: &SemanticItemKey) -> String {
    let mut hasher = DefaultHasher::new();
    let json_bytes = serde_json::to_vec(key).unwrap_or_default();
    hasher.write(&json_bytes);
    format!("{:016x}", hasher.finish())
}

fn extract_semantic_key(item: &Item) -> SemanticItemKey {
    let mut properties: Vec<PropertyTuple> = item
        .properties
        .iter()
        .map(|p| PropertyTuple {
            stat_id: p.stat_id,
            param: p.param,
            raw_value: p.raw_value,
            value: p.value,
        })
        .collect();
    properties.sort();

    let set_attributes: Vec<Vec<PropertyTuple>> = item
        .set_attributes
        .iter()
        .map(|list| {
            let mut inner: Vec<PropertyTuple> = list
                .iter()
                .map(|p| PropertyTuple {
                    stat_id: p.stat_id,
                    param: p.param,
                    raw_value: p.raw_value,
                    value: p.value,
                })
                .collect();
            inner.sort();
            inner
        })
        .collect();

    let mut runeword_attributes: Vec<PropertyTuple> = item
        .runeword_attributes
        .iter()
        .map(|p| PropertyTuple {
            stat_id: p.stat_id,
            param: p.param,
            raw_value: p.raw_value,
            value: p.value,
        })
        .collect();
    runeword_attributes.sort();

    let quality_str = item
        .header
        .quality
        .map(|q| format!("{:?}", q))
        .unwrap_or_else(|| "Normal".to_string());

    SemanticItemKey {
        code: item.body.code.trim().to_string(),
        quality: quality_str,
        magic_prefix: item.magic_prefix,
        magic_suffix: item.magic_suffix,
        rare_name_1: item.rare_name_1,
        rare_name_2: item.rare_name_2,
        rare_affixes: item.rare_affixes,
        unique_id: item.unique_id,
        runeword_id: item.runeword_id,
        runeword_level: item.runeword_level,
        is_ethereal: item.header.is_ethereal,
        properties,
        set_attributes,
        runeword_attributes,
    }
}

fn collect_items_recursive(
    item: &Item,
    save_filename: &str,
    item_index: &mut u32,
    records: &mut Vec<ScannedItemRecord>,
    raw_bits_and_total: &mut Vec<(u64, Vec<bool>)>,
) {
    let current_index = *item_index;
    *item_index += 1;

    let bits: Vec<bool> = item.bits.iter().map(|b| b.bit).collect();
    let phys_digest = compute_physical_digest(item.total_bits, &bits);
    raw_bits_and_total.push((item.total_bits, bits));

    let mut reasons = Vec::new();
    if item.header.is_compact {
        reasons.push("is_compact_item".to_string());
    }
    if !item.socketed_items.is_empty() {
        reasons.push("has_socketed_items".to_string());
    }
    if item.body.defense.is_some()
        || item.body.max_durability.is_some()
        || item.defense().is_some()
        || item.max_durability().is_some()
    {
        reasons.push("has_defense_or_max_durability".to_string());
    }
    let is_opaque = item.is_opaque() || item.is_semi_opaque();
    if is_opaque {
        reasons.push("opaque_or_semi_opaque".to_string());
    }

    let is_ineligible = !reasons.is_empty();
    let semantic_key = if !is_ineligible {
        Some(extract_semantic_key(item))
    } else {
        None
    };

    records.push(ScannedItemRecord {
        save_file: save_filename.to_string(),
        item_code: item.body.code.trim().to_string(),
        item_index: current_index,
        physical_digest: phys_digest,
        is_ineligible,
        ineligible_reasons: reasons,
        semantic_key,
        is_opaque,
    });

    for socketed in &item.socketed_items {
        collect_items_recursive(
            socketed,
            save_filename,
            item_index,
            records,
            raw_bits_and_total,
        );
    }
}

fn main() {
    let mut parser = ArgParser::new("d2item_semantic_dedup_oracle").description(
        "Report-only oracle scanning save files for semantic item identity candidate census",
    );

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_flag(
            "json",
            "Emit census report in JSON format to stdout and agent_artifacts/d2item_semantic_dedup_census.json",
        )
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
    let mut all_records: Vec<ScannedItemRecord> = Vec::new();
    let mut raw_bits_and_total: Vec<(u64, Vec<bool>)> = Vec::new();
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
                collect_items_recursive(
                    top_item,
                    file_name,
                    &mut item_counter,
                    &mut all_records,
                    &mut raw_bits_and_total,
                );
            }
        }
    }

    let total_items_collected = all_records.len() as u32;

    // Physical identity summary computation
    let mut phys_digest_map: HashMap<String, Vec<(u64, Vec<bool>)>> = HashMap::new();
    for (total_bits, bits) in &raw_bits_and_total {
        let digest = compute_physical_digest(*total_bits, bits);
        phys_digest_map
            .entry(digest)
            .or_default()
            .push((*total_bits, bits.clone()));
    }

    let mut unique_physical_identities = 0u32;
    let mut physical_duplicate_count = 0u32;
    let mut digest_collision_distinct_identity_count = 0u32;

    for (_digest, items) in phys_digest_map {
        let mut sub_groups: Vec<(u64, Vec<bool>, usize)> = Vec::new();
        for (tb, b) in items {
            if let Some(sub) = sub_groups.iter_mut().find(|(t, bits, _)| *t == tb && *bits == b) {
                sub.2 += 1;
            } else {
                sub_groups.push((tb, b, 1));
            }
        }

        unique_physical_identities += sub_groups.len() as u32;
        if sub_groups.len() > 1 {
            digest_collision_distinct_identity_count += (sub_groups.len() - 1) as u32;
        }
        for (_, _, count) in sub_groups {
            if count > 1 {
                physical_duplicate_count += count as u32;
            }
        }
    }

    let physical_identity_summary = PhysicalIdentitySummary {
        unique_physical_identities,
        physical_duplicate_count,
        digest_collision_distinct_identity_count,
    };

    // Semantic candidate & ineligible items partition
    let mut ineligible_items: Vec<IneligibleItemInfo> = Vec::new();
    let mut semantic_groups_map: HashMap<String, (SemanticItemKey, Vec<SemanticCandidateLocation>)> =
        HashMap::new();
    let mut eligible_witness_digests: HashSet<String> = HashSet::new();
    let mut unresolved_opaque_item_count = 0u32;

    for rec in all_records {
        if rec.is_opaque {
            unresolved_opaque_item_count += 1;
        }

        if rec.is_ineligible {
            ineligible_items.push(IneligibleItemInfo {
                save_file: rec.save_file,
                item_code: rec.item_code,
                item_index: rec.item_index,
                reason: rec.ineligible_reasons.join(", "),
            });
        } else if let Some(key) = rec.semantic_key {
            eligible_witness_digests.insert(rec.physical_digest.clone());
            let key_digest = compute_semantic_key_digest(&key);
            let location = SemanticCandidateLocation {
                save_file: rec.save_file,
                item_code: rec.item_code,
                item_index: rec.item_index,
                physical_digest: rec.physical_digest,
            };

            semantic_groups_map
                .entry(key_digest)
                .or_insert_with(|| (key, Vec::new()))
                .1
                .push(location);
        }
    }

    let eligible_item_count = semantic_groups_map
        .values()
        .map(|(_, locs)| locs.len() as u32)
        .sum();
    let ineligible_item_count = ineligible_items.len() as u32;

    let mut semantic_candidate_groups: Vec<SemanticCandidateGroupInfo> = Vec::new();
    for (digest, (key, items)) in semantic_groups_map {
        semantic_candidate_groups.push(SemanticCandidateGroupInfo {
            semantic_key_digest: digest,
            item_code: key.code,
            quality: key.quality,
            physical_witness_count: items.len() as u32,
            items,
        });
    }

    semantic_candidate_groups.sort_by(|a, b| {
        b.physical_witness_count
            .cmp(&a.physical_witness_count)
            .then_with(|| a.semantic_key_digest.cmp(&b.semantic_key_digest))
    });

    let semantic_candidate_summary = SemanticCandidateSummary {
        eligible_item_count,
        ineligible_item_count,
        semantic_class_groups_count: semantic_candidate_groups.len() as u32,
        distinct_physical_witnesses_count: eligible_witness_digests.len() as u32,
        unresolved_opaque_item_count,
    };

    let report = SemanticCensusReport {
        saves_dir: saves_dir_str,
        total_saves_scanned,
        total_items_collected,
        physical_identity_summary,
        semantic_candidate_summary,
        ineligible_items,
        semantic_candidate_groups,
    };

    let json_output =
        serde_json::to_string_pretty(&report).expect("Failed to serialize semantic census report");

    let artifact_paths = [
        PathBuf::from("agent_artifacts/d2item_semantic_dedup_census.json"),
        PathBuf::from("../d2r-spec/agent_artifacts/d2item_semantic_dedup_census.json"),
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
        println!("=== Report-Only Semantic Item Identity Census Summary ===");
        println!("Saves Scanned:                        {}", report.total_saves_scanned);
        println!("Total Items Collected:               {}", report.total_items_collected);
        println!("Eligible Candidate Items:            {}", report.semantic_candidate_summary.eligible_item_count);
        println!("Ineligible Items:                    {}", report.semantic_candidate_summary.ineligible_item_count);
        println!("Semantic Class Groups:               {}", report.semantic_candidate_summary.semantic_class_groups_count);
        println!("Distinct Physical Witness Digests:   {}", report.semantic_candidate_summary.distinct_physical_witnesses_count);
        println!("Unresolved / Opaque Items:           {}", report.semantic_candidate_summary.unresolved_opaque_item_count);
    }
}
