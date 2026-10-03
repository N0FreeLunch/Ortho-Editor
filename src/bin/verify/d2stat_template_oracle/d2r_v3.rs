//! Opt-in, read-only fixed-template observer for counted D2R v3 primary items.
use super::{evaluate_item_against_template, TemplateOracleReport};
use d2r_core::item::{HuffmanTree, Item};
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "d2stat-template-oracle-d2r-v3/v1";
const INDEX_CONTRACT: &str = "counted_primary_zero_based";

#[derive(Debug, Serialize)]
struct Failure {
    kind: &'static str,
    message: String,
    bit_offset: Option<u64>,
    hint: &'static str,
}

fn failure(kind: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        kind,
        message: message.into(),
        bit_offset: None,
        hint: if kind == "output_io_failure" {
            "Use a new output path whose parent exists; existing evidence is never overwritten."
        } else {
            "Use one full --file and explicit --dialect d2r-v3; verify the owned primary prefix. Do not infer dialect from version 105 or scan a later JM."
        },
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    file: PathBuf,
    output: Option<PathBuf>,
    json: bool,
    token_efficient: bool,
    help: bool,
}

fn explicitly_selected(args: &[String]) -> Result<bool, Failure> {
    let positions: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| *arg == "--dialect" || arg.starts_with("--dialect="))
        .collect();
    match positions.as_slice() {
        [] => Ok(false),
        [(index, arg)] if arg.as_str() == "--dialect" => {
            match args.get(index + 1).map(String::as_str) {
                Some("legacy") => Ok(false),
                Some("d2r-v3") => Ok(true),
                _ => Err(failure(
                    "invalid_dialect",
                    "Expected --dialect legacy or d2r-v3",
                )),
            }
        }
        _ => Err(failure(
            "invalid_dialect",
            "Supply --dialect exactly once using separate arguments",
        )),
    }
}

fn options(args: &[String]) -> Result<Options, Failure> {
    let mut result = Options::default();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let key = match flag {
            "--file" | "--fixture-name" => "file",
            "--output" | "-o" => "output",
            "--dialect" => "dialect",
            "--json" => "json",
            "--token-efficient" => "token-efficient",
            "--help" | "-h" => "help",
            _ => {
                return Err(failure(
                    "unsupported_option",
                    format!("Unsupported option: {flag}"),
                ))
            }
        };
        if !seen.insert(key) {
            return Err(failure(
                "duplicate_option",
                format!("Repeated {key} option"),
            ));
        }
        match key {
            "file" | "output" | "dialect" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| failure("missing_value", format!("{flag} requires a value")))?;
                match key {
                    "file" => result.file = PathBuf::from(value),
                    "output" => result.output = Some(PathBuf::from(value)),
                    _ if value == "d2r-v3" => {}
                    _ => return Err(failure("invalid_dialect", "Expected d2r-v3")),
                }
                index += 1;
            }
            "json" => result.json = true,
            "token-efficient" => result.token_efficient = true,
            "help" => result.help = true,
            _ => unreachable!(),
        }
        index += 1;
    }
    if !result.help {
        if result.file.as_os_str().is_empty() {
            return Err(failure(
                "missing_file",
                "A single full-save --file is required",
            ));
        }
        if result.token_efficient && result.output.is_none() {
            return Err(failure(
                "missing_output",
                "Token-efficient mode requires an explicit new output path",
            ));
        }
    }
    Ok(result)
}

#[derive(Serialize)]
struct Receipt {
    schema_version: &'static str,
    dialect: &'static str,
    status: &'static str,
    metric_outcome: &'static str,
    fixture: String,
    top_level_item_count: Option<usize>,
    item_index_contract: &'static str,
    evidence_surface: &'static str,
    coverage_limit: &'static str,
    report: TemplateOracleReport,
    errors: Vec<Failure>,
}

fn envelope(path: &Path) -> Receipt {
    Receipt {
        schema_version: SCHEMA,
        dialect: "d2r-v3",
        status: "error",
        metric_outcome: "not_measured",
        fixture: path.display().to_string(),
        top_level_item_count: None,
        item_index_contract: INDEX_CONTRACT,
        evidence_surface: "live-observer",
        coverage_limit: "Counted primary items only; children and secondary sections excluded. Template coverage counts stat-ID presence and separately value ranges; it is not canonical semantic-option inventory, spatial legality, game-load proof or Ftrue. Generated-template conflicts remain visible.",
        report: TemplateOracleReport::default(),
        errors: Vec::new(),
    }
}

fn observe(bytes: &[u8], path: &Path) -> Receipt {
    let mut receipt = envelope(path);
    let items = match Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()) {
        Ok(items) => items,
        Err(error) => {
            let mut detail = failure("primary_list_parse_failure", error.to_string());
            detail.bit_offset = Some(error.bit_offset);
            receipt.errors.push(detail);
            return receipt;
        }
    };
    receipt.top_level_item_count = Some(items.len());
    receipt.report.total_saves_scanned = 1;
    for (index, item) in items.iter().enumerate() {
        // Control sentinels own boundaries but are not semantic item options.
        let mut semantic_view = item.clone();
        semantic_view
            .properties
            .retain(|property| property.stat_id != 511);
        let old_len = receipt.report.item_evaluations.len();
        evaluate_item_against_template(&mut receipt.report, path, index, &semantic_view, false);
        if receipt.report.item_evaluations.len() > old_len {
            receipt
                .report
                .item_evaluations
                .last_mut()
                .unwrap()
                .item_index_contract = INDEX_CONTRACT;
        }
    }
    if receipt.report.total_expected_stats > 0 {
        receipt.report.semantic_option_coverage_percent = Some(
            100.0 * receipt.report.matched_expected_stats as f64
                / receipt.report.total_expected_stats as f64,
        );
    }
    receipt.status = "ok";
    receipt.metric_outcome = if receipt.report.total_expected_stats == 0 {
        "not_admitted"
    } else if receipt.report.missing_expected_stats > 0
        || receipt.report.extra_unmatched_stats > 0
        || receipt.report.value_in_range_stats != receipt.report.total_expected_stats
        || receipt.report.template_identity_conflicts > 0
    {
        "red"
    } else {
        "green"
    };
    receipt
}

fn emit(receipt: &Receipt, options: &Options) -> std::io::Result<()> {
    if let Some(path) = &options.output {
        let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
        serde_json::to_writer_pretty(&mut output, receipt)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    if options.json && !options.token_efficient {
        println!("{}", serde_json::to_string(receipt)?);
    } else {
        eprintln!("D2R v3 template oracle: status={}, metric={}, primary={:?}, matched={}/{}, in-range={}, errors={}",
            receipt.status, receipt.metric_outcome, receipt.top_level_item_count,
            receipt.report.matched_expected_stats, receipt.report.total_expected_stats,
            receipt.report.value_in_range_stats, receipt.errors.len());
    }
    Ok(())
}

pub(super) fn dispatch(args: &[String]) -> Option<i32> {
    let selection = explicitly_selected(args);
    if matches!(selection, Ok(false)) {
        return None;
    }
    let parsed = selection.and_then(|_| options(args));
    let options = match parsed {
        Ok(options) => options,
        Err(error) => {
            let mut receipt = envelope(Path::new(""));
            receipt.errors.push(error);
            println!("{}", serde_json::to_string(&receipt).unwrap());
            return Some(2);
        }
    };
    if options.help {
        println!("Usage: d2stat_template_oracle --dialect d2r-v3 --file <full.d2s> [--output <new.json>] [--json | --token-efficient]\nRead-only counted primary items; no corpus or fallback. Default/--dialect legacy retains the existing route.");
        return Some(0);
    }
    let receipt = match fs::read(&options.file) {
        Ok(bytes) => observe(&bytes, &options.file),
        Err(error) => {
            let mut receipt = envelope(&options.file);
            receipt
                .errors
                .push(failure("input_io_failure", error.to_string()));
            receipt
        }
    };
    if let Err(error) = emit(&receipt, &options) {
        eprintln!(
            "{}",
            serde_json::to_string(&failure("output_io_failure", error.to_string())).unwrap()
        );
        return Some(3);
    }
    Some(if receipt.status == "ok" { 0 } else { 2 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    fn args(input: &[&str]) -> Vec<String> {
        input.iter().map(|value| value.to_string()).collect()
    }

    fn fixture(name: &str, act: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/gameplay/normal")
            .join(act)
            .join(name)
    }

    #[settled_contract_test]
    fn explicit_cli_matrix_never_guesses_dialect_or_accepts_probe_flags() {
        assert!(!explicitly_selected(&args(&["--file", "x"])).unwrap());
        assert!(!explicitly_selected(&args(&["--dialect", "legacy"])).unwrap());
        assert!(explicitly_selected(&args(&["--dialect", "d2r-v3"])).unwrap());
        for input in [
            vec!["--dialect"],
            vec!["--dialect", "unknown"],
            vec!["--dialect", "legacy", "--dialect", "d2r-v3"],
            vec!["--dialect=d2r-v3"],
        ] {
            assert_eq!(
                explicitly_selected(&args(&input)).unwrap_err().kind,
                "invalid_dialect"
            );
        }
        for (tail, kind) in [
            (vec![], "missing_file"),
            (vec!["--file"], "missing_value"),
            (
                vec!["--file", "x", "--fixture-name", "x"],
                "duplicate_option",
            ),
            (vec!["--file", "x", "--token-efficient"], "missing_output"),
            (vec!["--corpus-dir", "x"], "unsupported_option"),
            (
                vec!["--file", "x", "--bit-offset", "1"],
                "unsupported_option",
            ),
            (vec!["--file", "x", "--output", "--json"], "missing_value"),
        ] {
            let mut input = args(&["--dialect", "d2r-v3"]);
            input.extend(args(&tail));
            assert_eq!(options(&input).unwrap_err().kind, kind);
        }
        assert!(
            options(&args(&["--dialect", "d2r-v3", "--help"]))
                .unwrap()
                .help
        );
    }

    #[settled_contract_test]
    fn manald_has_exact_identity_boundary_and_four_independent_template_expectations() {
        let path = fixture("TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s", "act1");
        let receipt = observe(&fs::read(&path).unwrap(), &path);
        assert_eq!(receipt.status, "ok");
        assert!(receipt.errors.is_empty());
        let rows: Vec<_> = receipt
            .report
            .item_evaluations
            .iter()
            .filter(|row| row.unique_id == Some(121))
            .collect();
        assert_eq!(rows.len(), 1);
        let row = rows[0];
        assert_eq!(row.code, "rin");
        assert_eq!(row.template_name, "Manald Heal");
        assert_eq!((row.raw_range_start, row.raw_range_end), (11320, 11536));
        assert_eq!(row.item_index_contract, INDEX_CONTRACT);
        assert!(row.expectations_admitted);
        assert_eq!((row.expected_stats_count, row.matched_stats_count), (4, 4));
        assert!(row.missing_stat_ids.is_empty());
        assert!(row.extra_parsed_stat_ids.is_empty());
        let mut parsed = row.parsed_stats.clone();
        parsed.sort();
        assert_eq!(parsed, vec![(7, 20), (27, 20), (62, 6), (74, 8)]);
        let template = super::super::UNIQUE_ITEMS
            .iter()
            .find(|item| item.id == 121)
            .unwrap();
        assert!(template
            .stats
            .iter()
            .all(|expected| row
                .parsed_stats
                .iter()
                .any(|(id, value)| *id == expected.stat_id
                    && *value >= expected.min
                    && *value <= expected.max)));
    }

    #[settled_contract_test]
    fn before_cap_cannot_become_manald() {
        let path = fixture("TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s", "act1");
        let receipt = observe(&fs::read(&path).unwrap(), &path);
        assert_eq!(receipt.status, "ok");
        assert!(receipt
            .report
            .item_evaluations
            .iter()
            .all(|row| row.unique_id != Some(121)));
    }

    #[settled_contract_test]
    fn malformed_input_is_unmeasured_not_zero_item_success() {
        for bytes in [vec![], vec![0; 32], vec![0x4a, 0x4d, 0, 0]] {
            let receipt = observe(&bytes, Path::new("malformed.d2s"));
            assert_eq!(receipt.status, "error");
            assert_eq!(receipt.metric_outcome, "not_measured");
            assert_eq!(receipt.top_level_item_count, None);
            assert_eq!(receipt.report.total_saves_scanned, 0);
            assert_eq!(receipt.errors.len(), 1);
            assert_eq!(receipt.errors[0].kind, "primary_list_parse_failure");
            assert!(!receipt.errors[0].hint.is_empty());
        }
    }

    #[settled_contract_test]
    fn cathan_source_concordance_and_counterfactual_mismatch() {
        use d2r_core::domain::stats::entity::ItemProperty;

        let path = fixture("TESTDRUID_Quest1_AshearaTalked.d2s", "act3");
        let receipt = observe(&fs::read(&path).unwrap(), &path);
        assert_eq!(receipt.status, "ok");
        assert_eq!(receipt.metric_outcome, "red");
        let row = receipt
            .report
            .item_evaluations
            .iter()
            .find(|row| row.unique_id == Some(29) && row.code == "rin")
            .unwrap();
        assert_eq!(row.item_index, 10);
        assert_eq!((row.raw_range_start, row.raw_range_end), (9616, 9832));
        assert!(row.expectations_admitted);
        assert_eq!((row.expected_stats_count, row.matched_stats_count), (2, 2));
        assert!(row.missing_stat_ids.is_empty());
        assert!(row.extra_parsed_stat_ids.is_empty());
        assert!(!row.extra_parsed_stat_ids.contains(&511));
        let mut parsed = row.parsed_stats.clone();
        parsed.sort();
        assert_eq!(parsed, vec![(34, 2), (60, 6)]);
        let template = super::super::SET_ITEMS
            .iter()
            .find(|template| template.id == 29)
            .unwrap();
        let mut expected: Vec<_> = template
            .stats
            .iter()
            .map(|stat| (stat.stat_id, stat.min, stat.max, stat.param))
            .collect();
        expected.sort();
        assert_eq!(expected, vec![(34, 2, 2, 0), (60, 6, 6, 0)]);

        // Synthetic wrong-stat input checks rejection; no save bytes are modified.
        let mut wrong = Item::default();
        wrong.header.version = 105;
        wrong.header.quality = Some(d2r_core::item::ItemQuality::Set);
        wrong.header.is_identified = true;
        wrong.unique_id = Some(29);
        wrong.code = row.code.clone();
        wrong.properties = parsed
            .iter()
            .map(|&(id, value)| {
                ItemProperty::new(
                    if id == 34 { 36 } else { id },
                    String::new(),
                    0,
                    value,
                    value,
                    Default::default(),
                )
            })
            .collect();
        let original = wrong.properties.clone();
        let mut rejected = TemplateOracleReport::default();
        super::super::evaluate_item_against_template(&mut rejected, &path, 10, &wrong, false);
        assert_eq!(wrong.properties, original);
        assert_eq!(rejected.item_evaluations.len(), 1);
        let rejected_row = &rejected.item_evaluations[0];
        assert!(rejected_row.expectations_admitted);
        assert_eq!(rejected_row.missing_stat_ids, vec![34]);
        assert_eq!(rejected_row.extra_parsed_stat_ids, vec![36]);
        assert_eq!(
            (
                rejected_row.expected_stats_count,
                rejected_row.matched_stats_count
            ),
            (2, 1)
        );
        assert_eq!(rejected.value_in_range_stats, 1);
    }

    #[settled_contract_test]
    fn output_collision_preserves_existing_bytes() {
        let output = fixture("TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s", "act1");
        let original = fs::read(&output).unwrap();
        let options = Options {
            output: Some(output.clone()),
            ..Options::default()
        };
        let error = emit(&envelope(Path::new("fixture.d2s")), &options).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(output).unwrap(), original);
    }
}
