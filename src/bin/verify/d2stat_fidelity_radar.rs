use clap_stub::parse_args;
use d2r_core::item::{HuffmanTree, Item};
use rayon::prelude::*;
use serde::Serialize;
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
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut items_dir = None;
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
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct RadarReport {
    total_files_scanned: usize,
    total_items_scanned: usize,
    property_lists_examined: usize,
    property_lists_complete: usize,
    property_list_completion_rate: f64,
    semantic_option_coverage_percent: Option<f64>,
    semantic_option_coverage_reason: String,
    bit_replay_items: usize,
    f_bit_lossless: usize,
    f_bit_rate: f64,
    mismatches: Vec<RadarMismatch>,
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
        self.property_lists_examined += other.property_lists_examined;
        self.property_lists_complete += other.property_lists_complete;
        self.bit_replay_items += other.bit_replay_items;
        self.f_bit_lossless += other.f_bit_lossless;
        self.mismatches.append(&mut other.mismatches);
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
) {
    report.total_items_scanned += 1;

    if !item.properties.is_empty() {
        report.property_lists_examined += 1;
        if item.properties_complete {
            report.property_lists_complete += 1;
        }
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
}

fn analyze_save(path: &Path) -> RadarReport {
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
    let mut report = RadarReport::default();
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
            save_files.iter().map(|path| analyze_save(path)).collect()
        } else {
            save_files
                .par_iter()
                .map(|path| analyze_save(path))
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
