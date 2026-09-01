use std::fs;
use std::path::{Path, PathBuf};
use clap_stub::parse_args;
use serde::Serialize;

mod clap_stub {
    pub struct Args {
        pub corpus_dir: Option<String>,
        pub items_dir: Option<String>,
        pub token_efficient: bool,
        pub help: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut items_dir = None;
        let mut token_efficient = false;
        let mut help = false;

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
                "--token-efficient" | "--json" => {
                    token_efficient = true;
                }
                "--help" | "-h" => {
                    help = true;
                }
                _ => {}
            }
            i += 1;
        }

        Args {
            corpus_dir,
            items_dir,
            token_efficient,
            help,
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct ProbeOracleReport {
    total_saves_scanned: usize,
    total_items_evaluated: usize,
    live_semantic_option_coverage_percent: Option<f64>,
    live_semantic_option_coverage_reason: String,
    controlled_witness_status: String,
    anomalies: Vec<String>,
}

fn scan_directory(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
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
    files
}

fn main() {
    let args = parse_args();
    let mut report = ProbeOracleReport::default();

    if args.help {
        println!(
            "d2stat_probe_oracle --corpus-dir <DIR> [--items-dir <DIR>] [--json|--token-efficient]\n\n\
             Reports whether this CLI has an independent live semantic-option oracle.\n\
             It never substitutes file readability or raw-bit replay for option coverage."
        );
        return;
    }

    if let Some(corpus_dir) = args.corpus_dir {
        let saves = scan_directory(Path::new(&corpus_dir), "d2s");
        report.total_saves_scanned = saves.len();
        let unreadable = saves.iter().filter(|path| fs::read(path).is_err()).count();
        if unreadable > 0 {
            report.anomalies.push(format!(
                "{unreadable} save files could not be read while enumerating the corpus"
            ));
        }
    } else {
        report
            .anomalies
            .push("no corpus directory was supplied".to_string());
    }

    if args.items_dir.is_some() {
        report.anomalies.push(
            "--items-dir is not evaluated: this probe has no independent expected-option oracle"
                .to_string(),
        );
    }

    report.live_semantic_option_coverage_percent = None;
    report.live_semantic_option_coverage_reason =
        "unavailable: this CLI has no independent expected-option oracle for live save items"
            .to_string();
    report.controlled_witness_status =
        "v96/v99 controlled semantic witness is verified by legacy_stat_snippets_test; it is not live coverage"
            .to_string();

    if args.token_efficient {
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }
}
