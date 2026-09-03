use clap_stub::parse_args;
use d2r_core::data::generated::{runewords::RUNEWORDS, set_items::SET_ITEMS, unique_items::UNIQUE_ITEMS};
use d2r_core::data::item_specs::{ItemStatRange, Runeword, SetItem, UniqueItem};
use d2r_core::domain::item::{HuffmanTree, Item, ItemQuality};
use d2r_core::domain::stats::axiom::StatsAxiom;
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

mod clap_stub {
    pub struct Args {
        pub corpus_dir: Option<String>,
        pub fixture_name: Option<String>,
        pub output_file: Option<String>,
        pub token_efficient: bool,
        pub sequential: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut fixture_name = None;
        let mut output_file = None;
        let mut token_efficient = false;
        let mut sequential = false;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--corpus-dir" => {
                    if i + 1 < args.len() {
                        corpus_dir = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--fixture-name" | "--file" => {
                    if i + 1 < args.len() {
                        fixture_name = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--output" | "-o" => {
                    if i + 1 < args.len() {
                        output_file = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--token-efficient" | "--json" => {
                    token_efficient = true;
                }
                "--sequential" => {
                    sequential = true;
                }
                _ => {}
            }
            i += 1;
        }

        Args {
            corpus_dir,
            fixture_name,
            output_file,
            token_efficient,
            sequential,
        }
    }
}

#[derive(Debug, Serialize, Default)]
pub struct TemplateOracleReport {
    pub total_saves_scanned: usize,
    pub total_items_scanned: usize,
    pub templated_items_found: usize,
    pub total_expected_stats: usize,
    pub matched_expected_stats: usize,
    pub value_in_range_stats: usize,
    pub missing_expected_stats: usize,
    pub extra_unmatched_stats: usize,
    pub semantic_option_coverage_percent: Option<f64>,
    pub item_evaluations: Vec<ItemEvaluation>,
}

#[derive(Debug, Serialize)]
pub struct ItemEvaluation {
    pub file: String,
    pub item_index: usize,
    pub code: String,
    pub template_type: String,
    pub template_name: String,
    pub expected_stats_count: usize,
    pub matched_stats_count: usize,
    pub coverage_percent: f64,
    pub matched_stat_ids: Vec<u32>,
    pub missing_stat_ids: Vec<u32>,
    pub extra_parsed_stat_ids: Vec<u32>,
    pub parsed_stats: Vec<(u32, i32)>,
}

impl TemplateOracleReport {
    pub fn merge(&mut self, mut other: Self) {
        self.total_saves_scanned += other.total_saves_scanned;
        self.total_items_scanned += other.total_items_scanned;
        self.templated_items_found += other.templated_items_found;
        self.total_expected_stats += other.total_expected_stats;
        self.matched_expected_stats += other.matched_expected_stats;
        self.value_in_range_stats += other.value_in_range_stats;
        self.missing_expected_stats += other.missing_expected_stats;
        self.extra_unmatched_stats += other.extra_unmatched_stats;
        self.item_evaluations.append(&mut other.item_evaluations);
    }
}

fn scan_directory(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if dir.is_file() {
        if dir.extension().and_then(|s| s.to_str()) == Some(ext) || ext == "*" {
            files.push(dir.to_path_buf());
        }
        return files;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(scan_directory(&path, ext));
            } else if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn is_alpha_v105(bytes: &[u8]) -> bool {
    bytes
        .get(4..8)
        .and_then(|version| version.try_into().ok())
        .map(u32::from_le_bytes)
        .is_some_and(|version| version == 105 || version == 0x69)
}

enum MatchedTemplate<'a> {
    Unique(&'a UniqueItem),
    Set(&'a SetItem),
    Runeword(&'a Runeword),
    AlphaWitnessRuneword {
        name: &'static str,
        stats: &'static [ItemStatRange],
    },
}

// Alpha v105 Authority Runeword stats for `wa2` verified from `amazon_authority_runeword.d2s`
static ALPHA_V105_WA2_RUNEWORD_STATS: &[ItemStatRange] = &[
    ItemStatRange { stat_id: 198, min: 10, max: 10, param: 7695 }, // item_skillonhit
    ItemStatRange { stat_id: 201, min: 2, max: 2, param: 8010 },   // item_skillongethit
    ItemStatRange { stat_id: 127, min: 2, max: 2, param: 0 },      // item_allskills
    ItemStatRange { stat_id: 99, min: 20, max: 20, param: 0 },     // item_fastergethitrate
    ItemStatRange { stat_id: 17, min: 50, max: 50, param: 0 },     // item_maxdamage_percent
    ItemStatRange { stat_id: 91, min: -15, max: -15, param: 0 },   // item_req_percent
    ItemStatRange { stat_id: 39, min: 30, max: 30, param: 0 },     // fireresist
];

fn find_item_template<'a>(item: &Item, alpha_mode: bool) -> Option<MatchedTemplate<'a>> {
    let trimmed = item.code.trim();

    // 1. Check Unique template
    if item.header.quality == Some(ItemQuality::Unique) {
        if let Some(uid) = item.unique_id {
            if let Some(u) = UNIQUE_ITEMS.iter().find(|u| u.id == uid as u32) {
                return Some(MatchedTemplate::Unique(u));
            }
        }
        if let Some(u) = UNIQUE_ITEMS.iter().find(|u| u.code == trimmed) {
            return Some(MatchedTemplate::Unique(u));
        }
    }

    // 2. Check Set template
    if item.header.quality == Some(ItemQuality::Set) {
        if let Some(s) = SET_ITEMS.iter().find(|s| s.code == trimmed) {
            return Some(MatchedTemplate::Set(s));
        }
    }

    // 3. Check Runeword template
    if item.header.is_runeword || item.is_runeword {
        if alpha_mode && trimmed == "wa2" {
            return Some(MatchedTemplate::AlphaWitnessRuneword {
                name: "AlphaAuthorityRuneword_wa2",
                stats: ALPHA_V105_WA2_RUNEWORD_STATS,
            });
        }
        if let Some(rw_id) = item.runeword_id {
            if let Some(rw) = RUNEWORDS.iter().find(|rw| rw.id == rw_id as u32) {
                return Some(MatchedTemplate::Runeword(rw));
            }
        }
    }

    None
}

fn evaluate_item_against_template(
    report: &mut TemplateOracleReport,
    path: &Path,
    item_index: usize,
    item: &Item,
    alpha_mode: bool,
) {
    report.total_items_scanned += 1;

    let template = match find_item_template(item, alpha_mode) {
        Some(t) => t,
        None => return,
    };

    // Unidentified items do not decode properties in binary savegames;
    // skip template stat expectations for unidentified items to prevent false missing stats.
    if !item.header.is_identified {
        return;
    }

    report.templated_items_found += 1;

    let (template_type, template_name, expected_stats): (&str, &str, &[ItemStatRange]) = match template {
        MatchedTemplate::Unique(u) => ("Unique", u.index, u.stats),
        MatchedTemplate::Set(s) => ("Set", s.index, s.stats),
        MatchedTemplate::Runeword(rw) => ("Runeword", rw.name, rw.stats),
        MatchedTemplate::AlphaWitnessRuneword { name, stats } => ("Runeword", name, stats),
    };

    let axiom = StatsAxiom::new(
        item.header.version,
        item.header.quality.unwrap_or(ItemQuality::Normal),
        alpha_mode,
    );

    // Map parsed property IDs to effective standard stat IDs
    let parsed_effective_stats: Vec<(u32, i32)> = item
        .properties
        .iter()
        .map(|p| (axiom.map_alpha_id(p.stat_id), p.value))
        .collect();

    let mut matched_stat_ids = Vec::new();
    let mut missing_stat_ids = Vec::new();

    for expected in expected_stats {
        report.total_expected_stats += 1;
        let expected_id = expected.stat_id;

        if let Some((_, val)) = parsed_effective_stats.iter().find(|(id, _)| *id == expected_id) {
            matched_stat_ids.push(expected_id);
            report.matched_expected_stats += 1;

            if *val >= expected.min && *val <= expected.max {
                report.value_in_range_stats += 1;
            }
        } else {
            missing_stat_ids.push(expected_id);
            report.missing_expected_stats += 1;
        }
    }

    let mut extra_parsed_stat_ids = Vec::new();
    for (parsed_id, _) in &parsed_effective_stats {
        if !expected_stats.iter().any(|e| e.stat_id == *parsed_id) {
            extra_parsed_stat_ids.push(*parsed_id);
            report.extra_unmatched_stats += 1;
        }
    }

    let coverage_percent = if !expected_stats.is_empty() {
        (matched_stat_ids.len() as f64 / expected_stats.len() as f64) * 100.0
    } else {
        100.0
    };

    report.item_evaluations.push(ItemEvaluation {
        file: path.display().to_string(),
        item_index,
        code: item.code.trim().to_string(),
        template_type: template_type.to_string(),
        template_name: template_name.to_string(),
        expected_stats_count: expected_stats.len(),
        matched_stats_count: matched_stat_ids.len(),
        coverage_percent,
        matched_stat_ids,
        missing_stat_ids,
        extra_parsed_stat_ids,
        parsed_stats: parsed_effective_stats,
    });
}

fn evaluate_save_file(path: &Path) -> TemplateOracleReport {
    let mut report = TemplateOracleReport {
        total_saves_scanned: 1,
        ..TemplateOracleReport::default()
    };

    if let Ok(bytes) = fs::read(path) {
        let alpha_mode = is_alpha_v105(&bytes);
        let huffman = HuffmanTree::new();
        if let Ok(items) = Item::read_player_items(&bytes, &huffman, alpha_mode) {
            for (idx, item) in items.iter().enumerate() {
                evaluate_item_against_template(&mut report, path, idx, item, alpha_mode);
            }
        }
    }

    report
}

fn main() {
    let args = parse_args();
    let mut report = TemplateOracleReport::default();
    let mut save_files = Vec::new();

    if let Some(fixture_name) = &args.fixture_name {
        save_files.extend(scan_directory(Path::new(fixture_name), "d2s"));
    }

    if let Some(corpus_dir) = &args.corpus_dir {
        save_files.extend(scan_directory(Path::new(corpus_dir), "d2s"));
    }

    save_files.sort();
    save_files.dedup();

    if !save_files.is_empty() {
        let partial_reports: Vec<_> = if args.sequential {
            save_files.iter().map(|path| evaluate_save_file(path)).collect()
        } else {
            save_files
                .par_iter()
                .map(|path| evaluate_save_file(path))
                .collect()
        };
        for partial in partial_reports {
            report.merge(partial);
        }
    }

    if report.total_expected_stats > 0 {
        report.semantic_option_coverage_percent = Some(
            (report.matched_expected_stats as f64 / report.total_expected_stats as f64) * 100.0,
        );
    }

    let json_output = if args.token_efficient {
        serde_json::to_string(&report).unwrap()
    } else {
        serde_json::to_string_pretty(&report).unwrap()
    };

    if let Some(output_path) = args.output_file {
        if let Some(parent) = Path::new(&output_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&output_path, &json_output) {
            eprintln!("Failed to write report to {output_path}: {e}");
        } else {
            eprintln!("Report successfully written to {output_path}");
        }
    } else {
        println!("{}", json_output);
    }
}
