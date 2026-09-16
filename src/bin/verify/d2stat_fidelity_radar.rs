use clap_stub::parse_args;
use d2r_core::item::{HuffmanTree, Item};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

mod clap_stub {
    pub struct Args {
        pub corpus_dir: Option<String>,
        pub items_dir: Option<String>,
        pub fixture_name: Option<String>,
        pub output_file: Option<String>,
        pub token_efficient: bool,
        pub sequential: bool,
        pub strict_rebuild: bool,
        pub metric_version: Option<String>,
        pub parser_revision: Option<String>,
        pub corpus_manifest: Option<String>,
        pub worktree_state: Option<String>,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut items_dir = None;
        let mut fixture_name = None;
        let mut output_file = None;
        let mut token_efficient = false;
        let mut sequential = false;
        let mut strict_rebuild = false;
        let mut metric_version = None;
        let mut parser_revision = None;
        let mut corpus_manifest = None;
        let mut worktree_state = None;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--corpus-dir" => {
                    if i + 1 < args.len() {
                        corpus_dir = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--items-dir" => {
                    if i + 1 < args.len() {
                        items_dir = Some(args[i + 1].clone());
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
                "--token-efficient" => {
                    token_efficient = true;
                }
                "--sequential" => {
                    sequential = true;
                }
                "--strict-rebuild" => {
                    strict_rebuild = true;
                }
                "--metric-version" => {
                    if i + 1 < args.len() {
                        metric_version = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--parser-revision" => {
                    if i + 1 < args.len() {
                        parser_revision = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--corpus-manifest" => {
                    if i + 1 < args.len() {
                        corpus_manifest = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--worktree-state" => {
                    if i + 1 < args.len() {
                        worktree_state = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        Args {
            corpus_dir,
            items_dir,
            fixture_name,
            output_file,
            token_efficient,
            sequential,
            strict_rebuild,
            metric_version,
            parser_revision,
            corpus_manifest,
            worktree_state,
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct RadarReport {
    total_files_scanned: usize,
    total_items_scanned: usize,
    standard_items_count: usize,
    forensic_items_count: usize,
    total_semantic_properties_decoded: usize,
    property_lists_examined: usize,
    property_lists_complete: usize,
    property_list_completion_rate: f64,
    semantic_option_coverage_percent: Option<f64>,
    semantic_option_coverage_reason: String,
    bit_replay_items: usize,
    f_bit_lossless: usize,
    f_bit_rate: f64,
    strict_rebuild_examined: usize,
    strict_rebuild_lossless: usize,
    strict_rebuild_rate: f64,
    standard_strict_rebuild_examined: usize,
    standard_strict_rebuild_lossless: usize,
    standard_strict_rebuild_rate: f64,
    forensic_strict_rebuild_examined: usize,
    forensic_strict_rebuild_lossless: usize,
    forensic_strict_rebuild_rate: f64,
    evaluation_context: EvaluationContext,
    forensic_taxonomy: ForensicTaxonomy,
    mismatches: Vec<RadarMismatch>,
}

#[derive(Debug, Serialize, Default)]
struct EvaluationContext {
    metric_version: String,
    parser_revision: String,
    corpus_manifest: String,
    worktree_state: String,
    strict_rebuild: bool,
}

#[derive(Debug, Serialize, Default)]
struct ForensicTaxonomy {
    by_code: BTreeMap<String, usize>,
    by_slot: BTreeMap<String, usize>,
    by_quality: BTreeMap<String, usize>,
    by_reason: BTreeMap<String, usize>,
}

impl ForensicTaxonomy {
    fn merge(&mut self, other: Self) {
        for (k, v) in other.by_code {
            *self.by_code.entry(k).or_insert(0) += v;
        }
        for (k, v) in other.by_slot {
            *self.by_slot.entry(k).or_insert(0) += v;
        }
        for (k, v) in other.by_quality {
            *self.by_quality.entry(k).or_insert(0) += v;
        }
        for (k, v) in other.by_reason {
            *self.by_reason.entry(k).or_insert(0) += v;
        }
    }
}

#[derive(Debug, Serialize)]
struct RadarMismatch {
    file: String,
    item_index: Option<usize>,
    code: Option<String>,
    reason: String,
    hint: String,
}

impl RadarReport {
    fn merge(&mut self, mut other: Self) {
        self.total_files_scanned += other.total_files_scanned;
        self.total_items_scanned += other.total_items_scanned;
        self.standard_items_count += other.standard_items_count;
        self.forensic_items_count += other.forensic_items_count;
        self.total_semantic_properties_decoded += other.total_semantic_properties_decoded;
        self.property_lists_examined += other.property_lists_examined;
        self.property_lists_complete += other.property_lists_complete;
        self.bit_replay_items += other.bit_replay_items;
        self.f_bit_lossless += other.f_bit_lossless;
        self.strict_rebuild_examined += other.strict_rebuild_examined;
        self.strict_rebuild_lossless += other.strict_rebuild_lossless;
        self.standard_strict_rebuild_examined += other.standard_strict_rebuild_examined;
        self.standard_strict_rebuild_lossless += other.standard_strict_rebuild_lossless;
        self.forensic_strict_rebuild_examined += other.forensic_strict_rebuild_examined;
        self.forensic_strict_rebuild_lossless += other.forensic_strict_rebuild_lossless;
        self.forensic_taxonomy.merge(other.forensic_taxonomy);
        self.mismatches.append(&mut other.mismatches);
    }
}

fn evaluation_context(args: &clap_stub::Args) -> EvaluationContext {
    EvaluationContext {
        metric_version: args
            .metric_version
            .clone()
            .unwrap_or_else(|| "radar-v1".to_string()),
        parser_revision: args
            .parser_revision
            .clone()
            .unwrap_or_else(|| "unrecorded".to_string()),
        corpus_manifest: args
            .corpus_manifest
            .clone()
            .unwrap_or_else(|| "unrecorded".to_string()),
        worktree_state: args
            .worktree_state
            .clone()
            .unwrap_or_else(|| "unrecorded".to_string()),
        strict_rebuild: args.strict_rebuild,
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

fn record_item_metrics(
    report: &mut RadarReport,
    path: &Path,
    item_index: usize,
    item: &Item,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    strict_rebuild: bool,
) {
    report.total_items_scanned += 1;

    let trimmed = item.code.trim();
    let is_forensic = trimmed.is_empty()
        || item.is_opaque()
        || item.is_residue()
        || item.is_semi_opaque()
        || item.body.v105_7mgw_payload.is_some()
        || item.modules.iter().any(|m| {
            matches!(
                m,
                d2r_core::domain::item::ItemModule::Opaque(_)
                    | d2r_core::domain::item::ItemModule::Residue(_)
                    | d2r_core::domain::item::ItemModule::SemiOpaque { .. }
            )
        });

    if is_forensic {
        report.forensic_items_count += 1;

        // Taxonomy breakdown
        let code_key = if trimmed.is_empty() { "(empty)".to_string() } else { trimmed.to_string() };
        *report.forensic_taxonomy.by_code.entry(code_key).or_insert(0) += 1;

        let slot_key = if item.header.location == 1 {
            match item.header.y {
                1 => "Equipped:Helm",
                2 => "Equipped:Amulet",
                3 => "Equipped:Armor",
                4 => "Equipped:RightHand",
                5 => "Equipped:LeftHand",
                6 => "Equipped:RightRing",
                7 => "Equipped:LeftRing",
                8 => "Equipped:Belt",
                9 => "Equipped:Boots",
                10 => "Equipped:Gloves",
                11 => "Equipped:AlternateRight",
                12 => "Equipped:AlternateLeft",
                _ => "Equipped:Other",
            }.to_string()
        } else {
            match item.header.location {
                0 => "Stored:Inventory/Stash/Cube",
                2 => "BeltStorage",
                4 => "Socketed",
                _ => "OtherLocation",
            }.to_string()
        };
        *report.forensic_taxonomy.by_slot.entry(slot_key).or_insert(0) += 1;

        let quality_key = match item.header.quality {
            Some(q) => format!("{:?}", q),
            None => if let Some(raw_q) = item.header.alpha_quality_raw {
                format!("AlphaRawQuality({})", raw_q)
            } else {
                "UnknownQuality".to_string()
            },
        };
        *report.forensic_taxonomy.by_quality.entry(quality_key).or_insert(0) += 1;

        let reason_key = if trimmed.is_empty() {
            "EmptyCode".to_string()
        } else if item.is_residue() {
            "ScannerResidue".to_string()
        } else if item.body.v105_7mgw_payload.is_some() {
            "V105_7mgwPayload".to_string()
        } else if item.is_semi_opaque() {
            let reason = item.modules.iter().find_map(|m| {
                if let d2r_core::domain::item::ItemModule::SemiOpaque { reason, .. } = m {
                    Some(reason.clone())
                } else {
                    None
                }
            }).unwrap_or_else(|| "SemiOpaqueWithoutReason".to_string());
            format!("SemiOpaque({})", reason)
        } else if item.is_opaque() {
            "OpaqueBitstream".to_string()
        } else {
            "OtherModuleForensic".to_string()
        };
        *report.forensic_taxonomy.by_reason.entry(reason_key).or_insert(0) += 1;
    } else {
        report.standard_items_count += 1;
    }

    if !item.properties.is_empty() {
        report.property_lists_examined += 1;
        if item.properties_complete {
            report.property_lists_complete += 1;
        }
        report.total_semantic_properties_decoded +=
            item.properties.iter().filter(|p| !p.is_opaque).count();
    }

    if item.bits.is_empty() {
        report.mismatches.push(RadarMismatch {
            file: path.display().to_string(),
            item_index: Some(item_index),
            code: Some(item.code.trim().to_string()),
            reason: "item_has_no_recorded_raw_bits".to_string(),
            hint: format!(
                "d2item_inspect.exe {} --section-item {} --json",
                path.display(),
                item_index + 1
            ),
        });
        return;
    }

    report.bit_replay_items += 1;
    let recorded: Vec<bool> = item.bits.iter().map(|bit| bit.bit).collect();
    match item.to_bits(item_index, huffman, alpha_mode) {
        Ok(emitted) if emitted == recorded => report.f_bit_lossless += 1,
        Ok(emitted) => report.mismatches.push(RadarMismatch {
            file: path.display().to_string(),
            item_index: Some(item_index),
            code: Some(item.code.trim().to_string()),
            reason: format!(
                "item_replay_mismatch recorded_bits={} emitted_bits={}",
                recorded.len(),
                emitted.len()
            ),
            hint: format!(
                "d2item_inspect.exe {} --section-item {} --section-context --json",
                path.display(),
                item_index + 1
            ),
        }),
        Err(error) => report.mismatches.push(RadarMismatch {
            file: path.display().to_string(),
            item_index: Some(item_index),
            code: Some(item.code.trim().to_string()),
            reason: format!("item_replay_error: {error}"),
            hint: format!(
                "d2item_inspect.exe {} --section-item {} --json",
                path.display(),
                item_index + 1
            ),
        }),
    }

    if strict_rebuild {
        report.strict_rebuild_examined += 1;
        if is_forensic {
            report.forensic_strict_rebuild_examined += 1;
        } else {
            report.standard_strict_rebuild_examined += 1;
        }

        let mut rebuilt = item.clone();
        rebuilt.bits.clear();
        match rebuilt.to_bits(item_index, huffman, alpha_mode) {
            Ok(emitted) if emitted == recorded => {
                report.strict_rebuild_lossless += 1;
                if is_forensic {
                    report.forensic_strict_rebuild_lossless += 1;
                } else {
                    report.standard_strict_rebuild_lossless += 1;
                }
            }
            _ => {}
        }
    }
}

fn analyze_save(path: &Path, strict_rebuild: bool) -> RadarReport {
    eprintln!("[RADAR] Analyzing: {}", path.display());
    let mut report = RadarReport {
        total_files_scanned: 1,
        ..RadarReport::default()
    };

    match fs::read(path) {
        Ok(bytes) => {
            let alpha_mode = is_alpha_v105(&bytes);
            let huffman = HuffmanTree::new();
            match Item::read_player_items(&bytes, &huffman, alpha_mode) {
                Ok(items) => {
                    for (item_index, item) in items.iter().enumerate() {
                        record_item_metrics(
                            &mut report,
                            path,
                            item_index,
                            item,
                            &huffman,
                            alpha_mode,
                            strict_rebuild,
                        );
                    }
                }
                Err(error) => report.mismatches.push(RadarMismatch {
                    file: path.display().to_string(),
                    item_index: None,
                    code: None,
                    reason: format!("player_item_parse_error: {error}"),
                    hint: format!("d2save_verify.exe {} --json", path.display()),
                }),
            }
        }
        Err(error) => report.mismatches.push(RadarMismatch {
            file: path.display().to_string(),
            item_index: None,
            code: None,
            reason: format!("save_read_error: {error}"),
            hint: format!("Get-Item {}", path.display()),
        }),
    }

    report
}

fn main() {
    let args = parse_args();
    let mut report = RadarReport {
        evaluation_context: evaluation_context(&args),
        ..RadarReport::default()
    };
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
            save_files.iter().map(|path| analyze_save(path, args.strict_rebuild)).collect()
        } else {
            save_files
                .par_iter()
                .map(|path| analyze_save(path, args.strict_rebuild))
                .collect()
        };
        for partial in partial_reports {
            report.merge(partial);
        }
    }

    if report.property_lists_examined > 0 {
        report.property_list_completion_rate =
            (report.property_lists_complete as f64 / report.property_lists_examined as f64) * 100.0;
    }
    if report.bit_replay_items > 0 {
        report.f_bit_rate = (report.f_bit_lossless as f64 / report.bit_replay_items as f64) * 100.0;
    }
    if report.strict_rebuild_examined > 0 {
        report.strict_rebuild_rate =
            (report.strict_rebuild_lossless as f64 / report.strict_rebuild_examined as f64) * 100.0;
    }
    if report.standard_strict_rebuild_examined > 0 {
        report.standard_strict_rebuild_rate =
            (report.standard_strict_rebuild_lossless as f64 / report.standard_strict_rebuild_examined as f64) * 100.0;
    }
    if report.forensic_strict_rebuild_examined > 0 {
        report.forensic_strict_rebuild_rate =
            (report.forensic_strict_rebuild_lossless as f64 / report.forensic_strict_rebuild_examined as f64) * 100.0;
    }

    if let Some(items_dir) = args.items_dir {
        report.mismatches.push(RadarMismatch {
            file: items_dir,
            item_index: None,
            code: None,
            reason: "items_dir_is_not_yet_supported".to_string(),
            hint: "Use --corpus-dir with .d2s saves; standalone .d2i requires an explicit alpha-mode contract."
                .to_string(),
        });
    }

    report.semantic_option_coverage_reason =
        "unavailable: this corpus has no independent expected-option oracle".to_string();

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

    #[test]
    fn evaluation_context_serializes_explicit_and_defaulted_values() {
        let explicit = clap_stub::Args {
            corpus_dir: None,
            items_dir: None,
            fixture_name: None,
            output_file: None,
            token_efficient: false,
            sequential: false,
            strict_rebuild: true,
            metric_version: Some("radar-provenance-v1".to_string()),
            parser_revision: Some("abc123".to_string()),
            corpus_manifest: Some("normal-88-current".to_string()),
            worktree_state: Some("clean".to_string()),
        };
        let defaulted = clap_stub::Args {
            corpus_dir: None,
            items_dir: None,
            fixture_name: None,
            output_file: None,
            token_efficient: false,
            sequential: false,
            strict_rebuild: false,
            metric_version: None,
            parser_revision: None,
            corpus_manifest: None,
            worktree_state: None,
        };

        let report = RadarReport {
            evaluation_context: evaluation_context(&explicit),
            ..RadarReport::default()
        };
        let serialized = serde_json::to_value(report).expect("serialize radar report");

        assert_eq!(
            serialized["evaluation_context"]["metric_version"],
            "radar-provenance-v1"
        );
        assert_eq!(
            serialized["evaluation_context"]["parser_revision"],
            "abc123"
        );
        assert_eq!(
            serialized["evaluation_context"]["corpus_manifest"],
            "normal-88-current"
        );
        assert_eq!(serialized["evaluation_context"]["worktree_state"], "clean");
        assert_eq!(serialized["evaluation_context"]["strict_rebuild"], true);

        let defaults = evaluation_context(&defaulted);
        assert_eq!(defaults.metric_version, "radar-v1");
        assert_eq!(defaults.parser_revision, "unrecorded");
        assert_eq!(defaults.corpus_manifest, "unrecorded");
        assert_eq!(defaults.worktree_state, "unrecorded");
        assert!(!defaults.strict_rebuild);
    }
}
