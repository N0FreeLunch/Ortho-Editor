//! Read-only projection of saved audit reports; never invokes a save parser.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const SCHEMA: &str = "d2item-global-audit-report-analysis/v1";
const DETAIL_LIMIT: usize = 10;
const LIMITATION: &str = "Saved-report projection only. Manifest authenticity, complete item inventory, game truth and fidelity improvement are not verified.";

#[derive(Deserialize)]
struct SavedReport {
    total_files: usize,
    total_pass: usize,
    total_fail: usize,
    total_items: u64,
    results: Vec<SavedRow>,
}

#[derive(Deserialize)]
struct SavedRow {
    filename: String,
    status: String,
    item_count: u64,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    hint: String,
    #[serde(default)]
    stage_timing_trace: Vec<StageEvent>,
}

#[derive(Deserialize)]
struct StageEvent {
    stage: String,
    event: String,
}

#[derive(Default)]
struct Options {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    json: bool,
    compact: bool,
    verbose: bool,
    fixture: Option<String>,
    expected_files: Option<usize>,
    help: bool,
}

#[derive(Debug)]
struct AnalysisError {
    code: i32,
    reason: &'static str,
    detail: String,
    hint: &'static str,
}

impl AnalysisError {
    fn input(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code: 2,
            reason,
            detail: detail.into(),
            hint: "Use analyze-report --help; supply one complete, unfiltered saved audit JSON with consistent counts.",
        }
    }

    fn output(detail: impl Into<String>) -> Self {
        Self {
            code: 3,
            reason: "output_io_or_collision",
            detail: detail.into(),
            hint: "Choose a new output file under an existing directory; in-place replacement is forbidden.",
        }
    }
}

#[derive(Serialize)]
struct Row {
    filename: String,
    status: &'static str,
    reported_item_count: u64,
    family: Option<String>,
    timeout_stage: Option<String>,
    hint: String,
}

#[derive(Serialize)]
struct Summary {
    schema: &'static str,
    status: &'static str,
    total_files: usize,
    counts: BTreeMap<&'static str, usize>,
    reported_item_count: u64,
    execution_incomplete: bool,
    inventory_completeness: &'static str,
    family_counts: BTreeMap<String, usize>,
    timeout_stages: BTreeMap<String, usize>,
    selected_count: usize,
    omitted_rows: usize,
    rows: Vec<Row>,
    hint: &'static str,
    coverage_limit: &'static str,
}

fn normalized_status(value: &str) -> Result<&'static str, AnalysisError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "PASS" | "[PASS]" => Ok("PASS"),
        "FAIL" | "[FAIL]" => Ok("FAIL"),
        "TIMEOUT" | "[TIMEOUT]" => Ok("TIMEOUT"),
        "ERROR" | "[ERROR]" => Ok("ERROR"),
        _ => Err(AnalysisError::input("unknown_status", value)),
    }
}

fn open_stage(events: &[StageEvent]) -> Result<Option<String>, AnalysisError> {
    let mut open: Vec<&str> = Vec::new();
    for event in events {
        if event.stage.trim().is_empty() {
            return Err(AnalysisError::input(
                "invalid_stage_trace",
                "Empty stage name",
            ));
        }
        match event.event.as_str() {
            "started" => {
                if open.contains(&event.stage.as_str()) {
                    return Err(AnalysisError::input(
                        "invalid_stage_trace",
                        "Repeated open stage",
                    ));
                }
                open.push(&event.stage);
            }
            "completed" => {
                let index = open.iter().position(|stage| *stage == event.stage);
                let Some(index) = index else {
                    return Err(AnalysisError::input(
                        "invalid_stage_trace",
                        "Completion without a start",
                    ));
                };
                open.remove(index);
            }
            _ => return Err(AnalysisError::input("invalid_stage_trace", &event.event)),
        }
    }
    Ok(open.last().map(|stage| (*stage).to_owned()))
}

fn analyze(report: SavedReport, options: &Options) -> Result<Summary, AnalysisError> {
    if report.total_files != report.results.len()
        || options
            .expected_files
            .is_some_and(|count| count != report.total_files)
    {
        return Err(AnalysisError::input(
            "file_count_mismatch",
            "Declared, observed or expected file counts differ",
        ));
    }
    let mut names = BTreeSet::new();
    let mut counts = BTreeMap::from([("PASS", 0), ("FAIL", 0), ("TIMEOUT", 0), ("ERROR", 0)]);
    let mut families = BTreeMap::new();
    let mut timeouts = BTreeMap::new();
    let mut items = 0_u64;
    let mut selected = 0;
    let mut rows = Vec::new();
    for row in report.results {
        if row.filename.trim().is_empty() || !names.insert(row.filename.clone()) {
            return Err(AnalysisError::input(
                "duplicate_or_empty_filename",
                row.filename,
            ));
        }
        let status = normalized_status(&row.status)?;
        *counts.get_mut(status).expect("Known status key") += 1;
        items = items
            .checked_add(row.item_count)
            .ok_or_else(|| AnalysisError::input("item_count_overflow", "Item sum exceeds u64"))?;
        let stage = open_stage(&row.stage_timing_trace)?;
        let timeout_stage =
            (status == "TIMEOUT").then(|| stage.unwrap_or_else(|| "unknown".to_owned()));
        if let Some(stage) = &timeout_stage {
            *timeouts.entry(stage.clone()).or_insert(0) += 1;
        }
        if status != "PASS" {
            *families
                .entry(
                    row.family
                        .clone()
                        .unwrap_or_else(|| "unclassified".to_owned()),
                )
                .or_insert(0) += 1;
        }
        if options
            .fixture
            .as_ref()
            .is_some_and(|fixture| fixture != &row.filename)
        {
            continue;
        }
        selected += 1;
        if status != "PASS" || options.verbose || options.fixture.is_some() {
            rows.push(Row {
                filename: row.filename,
                status,
                reported_item_count: row.item_count,
                family: row.family,
                timeout_stage,
                hint: row.hint.chars().take(240).collect(),
            });
        }
    }
    if counts["PASS"] != report.total_pass
        || report.total_files - counts["PASS"] != report.total_fail
        || items != report.total_items
    {
        return Err(AnalysisError::input(
            "aggregate_mismatch",
            "Top-level counts differ from row sums",
        ));
    }
    if options.fixture.is_some() && selected == 0 {
        return Err(AnalysisError::input(
            "fixture_not_found",
            "Exact filename selector did not match",
        ));
    }
    rows.sort_by(|left, right| left.filename.cmp(&right.filename));
    let length = rows.len();
    if !options.verbose {
        rows.truncate(DETAIL_LIMIT);
    }
    let incomplete = counts["TIMEOUT"] + counts["ERROR"] > 0;
    Ok(Summary {
        schema: SCHEMA,
        status: "ok",
        total_files: report.total_files,
        counts,
        reported_item_count: items,
        execution_incomplete: incomplete,
        inventory_completeness: "not_verified",
        family_counts: families,
        timeout_stages: timeouts,
        selected_count: selected,
        omitted_rows: length - rows.len(),
        rows,
        hint: if incomplete {
            "Inspect one timeout/error fixture and its stage trace; do not treat reported items as a complete census."
        } else {
            "Compare against independently verified manifest and domain evidence before admitting a baseline."
        },
        coverage_limit: LIMITATION,
    })
}

fn parse_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, AnalysisError> {
    let mut options = Options::default();
    let mut arguments = arguments.into_iter();
    let mut seen = BTreeSet::new();
    while let Some(argument) = arguments.next() {
        if argument.starts_with('-') && !seen.insert(argument.clone()) {
            return Err(AnalysisError::input("duplicate_option", argument));
        }
        match argument.as_str() {
            "--help" | "-h" => options.help = true,
            "--json" => options.json = true,
            "--token-efficient" => options.compact = true,
            "--verbose" => options.verbose = true,
            "--output" | "--fixture-name" | "--expected-files" => {
                let value = arguments
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| AnalysisError::input("missing_option_value", &argument))?;
                match argument.as_str() {
                    "--output" => options.output = Some(PathBuf::from(value)),
                    "--fixture-name" => options.fixture = Some(value),
                    _ => {
                        options.expected_files =
                            Some(value.parse().map_err(|_| {
                                AnalysisError::input("invalid_expected_files", value)
                            })?)
                    }
                }
            }
            _ if argument.starts_with('-') => {
                return Err(AnalysisError::input("unknown_option", argument));
            }
            _ if options.input.is_none() => options.input = Some(PathBuf::from(argument)),
            _ => return Err(AnalysisError::input("extra_input", argument)),
        }
    }
    if options.input.is_none() && !options.help {
        return Err(AnalysisError::input(
            "missing_input",
            "A saved audit JSON is required",
        ));
    }
    Ok(options)
}

fn execute(options: &Options) -> Result<(), AnalysisError> {
    let input = options.input.as_ref().expect("Validated input argument");
    let bytes =
        fs::read(input).map_err(|error| AnalysisError::input("input_io", error.to_string()))?;
    let report = serde_json::from_slice(&bytes)
        .map_err(|error| AnalysisError::input("invalid_report_json", error.to_string()))?;
    let summary = analyze(report, options)?;
    let json = if options.compact {
        serde_json::to_string(&summary)
    } else {
        serde_json::to_string_pretty(&summary)
    }
    .map_err(|error| AnalysisError::input("serialization_failure", error.to_string()))?;
    if let Some(path) = &options.output {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| AnalysisError::output(error.to_string()))?;
        output
            .write_all(json.as_bytes())
            .and_then(|()| output.write_all(b"\n"))
            .and_then(|()| output.flush())
            .map_err(|error| AnalysisError::output(error.to_string()))?;
    } else if options.json || options.compact {
        println!("{json}");
    } else {
        println!("Files | PASS | FAIL | TIMEOUT | ERROR | Reported items");
        println!(
            "{} | {} | {} | {} | {} | {}",
            summary.total_files,
            summary.counts["PASS"],
            summary.counts["FAIL"],
            summary.counts["TIMEOUT"],
            summary.counts["ERROR"],
            summary.reported_item_count
        );
        for row in summary.rows {
            println!(
                "{} | {} | {}",
                row.status,
                row.filename,
                row.timeout_stage.as_deref().unwrap_or("-")
            );
        }
        println!(
            "{}; omitted rows: {}. {}",
            summary.inventory_completeness, summary.omitted_rows, summary.hint
        );
    }
    Ok(())
}

pub(super) fn run(arguments: impl IntoIterator<Item = String>) -> i32 {
    let result = parse_options(arguments).and_then(|options| {
        if options.help {
            println!(
                "Usage: d2item_global_audit analyze-report <report.json> [options]\n\
                --json                 Machine-readable summary\n\
                --token-efficient      Compact JSON, bounded details (10 rows)\n\
                --verbose              Include all selected rows\n\
                --fixture-name <name>  Exact filename detail selector; totals stay global\n\
                --expected-files <N>   Require declared and observed file count N\n\
                --output <new.json>    Write JSON without overwriting; parent must exist\n\
                --help                 Show this help\n\
                Exit: 0 analysis succeeded (not audit passed), 2 input/CLI error, 3 output error.\n\
                No save parsing, in-place edits, manifest validation or fidelity claims."
            );
            Ok(())
        } else {
            execute(&options)
        }
    });
    match result {
        Ok(()) => 0,
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"schema": SCHEMA, "status": "error",
                "reason": error.reason, "detail": error.detail, "hint": error.hint})
            );
            error.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;
    use serde_json::{Value, json};

    fn report(rows: Vec<Value>) -> SavedReport {
        let passes = rows.iter().filter(|row| row["status"] == "[PASS]").count();
        let items: u64 = rows
            .iter()
            .map(|row| row["item_count"].as_u64().unwrap())
            .sum();
        serde_json::from_value(json!({"total_files":rows.len(), "total_pass":passes,
            "total_fail":rows.len()-passes, "total_items":items, "results":rows}))
        .unwrap()
    }

    fn row(name: &str, status: &str) -> Value {
        json!({"filename":name, "status":status, "item_count":2})
    }

    #[settled_contract_test]
    fn report_analysis_classifies_partial_execution_without_census_claim() {
        let mut timeout = row("b.d2s", "[TIMEOUT]");
        timeout["stage_timing_trace"] = json!([
            {"stage":"topology_parse","event":"started"},
            {"stage":"topology_parse","event":"completed"},
            {"stage":"strict_roundtrip_parse","event":"started"}]);
        let summary = analyze(
            report(vec![
                timeout,
                row("a.d2s", "[PASS]"),
                row("c.d2s", "[ERROR]"),
            ]),
            &Options::default(),
        )
        .unwrap();
        assert_eq!(summary.counts["PASS"], 1);
        assert_eq!(summary.counts["TIMEOUT"], 1);
        assert_eq!(summary.counts["ERROR"], 1);
        assert_eq!(summary.timeout_stages["strict_roundtrip_parse"], 1);
        assert_eq!(summary.reported_item_count, 6);
        assert!(summary.execution_incomplete);
        assert_eq!(summary.inventory_completeness, "not_verified");
    }

    #[settled_contract_test]
    fn report_analysis_bounds_and_sorts_details() {
        let rows = (0..12)
            .rev()
            .map(|index| row(&format!("{index:02}.d2s"), "[FAIL]"))
            .collect();
        let summary = analyze(report(rows), &Options::default()).unwrap();
        assert_eq!(summary.rows.len(), 10);
        assert_eq!(summary.omitted_rows, 2);
        assert_eq!(summary.rows[0].filename, "00.d2s");
        let summary = analyze(
            report(vec![row("p.d2s", "[PASS]")]),
            &Options {
                verbose: true,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(summary.rows.len(), 1);
        assert!(!summary.execution_incomplete);
        assert_eq!(summary.inventory_completeness, "not_verified");
    }

    #[settled_contract_test]
    fn report_analysis_selector_does_not_shrink_denominator() {
        let options = Options {
            fixture: Some("a.d2s".to_owned()),
            ..Options::default()
        };
        let summary = analyze(
            report(vec![row("a.d2s", "[PASS]"), row("b.d2s", "[FAIL]")]),
            &options,
        )
        .unwrap();
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.selected_count, 1);
        assert_eq!(summary.counts["FAIL"], 1);
        assert_eq!(summary.rows.len(), 1);
        assert!(analyze(report(vec![row("b.d2s", "[FAIL]")]), &options).is_err());
    }

    #[settled_contract_test]
    fn report_analysis_rejects_duplicate_empty_or_missing_rows() {
        for rows in [
            vec![row("a", "[PASS]"), row("a", "[FAIL]")],
            vec![row(" ", "[PASS]")],
        ] {
            assert!(analyze(report(rows), &Options::default()).is_err());
        }
        let mut missing = report(vec![row("a", "[PASS]")]);
        missing.total_files = 2;
        assert!(analyze(missing, &Options::default()).is_err());
    }

    #[settled_contract_test]
    fn report_analysis_rejects_inconsistent_aggregates() {
        for field in 0..3 {
            let mut input = report(vec![row("a", "[PASS]")]);
            match field {
                0 => input.total_pass = 0,
                1 => input.total_fail = 1,
                _ => input.total_items = 9,
            }
            assert!(analyze(input, &Options::default()).is_err());
        }
        assert!(
            analyze(
                report(vec![row("a", "[PASS]")]),
                &Options {
                    expected_files: Some(2),
                    ..Options::default()
                }
            )
            .is_err()
        );
    }

    #[settled_contract_test]
    fn report_analysis_normalizes_only_exact_known_statuses() {
        for (input, expected) in [
            ("[PASS]", "PASS"),
            ("fail", "FAIL"),
            (" [timeout] ", "TIMEOUT"),
        ] {
            assert_eq!(normalized_status(input).unwrap(), expected);
        }
        for input in ["NOT_PASS", "[PASS] extra", "[WARN]", ""] {
            assert!(normalized_status(input).is_err());
        }
    }

    #[settled_contract_test]
    fn report_analysis_missing_trace_is_unknown_not_parser_failure() {
        let summary = analyze(report(vec![row("a", "[TIMEOUT]")]), &Options::default()).unwrap();
        assert_eq!(summary.timeout_stages["unknown"], 1);
    }

    #[settled_contract_test]
    fn report_analysis_rejects_invalid_stage_sequences() {
        for trace in [
            json!([{"stage":"a","event":"completed"}]),
            json!([{"stage":"","event":"started"}]),
            json!([{"stage":"a","event":"bad"}]),
            json!([{"stage":"a","event":"started"},{"stage":"a","event":"started"}]),
        ] {
            let mut input = row("a", "[TIMEOUT]");
            input["stage_timing_trace"] = trace;
            assert!(analyze(report(vec![input]), &Options::default()).is_err());
        }
    }

    #[settled_contract_test]
    fn report_analysis_rejects_malformed_schema_and_negative_counts() {
        for input in ["{}", "not json", r#"{"total_files":-1}"#] {
            assert!(serde_json::from_str::<SavedReport>(input).is_err());
        }
        let mut input = row("a", "[PASS]");
        input["item_count"] = json!(-1);
        assert!(serde_json::from_value::<SavedRow>(input).is_err());
    }

    #[settled_contract_test]
    fn report_analysis_checks_item_sum_overflow() {
        let input = SavedReport {
            total_files: 2,
            total_pass: 2,
            total_fail: 0,
            total_items: 0,
            results: vec![
                SavedRow {
                    filename: "a".into(),
                    status: "[PASS]".into(),
                    item_count: u64::MAX,
                    family: None,
                    hint: String::new(),
                    stage_timing_trace: vec![],
                },
                SavedRow {
                    filename: "b".into(),
                    status: "[PASS]".into(),
                    item_count: 1,
                    family: None,
                    hint: String::new(),
                    stage_timing_trace: vec![],
                },
            ],
        };
        assert!(analyze(input, &Options::default()).is_err());
    }

    #[settled_contract_test]
    fn report_analysis_cli_rejects_ambiguous_or_mutating_arguments() {
        for args in [
            vec![],
            vec!["--fix"],
            vec!["a", "b"],
            vec!["a", "--output"],
            vec!["a", "--expected-files", "wrong"],
            vec!["a", "--json", "--json"],
        ] {
            assert!(parse_options(args.into_iter().map(str::to_owned)).is_err());
        }
        assert!(parse_options(["--help".to_owned()]).unwrap().help);
    }

    #[settled_contract_test]
    fn report_analysis_never_overwrites_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("d2item-report-contract-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let input = dir.join("input.json");
        let bytes =
            br#"{"total_files":0,"total_pass":0,"total_fail":0,"total_items":0,"results":[]}"#;
        fs::write(&input, bytes).unwrap();
        let options = Options {
            input: Some(input.clone()),
            output: Some(input.clone()),
            ..Options::default()
        };
        assert_eq!(execute(&options).unwrap_err().code, 3);
        assert_eq!(fs::read(&input).unwrap(), bytes);
        let output = dir.join("output.json");
        let options = Options {
            output: Some(output.clone()),
            ..options
        };
        execute(&options).unwrap();
        let before = fs::read(&output).unwrap();
        assert_eq!(execute(&options).unwrap_err().code, 3);
        assert_eq!(fs::read(&output).unwrap(), before);
        fs::remove_file(output).unwrap();
        fs::remove_file(input).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
