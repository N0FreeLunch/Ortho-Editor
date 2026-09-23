use d2r_core::domain::progression::Progression;
use d2r_core::item::{HuffmanTree, Item, ItemQuality};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Serialize)]
struct ItemGroup {
    key: String,
    count: usize,
    template_candidate: bool,
    template_candidate_basis: &'static str,
    evidence_grade: &'static str,
}

#[derive(Serialize)]
struct SaveNode {
    path: String,
    progression_supported: bool,
    completed_quests: Vec<String>,
    active_waypoints: Vec<String>,
    item_groups: Vec<ItemGroup>,
}

#[derive(Serialize)]
struct GroupDelta {
    key: String,
    classification: &'static str,
    before_count: usize,
    after_count: usize,
    template_candidate: bool,
    template_candidate_basis: &'static str,
    evidence_grade: &'static str,
}

#[derive(Serialize)]
struct TemporalEdge {
    before: String,
    after: String,
    relation: &'static str,
    item_deltas: Vec<GroupDelta>,
    hint: String,
}

#[derive(Serialize)]
struct TemporalWitnessPayload {
    nodes: Vec<SaveNode>,
    edges: Vec<TemporalEdge>,
    hint: String,
}

fn collect_saves(dir: &Path, saves: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_saves(&path, saves)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("d2s") {
            saves.push(path);
        }
    }
    Ok(())
}

fn quality_name(item: &Item) -> String {
    if item.header.is_runeword {
        "Runeword".to_string()
    } else {
        format!("{:?}", item.header.quality)
    }
}

fn template_candidate_basis(item: &Item) -> &'static str {
    if item.header.is_runeword {
        "runeword_header_flag"
    } else {
        match item.header.quality {
            Some(ItemQuality::Unique) => "unique_quality",
            Some(ItemQuality::Set) => "set_quality",
            _ => "none",
        }
    }
}

fn node_from_path(path: &Path) -> Result<(SaveNode, BTreeMap<String, ItemGroup>), String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let is_alpha = bytes.len() >= 8 && u32::from_le_bytes(bytes[4..8].try_into().unwrap()) == 105;
    let mut quests = BTreeSet::new();
    let mut waypoints = BTreeSet::new();
    if is_alpha {
        if let Ok(progression) = Progression::from_bytes(&bytes, true).value {
            for quest in progression
                .quests
                .quests()
                .iter()
                .filter(|quest| quest.is_completed())
            {
                quests.insert(format!("a{}q{}", quest.act(), quest.index() + 1));
            }
            for waypoint in progression
                .waypoints
                .waypoints()
                .iter()
                .filter(|waypoint| waypoint.is_active())
            {
                waypoints.insert(format!("a{}w{}", waypoint.act(), waypoint.index() + 1));
            }
        }
    }

    let huffman = HuffmanTree::new();
    let mut groups = BTreeMap::new();
    if let Ok(items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
        for item in items
            .iter()
            .filter(|item| !item.is_residue() && !item.code.trim().is_empty())
        {
            let quality = quality_name(item);
            let template_candidate_basis = template_candidate_basis(item);
            let template_candidate = template_candidate_basis != "none";
            let key = format!(
                "{}|{}|mode={}|location={}",
                item.code.trim(),
                quality,
                item.mode,
                item.location
            );
            let evidence_grade = if item.mode == 1 {
                "parser_suspect"
            } else {
                "provisional"
            };
            groups
                .entry(key.clone())
                .and_modify(|group: &mut ItemGroup| group.count += 1)
                .or_insert(ItemGroup {
                    key,
                    count: 1,
                    template_candidate,
                    template_candidate_basis,
                    evidence_grade,
                });
        }
    }
    let path = path.to_string_lossy().replace('\\', "/");
    let node = SaveNode {
        path,
        progression_supported: is_alpha,
        completed_quests: quests.into_iter().collect(),
        active_waypoints: waypoints.into_iter().collect(),
        item_groups: groups
            .values()
            .map(|group| ItemGroup {
                key: group.key.clone(),
                count: group.count,
                template_candidate: group.template_candidate,
                template_candidate_basis: group.template_candidate_basis,
                evidence_grade: group.evidence_grade,
            })
            .collect(),
    };
    Ok((node, groups))
}

fn is_strict_subset(before: &[String], after: &[String]) -> bool {
    let before: BTreeSet<_> = before.iter().collect();
    let after: BTreeSet<_> = after.iter().collect();
    before.is_subset(&after) && before != after
}

fn main() {
    let mut parser = ArgParser::new("d2save_temporal_witness").description(
        "Read-only progression partial-order and item-group witness discovery for save directories",
    );
    parser.add_spec(ArgSpec::positional(
        "directory",
        "Directory containing .d2s files",
    ));
    parser.add_spec(ArgSpec::option(
        "max-files",
        Some('n'),
        Some("max-files"),
        "Bound the number of sorted save files to index",
    ));
    parser.add_spec(ArgSpec::option(
        "skip-files",
        None,
        Some("skip-files"),
        "Skip this many lexically sorted save files before applying --max-files",
    ));
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(parsed) => parsed,
        Err(ArgError::Help(help)) => {
            println!("{}", help);
            process::exit(0);
        }
        Err(ArgError::Error(error)) => {
            eprintln!("Error: {}", error);
            process::exit(1);
        }
    };
    let mut out = OutputManager::new("d2save_temporal_witness", &parsed);
    let directory = Path::new(parsed.get("directory").unwrap());
    let mut paths = Vec::new();
    if let Err(error) = collect_saves(directory, &mut paths) {
        eprintln!("Error: {}", error);
        process::exit(1);
    }
    paths.sort();
    if let Some(skip) = parsed
        .get("skip-files")
        .and_then(|value| value.parse::<usize>().ok())
    {
        paths.drain(..skip.min(paths.len()));
    }
    if let Some(limit) = parsed
        .get("max-files")
        .and_then(|value| value.parse::<usize>().ok())
    {
        paths.truncate(limit);
    }
    let mut indexed: Vec<_> = paths
        .par_iter()
        .filter_map(|path| node_from_path(path).ok())
        .collect();
    indexed.sort_by(|(left, _), (right, _)| left.path.cmp(&right.path));
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (node, _) in &indexed {
        nodes.push(SaveNode {
            path: node.path.clone(),
            progression_supported: node.progression_supported,
            completed_quests: node.completed_quests.clone(),
            active_waypoints: node.active_waypoints.clone(),
            item_groups: node
                .item_groups
                .iter()
                .map(|group| ItemGroup {
                    key: group.key.clone(),
                    count: group.count,
                    template_candidate: group.template_candidate,
                    template_candidate_basis: group.template_candidate_basis,
                    evidence_grade: group.evidence_grade,
                })
                .collect(),
        });
    }
    for left_index in 0..indexed.len() {
        for right_index in left_index + 1..indexed.len() {
            let (left, left_groups) = &indexed[left_index];
            let (right, right_groups) = &indexed[right_index];
            let left_before_right = left.progression_supported
                && right.progression_supported
                && (is_strict_subset(&left.completed_quests, &right.completed_quests)
                    || is_strict_subset(&left.active_waypoints, &right.active_waypoints));
            let right_before_left = left.progression_supported
                && right.progression_supported
                && (is_strict_subset(&right.completed_quests, &left.completed_quests)
                    || is_strict_subset(&right.active_waypoints, &left.active_waypoints));
            let (before, before_groups, after, after_groups) = if left_before_right {
                (left, left_groups, right, right_groups)
            } else if right_before_left {
                (right, right_groups, left, left_groups)
            } else {
                continue;
            };
            let keys: BTreeSet<_> = before_groups.keys().chain(after_groups.keys()).collect();
            let mut item_deltas = Vec::new();
            for key in keys.iter() {
                let before_count = before_groups.get(*key).map_or(0, |group| group.count);
                let after_count = after_groups.get(*key).map_or(0, |group| group.count);
                if before_count == after_count {
                    continue;
                }
                let group = after_groups
                    .get(*key)
                    .or_else(|| before_groups.get(*key))
                    .unwrap();
                item_deltas.push(GroupDelta {
                    key: (*key).clone(),
                    classification: if before_count == 0 {
                        "introduced"
                    } else if after_count == 0 {
                        "removed"
                    } else {
                        "mutated"
                    },
                    before_count,
                    after_count,
                    template_candidate: group.template_candidate,
                    template_candidate_basis: group.template_candidate_basis,
                    evidence_grade: group.evidence_grade,
                });
            }
            edges.push(TemporalEdge { before: before.path.clone(), after: after.path.clone(), relation: "forward_partial_order", item_deltas, hint: "Run d2save_equipped_invariant on endpoints before treating equipped groups as identity witnesses.".to_string() });
        }
    }
    let node_count = nodes.len();
    let edge_count = edges.len();
    let payload = TemporalWitnessPayload { nodes, edges, hint: "This report is a partial order. Filename order and parser-derived slots are not authoritative identity proof.".to_string() };
    let directory_label = directory.to_string_lossy();
    let report = Report::new(
        ReportMetadata::new("d2save_temporal_witness", &directory_label, "Alpha v105"),
        ReportStatus::Ok,
    )
    .with_results(payload);
    if out.is_json() {
        out.json(&serde_json::to_string_pretty(&report).unwrap());
    } else {
        out.println(&format!(
            "Temporal witness index: {} nodes, {} forward edges",
            node_count, edge_count
        ));
        out.println("Use --json --output <path> for the machine-readable report.");
    }
}
