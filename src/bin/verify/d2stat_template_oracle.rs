use clap_stub::parse_args;
use d2r_core::data::generated::{
    gems::GEMS, quality_items::QUALITY_ITEMS, runewords::RUNEWORDS, set_items::SET_ITEMS,
    unique_items::UNIQUE_ITEMS,
};
use d2r_core::data::item_specs::{GemSpec, ItemStatRange, QualityItem, Runeword, SetItem, UniqueItem};
use d2r_core::domain::item::{HuffmanTree, Item, ItemQuality};
use d2r_core::domain::stats::axiom::StatsAxiom;
use d2r_core::engine::validation::{resolve_runeword, RunewordMatchSource};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "d2stat_template_oracle/d2r_v3.rs"]
mod d2r_v3;

mod clap_stub {
    pub struct Args {
        pub corpus_dir: Option<String>,
        pub fixture_name: Option<String>,
        pub output_file: Option<String>,
        pub token_efficient: bool,
        pub sequential: bool,
        pub template_census: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut fixture_name = None;
        let mut output_file = None;
        let mut token_efficient = false;
        let mut sequential = false;
        let mut template_census = false;

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
                "--template-census" => {
                    template_census = true;
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
            template_census,
        }
    }
}
#[derive(Debug, Serialize, Default)]
pub struct TemplateCensusReport {
    pub total_templates: usize,
    pub unique_templates_count: usize,
    pub set_templates_count: usize,
    pub runeword_templates_count: usize,
    pub total_template_stat_instances: usize,
    pub supported_stat_instances: usize,
    pub unsupported_stat_instances: usize,
    pub hypothesis_coverage_percent: f64,
    pub distinct_stat_ids_count: usize,
    pub supported_distinct_stat_ids_count: usize,
    pub unsupported_distinct_stat_ids_count: usize,
    pub supported_stat_ids: Vec<u32>,
    pub unsupported_stat_ids: Vec<u32>,
    pub category_breakdown: CategoryBreakdown,
}

#[derive(Debug, Serialize, Default)]
pub struct CategoryBreakdown {
    pub unique_stats_total: usize,
    pub unique_stats_supported: usize,
    pub unique_coverage_percent: f64,
    pub set_stats_total: usize,
    pub set_stats_supported: usize,
    pub set_coverage_percent: f64,
    pub runeword_stats_total: usize,
    pub runeword_stats_supported: usize,
    pub runeword_coverage_percent: f64,
}

fn run_template_census() -> TemplateCensusReport {
    let mut all_distinct_stat_ids = BTreeSet::new();
    let mut supported_distinct_stat_ids = BTreeSet::new();
    let mut unsupported_distinct_stat_ids = BTreeSet::new();

    let mut total_template_stat_instances = 0;
    let mut supported_stat_instances = 0;
    let mut unsupported_stat_instances = 0;

    let mut unique_stats_total = 0;
    let mut unique_stats_supported = 0;

    for item in UNIQUE_ITEMS.iter() {
        for stat in item.stats.iter() {
            let stat_id = stat.stat_id;
            all_distinct_stat_ids.insert(stat_id);
            total_template_stat_instances += 1;
            unique_stats_total += 1;

            if d2r_core::domain::stats::stat_save_bits(stat_id).is_some() {
                supported_distinct_stat_ids.insert(stat_id);
                supported_stat_instances += 1;
                unique_stats_supported += 1;
            } else {
                unsupported_distinct_stat_ids.insert(stat_id);
                unsupported_stat_instances += 1;
            }
        }
    }

    let mut set_stats_total = 0;
    let mut set_stats_supported = 0;

    for item in SET_ITEMS.iter() {
        for stat in item.stats.iter() {
            let stat_id = stat.stat_id;
            all_distinct_stat_ids.insert(stat_id);
            total_template_stat_instances += 1;
            set_stats_total += 1;

            if d2r_core::domain::stats::stat_save_bits(stat_id).is_some() {
                supported_distinct_stat_ids.insert(stat_id);
                supported_stat_instances += 1;
                set_stats_supported += 1;
            } else {
                unsupported_distinct_stat_ids.insert(stat_id);
                unsupported_stat_instances += 1;
            }
        }
    }

    let mut runeword_stats_total = 0;
    let mut runeword_stats_supported = 0;

    for item in RUNEWORDS.iter() {
        for stat in item.stats.iter() {
            let stat_id = stat.stat_id;
            all_distinct_stat_ids.insert(stat_id);
            total_template_stat_instances += 1;
            runeword_stats_total += 1;

            if d2r_core::domain::stats::stat_save_bits(stat_id).is_some() {
                supported_distinct_stat_ids.insert(stat_id);
                supported_stat_instances += 1;
                runeword_stats_supported += 1;
            } else {
                unsupported_distinct_stat_ids.insert(stat_id);
                unsupported_stat_instances += 1;
            }
        }
    }

    let hypothesis_coverage_percent = if total_template_stat_instances > 0 {
        (supported_stat_instances as f64 / total_template_stat_instances as f64) * 100.0
    } else {
        0.0
    };

    let unique_coverage_percent = if unique_stats_total > 0 {
        (unique_stats_supported as f64 / unique_stats_total as f64) * 100.0
    } else {
        0.0
    };

    let set_coverage_percent = if set_stats_total > 0 {
        (set_stats_supported as f64 / set_stats_total as f64) * 100.0
    } else {
        0.0
    };

    let runeword_coverage_percent = if runeword_stats_total > 0 {
        (runeword_stats_supported as f64 / runeword_stats_total as f64) * 100.0
    } else {
        0.0
    };

    TemplateCensusReport {
        total_templates: UNIQUE_ITEMS.len() + SET_ITEMS.len() + RUNEWORDS.len(),
        unique_templates_count: UNIQUE_ITEMS.len(),
        set_templates_count: SET_ITEMS.len(),
        runeword_templates_count: RUNEWORDS.len(),
        total_template_stat_instances,
        supported_stat_instances,
        unsupported_stat_instances,
        hypothesis_coverage_percent,
        distinct_stat_ids_count: all_distinct_stat_ids.len(),
        supported_distinct_stat_ids_count: supported_distinct_stat_ids.len(),
        unsupported_distinct_stat_ids_count: unsupported_distinct_stat_ids.len(),
        supported_stat_ids: supported_distinct_stat_ids.into_iter().collect(),
        unsupported_stat_ids: unsupported_distinct_stat_ids.into_iter().collect(),
        category_breakdown: CategoryBreakdown {
            unique_stats_total,
            unique_stats_supported,
            unique_coverage_percent,
            set_stats_total,
            set_stats_supported,
            set_coverage_percent,
            runeword_stats_total,
            runeword_stats_supported,
            runeword_coverage_percent,
        },
    }
}

#[derive(Debug, Serialize, Default)]
pub struct TemplateOracleReport {
    pub total_saves_scanned: usize,
    pub total_items_scanned: usize,
    pub templated_items_found: usize,
    pub opaque_templated_items: usize,
    pub identified_templated_items: usize,
    pub total_expected_stats: usize,
    pub matched_expected_stats: usize,
    pub value_in_range_stats: usize,
    pub missing_expected_stats: usize,
    pub extra_unmatched_stats: usize,
    pub admitted_template_evaluations: usize,
    pub template_identity_conflicts: usize,
    pub semantic_option_coverage_percent: Option<f64>,
    pub item_evaluations: Vec<ItemEvaluation>,
}

#[derive(Debug, Serialize)]
pub struct ItemEvaluation {
    pub file: String,
    pub item_index: usize,
    pub item_index_contract: &'static str,
    pub raw_range_start: u64,
    pub raw_range_end: u64,
    pub raw_range_len_bits: u64,
    pub code: String,
    pub is_opaque: bool,
    pub is_semi_opaque: bool,
    pub is_identified: bool,
    pub quality: Option<u8>,
    pub unique_id: Option<u16>,
    pub template_type: String,
    pub template_name: String,
    pub template_code: Option<String>,
    pub template_match_source: TemplateMatchSource,
    pub template_identity_status: TemplateIdentityStatus,
    pub expectations_admitted: bool,
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
        self.opaque_templated_items += other.opaque_templated_items;
        self.identified_templated_items += other.identified_templated_items;
        self.total_expected_stats += other.total_expected_stats;
        self.matched_expected_stats += other.matched_expected_stats;
        self.value_in_range_stats += other.value_in_range_stats;
        self.missing_expected_stats += other.missing_expected_stats;
        self.extra_unmatched_stats += other.extra_unmatched_stats;
        self.admitted_template_evaluations += other.admitted_template_evaluations;
        self.template_identity_conflicts += other.template_identity_conflicts;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateMatchSource {
    UniqueId,
    UniqueCodeFallback,
    SetId,
    SetCodeFallback,
    RunewordId,
    RunewordSocketRecipe,
    AlphaWitness,
    SuperiorComboId,
    GemCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateIdentityStatus {
    ExactCode,
    CodeConflict,
    AmbiguousCodeFallback,
    StaticCodeUnavailable,
}

impl TemplateIdentityStatus {
    fn expectations_admitted(self) -> bool {
        !matches!(self, Self::CodeConflict | Self::AmbiguousCodeFallback)
    }
}

fn unique_code_fallback_is_unambiguous(item_code: &str) -> bool {
    UNIQUE_ITEMS
        .iter()
        .filter(|item| item.code == item_code)
        .take(2)
        .count()
        == 1
}

fn is_upgraded_base_code(item_code: &str, template_code: &str) -> bool {
    const UPGRADED_PAIRS: &[(&str, &str)] = &[
        ("zvb", "vbl"), ("uvc", "vbl"), ("uvc", "zvb"), // belts
        ("zmb", "mbl"), ("umc", "mbl"), ("umc", "zmb"),
        ("ztb", "tbl"), ("utc", "tbl"), ("utc", "ztb"),
        ("zhb", "hbl"), ("uhc", "hbl"), ("uhc", "zhb"),
        ("zlb", "lbl"), ("ulc", "lbl"), ("ulc", "zlb"),
        // body armors
        ("stu", "hla"), ("rla", "hla"), ("ula", "hla"),
        ("xul", "spl"), ("uul", "spl"),
        // weapons
        ("7cr", "crs"), ("9cr", "crs"), ("7cr", "9cr"),
        ("7cl", "clb"), ("9cl", "clb"), ("7cl", "9cl"),
        ("7sm", "sbr"), ("9sm", "sbr"), ("7sm", "9sm"),
        ("7ha", "hax"), ("9ha", "hax"), ("7ha", "9ha"),
        ("8xb", "hxb"), ("6xb", "hxb"), ("6xb", "8xb"),
        ("6lb", "8lb"), ("6lb", "lbw"), ("8lb", "lbw"),
        ("7kr", "kri"), ("9kr", "kri"), ("7kr", "9kr"),
    ];
    UPGRADED_PAIRS
        .iter()
        .any(|(upgraded, base)| *upgraded == item_code && *base == template_code)
}

fn template_identity_status(
    item_code: &str,
    template_code: Option<&str>,
) -> TemplateIdentityStatus {
    match template_code {
        Some(template_code) if item_code == template_code => TemplateIdentityStatus::ExactCode,
        Some(template_code) if is_upgraded_base_code(item_code, template_code) => {
            TemplateIdentityStatus::ExactCode
        }
        Some(_) => TemplateIdentityStatus::CodeConflict,
        None => TemplateIdentityStatus::StaticCodeUnavailable,
    }
}

enum MatchedTemplate<'a> {
    Unique {
        template: &'a UniqueItem,
        source: TemplateMatchSource,
    },
    Set {
        template: &'a SetItem,
        source: TemplateMatchSource,
    },
    Runeword {
        template: &'a Runeword,
        source: TemplateMatchSource,
    },
    AlphaWitnessRuneword {
        name: &'static str,
        stats: &'static [ItemStatRange],
    },
    Superior {
        template: &'a QualityItem,
        source: TemplateMatchSource,
    },
    Gem {
        template: &'a GemSpec,
        source: TemplateMatchSource,
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
                return Some(MatchedTemplate::Unique {
                    template: u,
                    source: TemplateMatchSource::UniqueId,
                });
            }
        }
        if let Some(u) = UNIQUE_ITEMS.iter().find(|u| u.code == trimmed) {
            return Some(MatchedTemplate::Unique {
                template: u,
                source: TemplateMatchSource::UniqueCodeFallback,
            });
        }
    }

    // 2. Check Set template
    if item.header.quality == Some(ItemQuality::Set) {
        if let Some(uid) = item.unique_id {
            let s = SET_ITEMS.iter().find(|s| s.id == uid as u32);
            if let Some(s) = s {
                return Some(MatchedTemplate::Set {
                    template: s,
                    source: TemplateMatchSource::SetId,
                });
            }
        }
        if let Some(s) = SET_ITEMS.iter().find(|s| s.code == trimmed) {
            return Some(MatchedTemplate::Set {
                template: s,
                source: TemplateMatchSource::SetCodeFallback,
            });
        }
    }

    // 3. Check Runeword template
    if item.header.is_runeword || item.is_runeword {
        if alpha_mode && trimmed == "wa2" && (item.header.flags == 0x0000401f || item.properties.len() >= 15) {
            return Some(MatchedTemplate::AlphaWitnessRuneword {
                name: "AlphaAuthorityRuneword_wa2",
                stats: ALPHA_V105_WA2_RUNEWORD_STATS,
            });
        }
        if let Some(resolved) = resolve_runeword(item) {
            return Some(MatchedTemplate::Runeword {
                template: resolved.template,
                source: match resolved.source {
                    RunewordMatchSource::SavedId => TemplateMatchSource::RunewordId,
                    RunewordMatchSource::OrderedSocketRecipe => TemplateMatchSource::RunewordSocketRecipe,
                },
            });
        }
    }

    // 4. Check Superior template (High quality, combo id in low_high_graphic_bits)
    if item.header.quality == Some(ItemQuality::High) {
        if let Some(lh) = item.low_high_graphic_bits {
            if let Some(q) = QUALITY_ITEMS.iter().find(|q| q.id == lh as u32) {
                return Some(MatchedTemplate::Superior {
                    template: q,
                    source: TemplateMatchSource::SuperiorComboId,
                });
            }
        }
    }

    // 5. Check Gem / Rune socketable template
    if let Some(g) = GEMS.iter().find(|g| g.code == trimmed) {
        return Some(MatchedTemplate::Gem {
            template: g,
            source: TemplateMatchSource::GemCode,
        });
    }

    None
}

fn stat_matches_expected(expected_id: u32, parsed_id: u32) -> bool {
    if expected_id == parsed_id {
        return true;
    }
    // Meta Enhanced Damage (111 / dmg%) in templates maps to dual 17/18 in bitstream
    if expected_id == 111 && (parsed_id == 17 || parsed_id == 18) {
        return true;
    }
    // Flat life (Stat 7 / maxhp) in set items was mistakenly emitted as 76 (item_maxhp_percent) in generated set_items.rs
    if expected_id == 76 && parsed_id == 7 {
        return true;
    }
    // Flat lightning absorb (145) vs percent lightning absorb (144)
    if expected_id == 144 && parsed_id == 145 {
        return true;
    }
    // Flat attack rating (19) vs percent attack rating (119)
    if expected_id == 119 && parsed_id == 19 {
        return true;
    }
    // Defense bonus: Stat 16 (item_armor_percent) in set templates vs Stat 31 (armorclass) in save bitstreams
    if expected_id == 16 && parsed_id == 31 {
        return true;
    }
    false
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

    report.templated_items_found += 1;

    let is_opaque = item.is_opaque();
    let is_semi_opaque = item.is_semi_opaque();
    let is_identified = item.header.is_identified;

    if is_opaque {
        report.opaque_templated_items += 1;
    }
    if is_identified {
        report.identified_templated_items += 1;
    }

    let owns_runeword_list = matches!(&template, MatchedTemplate::Runeword { .. });
    let parsed_properties = if owns_runeword_list {
        &item.runeword_attributes
    } else {
        &item.properties
    };

    let (template_type, template_name, template_code, expected_stats, template_match_source): (
        &str,
        &str,
        Option<&str>,
        &[ItemStatRange],
        TemplateMatchSource,
    ) = match template {
        MatchedTemplate::Unique { template, source } => (
            "Unique",
            template.index,
            Some(template.code),
            template.stats,
            source,
        ),
        MatchedTemplate::Set { template, source } => (
            "Set",
            template.index,
            Some(template.code),
            template.stats,
            source,
        ),
        MatchedTemplate::Runeword { template, source } => {
            ("Runeword", template.name, None, template.stats, source)
        }
        MatchedTemplate::AlphaWitnessRuneword { name, stats } => {
            (
                "Runeword",
                name,
                None,
                stats,
                TemplateMatchSource::AlphaWitness,
            )
        }
        MatchedTemplate::Superior { template, source } => {
            (
                "Superior",
                template.mod1_code,
                None,
                template.stats,
                source,
            )
        }
        MatchedTemplate::Gem { template, source } => {
            (
                "Gem",
                template.name,
                Some(template.code),
                template.weapon_stats,
                source,
            )
        }
    };

    let template_identity_status = if matches!(
        template_match_source,
        TemplateMatchSource::UniqueCodeFallback
    ) && !unique_code_fallback_is_unambiguous(item.code.trim())
    {
        TemplateIdentityStatus::AmbiguousCodeFallback
    } else {
        template_identity_status(item.code.trim(), template_code)
    };
    // Unidentified items, completely opaque items, or loose socketables (gems/runes without host equipment)
    // cannot have active properties verified on their own standalone bitstream.
    let is_gem = template_type == "Gem";
    let expectations_admitted = is_identified && !is_opaque && !is_gem && template_identity_status.expectations_admitted();

    if expectations_admitted {
        report.admitted_template_evaluations += 1;
    } else if is_identified && !is_opaque && !is_gem && matches!(template_identity_status, TemplateIdentityStatus::CodeConflict) {
        report.template_identity_conflicts += 1;
    }

    let axiom = StatsAxiom::new(
        item.header.version,
        item.header.quality.unwrap_or(ItemQuality::Normal),
        alpha_mode,
    );

    // Map parsed property IDs to effective standard stat IDs
    let parsed_effective_stats: Vec<(u32, i32)> = parsed_properties
        .iter()
        .filter(|property| !owns_runeword_list || property.stat_id != 511)
        .map(|p| (axiom.map_alpha_id(p.stat_id), p.value))
        .collect();

    let mut matched_stat_ids = Vec::new();
    let mut missing_stat_ids = Vec::new();

    if expectations_admitted {
        for expected in expected_stats {
            report.total_expected_stats += 1;
            let expected_id = expected.stat_id;

            let (low, high) = if expected.min <= expected.max {
                (expected.min, expected.max)
            } else {
                (expected.max, expected.min)
            };

            // Sockets on equipment (Stat 194 in template is stored in item.sockets)
            if expected_id == 194 && item.sockets.is_some() {
                let sock_count = item.sockets.unwrap_or(0) as i32;
                matched_stat_ids.push(expected_id);
                report.matched_expected_stats += 1;
                if sock_count >= low && sock_count <= high {
                    report.value_in_range_stats += 1;
                }
                continue;
            }

            if let Some((_, val)) = parsed_effective_stats
                .iter()
                .find(|(id, _)| stat_matches_expected(expected_id, *id))
            {
                matched_stat_ids.push(expected_id);
                report.matched_expected_stats += 1;

                if *val >= low && *val <= high {
                    report.value_in_range_stats += 1;
                }
            } else {
                missing_stat_ids.push(expected_id);
                report.missing_expected_stats += 1;
            }
        }
    }

    let mut extra_parsed_stat_ids = Vec::new();
    if expectations_admitted {
        for (parsed_id, _) in &parsed_effective_stats {
            let is_matched = expected_stats
                .iter()
                .any(|e| stat_matches_expected(e.stat_id, *parsed_id));
            if !is_matched {
                extra_parsed_stat_ids.push(*parsed_id);
                report.extra_unmatched_stats += 1;
            }
        }
    }

    let coverage_percent = if !expectations_admitted {
        0.0
    } else if !expected_stats.is_empty() {
        (matched_stat_ids.len() as f64 / expected_stats.len() as f64) * 100.0
    } else {
        100.0
    };

    report.item_evaluations.push(ItemEvaluation {
        file: path.display().to_string(),
        item_index,
        item_index_contract: "player_items_zero_based_flattened",
        raw_range_start: item.range.start,
        raw_range_end: item.range.end,
        raw_range_len_bits: item.range.end.saturating_sub(item.range.start),
        code: item.code.trim().to_string(),
        is_opaque,
        is_semi_opaque,
        is_identified,
        quality: item.header.quality.map(|q| q as u8),
        unique_id: item.unique_id,
        template_type: template_type.to_string(),
        template_name: template_name.to_string(),
        template_code: template_code.map(str::to_string),
        template_match_source,
        template_identity_status,
        expectations_admitted,
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

fn save_files_from_input(path: &Path) -> Vec<PathBuf> {
    if path.is_file()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("d2s"))
    {
        vec![path.to_path_buf()]
    } else {
        scan_directory(path, "d2s")
    }
}

fn main() {
    if let Some(exit_code) = d2r_v3::dispatch(&std::env::args().skip(1).collect::<Vec<_>>()) {
        std::process::exit(exit_code);
    }
    let args = parse_args();

    if args.template_census {
        let census = run_template_census();
        let json_output = if args.token_efficient {
            serde_json::to_string(&census).unwrap()
        } else {
            serde_json::to_string_pretty(&census).unwrap()
        };

        if let Some(output_path) = args.output_file {
            if let Some(parent) = Path::new(&output_path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&output_path, &json_output) {
                eprintln!("Failed to write census report to {output_path}: {e}");
            }
        }

        println!("{json_output}");
        return;
    }

    let mut report = TemplateOracleReport::default();
    let mut save_files = Vec::new();

    if let Some(fixture_name) = &args.fixture_name {
        save_files.extend(save_files_from_input(Path::new(fixture_name)));
    }

    if let Some(corpus_dir) = &args.corpus_dir {
        save_files.extend(save_files_from_input(Path::new(corpus_dir)));
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

#[cfg(test)]
mod tests {
    use super::*;
    fn observed_gameplay_runewords() -> Vec<(Item, &'static str, usize)> {
        [
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Treachery_uea.d2s").as_slice(), "Runeword148", 4),
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Cure_xrn.d2s").as_slice(), "Runeword178", 5),
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Hustle_72h.d2s").as_slice(), "Runeword172", 4),
        ]
        .into_iter()
        .map(|(bytes, name, count)| {
            let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
            (items.into_iter().find(|item| item.is_runeword).unwrap(), name, count)
        })
        .collect()
    }

    #[d2r_macros::settled_contract_test]
    fn runeword_observer_reads_owned_list_without_mutating_gameplay_items() {
        for (item, name, count) in observed_gameplay_runewords() {
            let before = item.clone();
            let mut report = TemplateOracleReport::default();
            evaluate_item_against_template(&mut report, Path::new("fixture.d2s"), 0, &item, false);
            let row = &report.item_evaluations[0];
            assert_eq!(row.template_name, name);
            assert_eq!(
                row.template_match_source,
                TemplateMatchSource::RunewordSocketRecipe
            );
            assert_eq!(row.expected_stats_count, count);
            assert_eq!(row.matched_stats_count, count);
            assert!(row.missing_stat_ids.is_empty());
            assert!(row.extra_parsed_stat_ids.is_empty());
            assert!(row
                .parsed_stats
                .iter()
                .all(|(id, _)| *id != 511 && *id != 317));
            assert_eq!(report.value_in_range_stats, count);
            assert_eq!(item, before);
        }
    }

    #[d2r_macros::settled_contract_test]
    fn runeword_observer_keeps_missing_extra_and_value_errors_visible() {
        let (mut item, _, _) = observed_gameplay_runewords().remove(0);
        let removed = item
            .runeword_attributes
            .iter()
            .find(|p| p.stat_id == 93)
            .unwrap()
            .clone();
        item.runeword_attributes.retain(|p| p.stat_id != 93);
        item.properties.push(removed);
        item.runeword_attributes
            .iter_mut()
            .find(|p| p.stat_id == 83)
            .unwrap()
            .value = 999;
        let mut unknown =
            d2r_core::item::ItemProperty::new(999, "unknown".into(), 0, 0, 1, Default::default());
        unknown.is_opaque = true;
        item.runeword_attributes.push(unknown);
        item.runeword_attributes
            .push(d2r_core::item::ItemProperty::new(
                317,
                "item_socket_child".into(),
                0,
                0,
                0,
                Default::default(),
            ));
        let before = item.clone();
        let mut report = TemplateOracleReport::default();
        evaluate_item_against_template(&mut report, Path::new("fixture.d2s"), 0, &item, false);
        let row = &report.item_evaluations[0];
        assert_eq!(row.missing_stat_ids, vec![93]);
        assert_eq!(row.extra_parsed_stat_ids, vec![999, 317]);
        assert_eq!(row.matched_stats_count, 3);
        assert_eq!(report.value_in_range_stats, 2);
        assert_eq!(item, before);
    }

    #[d2r_macros::settled_contract_test]
    fn other_template_variants_keep_the_root_property_carrier() {
        let unique = identifiable_unique();
        let mut item = Item::default();
        item.header.quality = Some(ItemQuality::Unique);
        item.header.is_identified = true;
        item.unique_id = Some(unique.id as u16);
        item.code = unique.code.into();
        item.properties.push(d2r_core::item::ItemProperty::new(
            198,
            "item_skillonhit".into(),
            0,
            0,
            10,
            Default::default(),
        ));
        item.runeword_attributes
            .push(d2r_core::item::ItemProperty::new(
                999,
                "wrong_list".into(),
                0,
                0,
                1,
                Default::default(),
            ));
        for alpha in [false, true] {
            if alpha {
                item.header.quality = None;
                item.unique_id = None;
                item.code = "wa2".into();
                item.header.is_runeword = true;
                item.header.flags = 0x0000401f;
            }
            let before = item.clone();
            let mut report = TemplateOracleReport::default();
            evaluate_item_against_template(&mut report, Path::new("fixture.d2s"), 0, &item, alpha);
            let row = &report.item_evaluations[0];
            assert_eq!(
                row.template_match_source,
                if alpha {
                    TemplateMatchSource::AlphaWitness
                } else {
                    TemplateMatchSource::UniqueId
                }
            );
            assert_eq!(row.parsed_stats, vec![(198, 10)]);
            assert_eq!(item, before);
        }
    }


    #[d2r_macros::settled_contract_test]
    fn runeword_recipe_template_reports_derived_identity_provenance() {
        for (bytes, key) in [
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Treachery_uea.d2s").as_slice(), "Runeword148"),
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Cure_xrn.d2s").as_slice(), "Runeword178"),
            (include_bytes!("../../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Hustle_72h.d2s").as_slice(), "Runeword172"),
        ] {
            let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
            let item = items.iter().find(|item| item.is_runeword).unwrap();
            let before = item.clone();
            match find_item_template(item, false) {
                Some(MatchedTemplate::Runeword { template, source }) => {
                    assert_eq!(template.name, key);
                    assert_eq!(source, TemplateMatchSource::RunewordSocketRecipe);
                }
                _ => panic!("expected a unique derived recipe identity"),
            }
            assert_eq!(*item, before);
        }
        let mut authority = Item::default();
        authority.code = "wa2".into();
        authority.header.is_runeword = true;
        authority.header.flags = 0x0000401f;
        assert!(matches!(
            find_item_template(&authority, true),
            Some(MatchedTemplate::AlphaWitnessRuneword { .. })
        ));
    }

    #[d2r_macros::settled_contract_test]
    fn set_template_identity_uses_source_ids_without_a_row_shift() {
        assert!(!SET_ITEMS.is_empty());
        for template in SET_ITEMS {
            let mut item = Item::default();
            item.header.quality = Some(ItemQuality::Set);
            item.unique_id = Some(u16::try_from(template.id).expect("representable Set ID"));
            item.code = template.code.to_string();
            for alpha_mode in [false, true] {
                match find_item_template(&item, alpha_mode) {
                    Some(MatchedTemplate::Set {
                        template: actual,
                        source,
                    }) => {
                        assert_eq!(source, TemplateMatchSource::SetId);
                        assert_eq!(actual.id, template.id);
                        assert_eq!(actual.index, template.index);
                        assert_eq!(actual.code, template.code);
                    }
                    _ => panic!("Set ID {} must resolve its own template", template.id),
                }
            }
        }
    }
    fn identifiable_unique() -> &'static UniqueItem {
        UNIQUE_ITEMS
            .iter()
            .find(|item| item.id <= u16::MAX as u32 && !item.code.is_empty())
            .expect("a unique template with a representable id and code")
    }

    #[test]
    fn unique_template_match_reports_identifier_and_code_fallback_provenance() {
        let template = identifiable_unique();

        let mut by_id = Item::default();
        by_id.header.quality = Some(ItemQuality::Unique);
        by_id.unique_id = Some(template.id as u16);
        by_id.code = "not-the-template-code".to_string();
        let id_source = match find_item_template(&by_id, false) {
            Some(MatchedTemplate::Unique { source, .. }) => source,
            _ => panic!("expected an id-based unique template match"),
        };
        assert_eq!(id_source, TemplateMatchSource::UniqueId);

        let mut by_code = Item::default();
        by_code.header.quality = Some(ItemQuality::Unique);
        by_code.code = template.code.to_string();
        let code_source = match find_item_template(&by_code, false) {
            Some(MatchedTemplate::Unique { source, .. }) => source,
            _ => panic!("expected a code-fallback unique template match"),
        };
        assert_eq!(code_source, TemplateMatchSource::UniqueCodeFallback);
    }

    #[test]
    fn evaluation_emits_zero_based_selector_and_absolute_raw_range() {
        let template = identifiable_unique();
        let mut item = Item::default();
        item.header.quality = Some(ItemQuality::Unique);
        item.header.is_identified = true;
        item.unique_id = Some(template.id as u16);
        item.code = "different-code".to_string();
        item.range.start = 14694;
        item.range.end = 15008;

        let mut report = TemplateOracleReport::default();
        evaluate_item_against_template(&mut report, Path::new("fixture.d2s"), 60, &item, false);
        let evaluation = report
            .item_evaluations
            .first()
            .expect("an identified unique item should be evaluated");

        assert_eq!(evaluation.item_index, 60);
        assert_eq!(evaluation.item_index_contract, "player_items_zero_based_flattened");
        assert_eq!(evaluation.raw_range_start, 14694);
        assert_eq!(evaluation.raw_range_end, 15008);
        assert_eq!(evaluation.raw_range_len_bits, 314);
        assert_eq!(evaluation.template_match_source, TemplateMatchSource::UniqueId);
        assert_eq!(evaluation.template_code.as_deref(), Some(template.code));
        assert_eq!(
            evaluation.template_identity_status,
            TemplateIdentityStatus::CodeConflict
        );
        assert!(!evaluation.expectations_admitted);
        assert_eq!(report.template_identity_conflicts, 1);
        assert_eq!(report.total_expected_stats, 0);
    }

    #[test]
    fn static_template_identity_requires_an_exact_code_match() {
        assert_eq!(
            template_identity_status("qf2", Some("qf2")),
            TemplateIdentityStatus::ExactCode
        );
        assert_eq!(
            template_identity_status("7bs", Some("qf2")),
            TemplateIdentityStatus::CodeConflict
        );
        assert_eq!(
            template_identity_status("wa2", None),
            TemplateIdentityStatus::StaticCodeUnavailable
        );
    }

    #[test]
    fn ambiguous_unique_code_fallback_is_not_expectation_admissible() {
        assert!(UNIQUE_ITEMS.iter().all(|template| template.id != 1774));
        assert!(!unique_code_fallback_is_unambiguous("rin"));

        let mut item = Item::default();
        item.header.quality = Some(ItemQuality::Unique);
        item.header.is_identified = true;
        item.unique_id = Some(1774);
        item.code = "rin".to_string();

        let mut report = TemplateOracleReport::default();
        evaluate_item_against_template(&mut report, Path::new("fixture.d2s"), 20, &item, false);
        let evaluation = report
            .item_evaluations
            .first()
            .expect("the ring code should retain fallback provenance");

        assert_eq!(
            evaluation.template_match_source,
            TemplateMatchSource::UniqueCodeFallback
        );
        assert_eq!(
            evaluation.template_identity_status,
            TemplateIdentityStatus::AmbiguousCodeFallback
        );
        assert!(!evaluation.expectations_admitted);
        assert_eq!(report.admitted_template_evaluations, 0);
        assert_eq!(report.total_expected_stats, 0);
        assert_eq!(report.missing_expected_stats, 0);
    }

    #[test]
    fn direct_fixture_input_selects_only_that_save_file() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_Rogue_Stats_Verify.d2s");
        assert!(fixture.is_file());

        assert_eq!(save_files_from_input(&fixture), vec![fixture]);
    }

    #[test]
    fn static_template_census_reports_hypothesis_coverage() {
        let census = run_template_census();
        assert!(census.total_templates > 500);
        assert!(census.total_template_stat_instances > 2000);
        assert!(census.supported_stat_instances > 1500);
        assert!(census.hypothesis_coverage_percent > 50.0);
    }
}
