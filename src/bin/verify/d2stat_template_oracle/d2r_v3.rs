//! Opt-in, read-only fixed-template observer for counted D2R v3 primary items.
use super::{TemplateOracleReport, evaluate_item_against_template};
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
                ));
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
        coverage_limit: "Counted primary items only; children and secondary sections excluded. Fixed-template coverage counts stat-ID presence and value ranges. Declared automatic checks separately require source parameter/value/occurrence; optional staffmod counts are observations, not missing-roll coverage. Unsupported overlap remains unadmitted. This is not canonical semantic-option inventory, spatial legality, game-load proof or Ftrue. Source and generated-template conflicts remain visible.",
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
        let old_intrinsic_len = receipt.report.intrinsic_evaluations.len();
        let old_len = receipt.report.item_evaluations.len();
        evaluate_item_against_template(&mut receipt.report, path, index, &semantic_view, false);
        if receipt.report.intrinsic_evaluations.len() > old_intrinsic_len {
            receipt
                .report
                .intrinsic_evaluations
                .last_mut()
                .unwrap()
                .item_index_contract = INDEX_CONTRACT;
        }
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
    receipt.metric_outcome = metric_outcome(&receipt.report);
    receipt
}

fn metric_outcome(report: &TemplateOracleReport) -> &'static str {
    if report.conditional_template_mismatches > 0 || report.intrinsic_mismatches > 0 {
        "red"
    } else if report.total_expected_stats == 0 && report.required_automatic_stats == 0 {
        "not_admitted"
    } else if report.missing_expected_stats > 0
        || report.extra_unmatched_stats > 0
        || report.value_in_range_stats != report.total_expected_stats
        || report.template_identity_conflicts > 0
        || report.matched_automatic_stats != report.required_automatic_stats
        || report.value_in_range_automatic_stats != report.required_automatic_stats
    {
        "red"
    } else {
        "green"
    }
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
        eprintln!(
            "D2R v3 template oracle: status={}, metric={}, primary={:?}, matched={}/{}, in-range={}, conditional-mismatches={}, errors={}",
            receipt.status,
            receipt.metric_outcome,
            receipt.top_level_item_count,
            receipt.report.matched_expected_stats,
            receipt.report.total_expected_stats,
            receipt.report.value_in_range_stats,
            receipt.report.conditional_template_mismatches,
            receipt.errors.len()
        );
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
        println!(
            "Usage: d2stat_template_oracle --dialect d2r-v3 --file <full.d2s> [--output <new.json>] [--json | --token-efficient]\nRead-only counted primary items; no corpus or fallback. Default/--dialect legacy retains the existing route."
        );
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
        assert!(
            template.stats.iter().all(|expected| row
                .parsed_stats
                .iter()
                .any(|(id, value)| *id == expected.stat_id
                    && *value >= expected.min
                    && *value <= expected.max))
        );
    }

    #[settled_contract_test]
    fn before_cap_cannot_become_manald() {
        let path = fixture("TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s", "act1");
        let receipt = observe(&fs::read(&path).unwrap(), &path);
        assert_eq!(receipt.status, "ok");
        assert!(
            receipt
                .report
                .item_evaluations
                .iter()
                .all(|row| row.unique_id != Some(121))
        );
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
        assert_eq!(receipt.metric_outcome, "green");
        assert_eq!(receipt.report.missing_expected_stats, 0);
        assert_eq!(receipt.report.extra_unmatched_stats, 0);
        assert_eq!(
            receipt.report.value_in_range_stats,
            receipt.report.total_expected_stats
        );
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

#[cfg(test)]
mod superior_tests {
    use super::*;
    use d2r_core::item::ItemProperty;
    use d2r_macros::settled_contract_test;
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    fn sources() -> &'static BTreeMap<&'static str, Item> {
        static SOURCES: OnceLock<BTreeMap<&'static str, Item>> = OnceLock::new();
        SOURCES.get_or_init(|| {
            let root = std::env::var_os("D2R_CORE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
            let mut cases = BTreeMap::new();
            for (file, targets) in [
                (
                    "TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s",
                    vec![(31, "ktr")],
                ),
                (
                    "TESTASSASSIN_Act1_Rogue_Lv7_Exp58640.d2s",
                    vec![(17, "wrb")],
                ),
                (
                    "TESTDRUID_Quest1_DenOfEvil_Done_PreAkara.d2s",
                    vec![(9, "jav"), (19, "ob1")],
                ),
            ] {
                let path = root
                    .join("tests/fixtures/savegames/gameplay/normal/act1")
                    .join(file);
                let bytes = fs::read(&path).unwrap();
                let items = Item::read_player_items_d2r_v3(&bytes, &HuffmanTree::new()).unwrap();
                for (index, code) in targets {
                    let item = &items[index];
                    assert_eq!(item.code.trim(), code);
                    cases.insert(code, item.clone());
                }
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
            cases
        })
    }

    fn evaluate(item: &Item) -> TemplateOracleReport {
        let mut report = TemplateOracleReport::default();
        let mut view = item.clone();
        view.properties.retain(|property| property.stat_id != 511);
        let original = view.clone();
        evaluate_item_against_template(
            &mut report,
            Path::new("controlled-source-view"),
            0,
            &view,
            false,
        );
        assert_eq!(view, original);
        report
    }

    fn percent(item: &Item, first: i32, second: i32) -> Item {
        let mut result = item.clone();
        result
            .properties
            .retain(|property| ![17, 18, 22, 24, 160].contains(&property.stat_id));
        for (id, value) in [(17, first), (18, second)] {
            result.properties.insert(
                (id - 17) as usize,
                ItemProperty::new(
                    id,
                    "controlled-damage".to_owned(),
                    0,
                    value,
                    value,
                    Default::default(),
                ),
            );
        }
        result.stats.properties = result.properties.clone();
        // The changed payload width determines this control's padding from the captured zero padding.
        let old_padding = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .unwrap()
            .alignment_padding
            .as_ref()
            .unwrap();
        assert!(old_padding.iter().all(|bit| !*bit));
        let old_width: usize = item
            .properties
            .iter()
            .filter(|p| [17, 18, 22, 24, 160].contains(&p.stat_id))
            .map(|p| (p.range.end - p.range.start) as usize)
            .sum();
        let new_width = 9 + [17, 18]
            .iter()
            .map(|id| {
                d2r_core::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|cost| cost.id == *id)
                    .unwrap()
                    .save_bits as usize
            })
            .sum::<usize>();
        let padding = (old_padding.len() + old_width % 8 + 8 - new_width % 8) % 8;
        result
            .body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .alignment_padding = Some(vec![false; padding]);
        let huffman = HuffmanTree::new();
        let bits = if result.header.is_socketed {
            result.to_bits_d2r_v3_socket_host(&huffman)
        } else {
            result.to_bits_d2r_v3_stored_weapon(&huffman)
        }
        .unwrap();
        assert_eq!(bits.len() % 8, 0);
        let mut bytes = b"JM\x01\x00".to_vec();
        for chunk in bits.chunks(8) {
            bytes.push(
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |byte, (index, bit)| byte | ((*bit as u8) << index)),
            );
        }
        let items = Item::read_counted_list_d2r_v3(&bytes, 0, &huffman, 105).unwrap();
        assert_eq!(items.len(), 1);
        items.into_iter().next().unwrap()
    }

    #[settled_contract_test]
    fn captured_superior_flat_channels_keep_source_qualified_intrinsics() {
        for (code, max_damage, channels, extra) in [
            ("ktr", 7, vec![22], Vec::<u32>::new()),
            ("wrb", 9, vec![22], vec![]),
            ("jav", 5, vec![22, 160], vec![]),
            ("ob1", 5, vec![22], vec![]),
        ] {
            let report = evaluate(&sources()[code]);
            let row = &report.item_evaluations[0];
            assert_eq!(report.missing_expected_stats, 0, "{code}");
            assert_eq!(
                report.value_in_range_stats, report.total_expected_stats,
                "{code}"
            );
            assert_eq!(report.conditional_template_mismatches, 0);
            assert!(row.conditional_diagnostics.is_empty());
            assert_eq!(row.extra_parsed_stat_ids, extra);
            assert_eq!(report.required_automatic_stats, usize::from(code == "ob1"));
            assert_eq!(
                report.admitted_staffmod_properties,
                usize::from(code == "ob1")
            );
            assert_eq!(report.intrinsic_mismatches, 0);
            if code == "ob1" {
                assert!(row.parsed_stats.contains(&(7, 4)));
                assert!(row.parsed_stats.contains(&(107, 2)));
            }
            let profile = row.superior_damage_expectation.as_ref().unwrap();
            assert_eq!(profile.branch, "flat_one");
            assert_eq!(profile.source_roll_range, (5, 15));
            assert_eq!(profile.base_max_damage, max_damage);
            assert_eq!(profile.flat_stat_ids, channels);
            assert!(profile.flat_fallback_possible);
            assert_eq!(
                metric_outcome(&report),
                if extra.is_empty() { "green" } else { "red" }
            );
        }
    }

    #[settled_contract_test]
    fn encoded_percentage_controls_reject_rounding_and_unequal_pairs() {
        let valid = evaluate(&percent(&sources()["ktr"], 15, 15));
        assert_eq!(metric_outcome(&valid), "green");
        assert_eq!(
            valid.item_evaluations[0]
                .superior_damage_expectation
                .as_ref()
                .unwrap()
                .admissible_percent_range,
            Some((15, 15))
        );
        let zero_bonus = evaluate(&percent(&sources()["ktr"], 5, 5));
        assert_eq!(metric_outcome(&zero_bonus), "red");
        assert!(zero_bonus.value_in_range_stats < zero_bonus.total_expected_stats);
        // Both values independently pass; only the shared-roll condition makes this red.
        let unequal = evaluate(&percent(&sources()["wrb"], 15, 14));
        assert_eq!(unequal.missing_expected_stats, 0);
        assert_eq!(unequal.extra_unmatched_stats, 0);
        assert_eq!(unequal.value_in_range_stats, unequal.total_expected_stats);
        assert_eq!(unequal.conditional_template_mismatches, 1);
        assert_eq!(metric_outcome(&unequal), "red");
        let diagnostic = &unequal.item_evaluations[0].conditional_diagnostics[0];
        assert_eq!(diagnostic.kind, "invalid_superior_percentage_pair");
        assert!(!diagnostic.message.is_empty() && !diagnostic.hint.is_empty());
        let impossible = evaluate(&percent(&sources()["jav"], 15, 15));
        assert_eq!(
            impossible.value_in_range_stats,
            impossible.total_expected_stats
        );
        assert_eq!(metric_outcome(&impossible), "red");
        assert!(
            impossible.item_evaluations[0]
                .superior_damage_expectation
                .as_ref()
                .unwrap()
                .admissible_percent_range
                .is_none()
        );
        assert!(
            impossible.item_evaluations[0]
                .conditional_diagnostics
                .iter()
                .any(|d| d.kind == "impossible_superior_percentage")
        );
    }

    #[settled_contract_test]
    fn malformed_carriers_and_unsupported_context_remain_visible() {
        let original = &sources()["ktr"];
        let mut flat = original.clone();
        flat.properties
            .iter_mut()
            .find(|p| p.stat_id == 22)
            .unwrap()
            .value = 2;
        assert_eq!(metric_outcome(&evaluate(&flat)), "red");
        flat.properties.retain(|p| p.stat_id != 22);
        assert_eq!(evaluate(&flat).missing_expected_stats, 1);
        let mut duplicate = original.clone();
        duplicate.properties.push(
            duplicate
                .properties
                .iter()
                .find(|p| p.stat_id == 22)
                .unwrap()
                .clone(),
        );
        assert_eq!(evaluate(&duplicate).conditional_template_mismatches, 1);
        let mut parameter = original.clone();
        parameter
            .properties
            .iter_mut()
            .find(|p| p.stat_id == 22)
            .unwrap()
            .param = 1;
        assert_eq!(evaluate(&parameter).conditional_template_mismatches, 1);
        let full_pair = percent(&sources()["wrb"], 15, 15);
        for broken in [17, 18] {
            let mut missing = full_pair.clone();
            missing.properties.retain(|p| p.stat_id != broken);
            assert_eq!(metric_outcome(&evaluate(&missing)), "red");
        }
        let mut duplicate = full_pair.clone();
        duplicate.properties.push(
            duplicate
                .properties
                .iter()
                .find(|p| p.stat_id == 17)
                .unwrap()
                .clone(),
        );
        assert_eq!(evaluate(&duplicate).conditional_template_mismatches, 1);
        let mut parameter = full_pair.clone();
        parameter
            .properties
            .iter_mut()
            .find(|p| p.stat_id == 18)
            .unwrap()
            .param = 1;
        assert_eq!(evaluate(&parameter).conditional_template_mismatches, 1);
        let mut mixed = full_pair.clone();
        mixed.properties.push(
            original
                .properties
                .iter()
                .find(|p| p.stat_id == 22)
                .unwrap()
                .clone(),
        );
        assert_eq!(evaluate(&mixed).extra_unmatched_stats, 1);
        let mut unsupported = original.clone();
        unsupported.code = "not-a-base".to_owned();
        assert_eq!(evaluate(&unsupported).conditional_template_mismatches, 1);
        let quality = super::super::QUALITY_ITEMS
            .iter()
            .find(|q| q.id == 6)
            .unwrap();
        let mut wrong_source = quality.clone();
        wrong_source.mod1_param = 1;
        assert!(
            !super::super::superior_damage_expectations(original, &wrong_source)
                .2
                .is_empty()
        );
    }

    #[settled_contract_test]
    fn profile_admission_and_report_merge_preserve_legacy_contracts() {
        let original = &sources()["ktr"];
        for variant in 0..4 {
            let mut item = original.clone();
            match variant {
                0 => item.header.save_is_alpha = true,
                1 => item.body.d2r_v3_auxiliary = None,
                2 => item.properties_complete = false,
                _ => item.header.is_identified = false,
            }
            let report = evaluate(&item);
            assert!(
                report.item_evaluations[0]
                    .superior_damage_expectation
                    .is_none()
            );
            assert_eq!(report.conditional_template_mismatches, 0);
            assert!(
                report.item_evaluations[0]
                    .conditional_diagnostics
                    .is_empty()
            );
            if variant == 3 {
                assert_eq!(report.total_expected_stats, 0);
            } else {
                assert!(report.item_evaluations[0].missing_stat_ids.contains(&17));
            }
        }
        let mut normal = original.clone();
        normal.header.quality = Some(d2r_core::item::ItemQuality::Normal);
        assert!(evaluate(&normal).item_evaluations.is_empty());
        let mut merged = evaluate(&sources()["ktr"]);
        merged.merge(evaluate(&percent(&sources()["wrb"], 15, 14)));
        assert_eq!(merged.conditional_template_mismatches, 1);
        assert_eq!(merged.item_evaluations.len(), 2);
        assert_eq!(metric_outcome(&merged), "red");
    }
}

#[cfg(test)]
mod intrinsic_source_tests {
    use super::*;
    use d2r_core::data::item_specs::ItemStatRange;
    use d2r_core::item::ItemQuality;
    use d2r_macros::settled_contract_test;

    fn path(relative: &str) -> PathBuf {
        std::env::var_os("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .join("tests/fixtures/savegames")
            .join(relative)
    }
    fn item(relative: &str, index: usize) -> Item {
        let bytes = fs::read(path(relative)).unwrap();
        Item::read_player_items_d2r_v3(&bytes, &HuffmanTree::new()).unwrap()[index].clone()
    }
    fn orb() -> Item {
        item(
            "gameplay/normal/act1/TESTDRUID_Quest1_DenOfEvil_Done_PreAkara.d2s",
            19,
        )
    }
    fn evaluate(item: &Item) -> TemplateOracleReport {
        let mut view = item.clone();
        view.properties.retain(|p| p.stat_id != 511);
        let original = view.clone();
        let mut report = TemplateOracleReport::default();
        evaluate_item_against_template(
            &mut report,
            Path::new("controlled-intrinsic-source"),
            0,
            &view,
            false,
        );
        assert_eq!(view, original);
        report
    }

    #[settled_contract_test]
    fn captured_intrinsics_have_independent_required_and_optional_counters() {
        let item = orb();
        let original = item.clone();
        let report = evaluate(&item);
        assert_eq!(
            (
                report.total_expected_stats,
                report.matched_expected_stats,
                report.value_in_range_stats
            ),
            (2, 2, 2)
        );
        assert_eq!(
            (
                report.required_automatic_stats,
                report.matched_automatic_stats,
                report.value_in_range_automatic_stats
            ),
            (1, 1, 1)
        );
        assert_eq!(report.admitted_staffmod_properties, 1);
        assert_eq!(report.intrinsic_mismatches, 0);
        let row = &report.intrinsic_evaluations[0].evaluation;
        assert_eq!(
            (
                row.auto_saved_id,
                row.auto_source_row_id,
                row.host_auto_group
            ),
            (Some(10), Some(9), Some(303))
        );
        assert_eq!((row.source_range_start, row.source_range_end), (9672, 9912));
        assert_eq!(row.staffmod_checks[0].param, 43);
        assert_eq!(row.staffmod_checks[0].observed[0].value, 2);
        assert!(report.item_evaluations[0].extra_parsed_stat_ids.is_empty());
        assert_eq!(
            report.item_evaluations[0].parsed_stats,
            [(7, 4), (22, 1), (75, 11), (107, 2)]
        );
        assert_eq!(metric_outcome(&report), "green");
        assert_eq!(item, original);
    }

    #[settled_contract_test]
    fn intrinsic_contradictions_stay_red_when_fixed_ratios_are_green() {
        let original = orb();
        let mut cases = Vec::new();
        for id in [0, 1, 11, 2047] {
            let mut x = original.clone();
            x.class_specific_bits = Some(id);
            cases.push((format!("auto_id{id}"), x));
        }
        let mut x = original.clone();
        x.has_class_specific_data = false;
        cases.push(("header_presence".into(), x));
        for value in [0, 6] {
            let mut x = original.clone();
            x.properties[0].value = value;
            cases.push((format!("auto_value{value}"), x));
        }
        let mut x = original.clone();
        x.properties.remove(0);
        cases.push(("missing_auto".into(), x));
        let mut x = original.clone();
        x.properties.push(x.properties[0].clone());
        cases.push(("duplicate_auto".into(), x));
        for value in [0, 4] {
            let mut x = original.clone();
            x.properties[3].value = value;
            cases.push((format!("staff_value{value}"), x));
        }
        for param in [0, 66, 60] {
            let mut x = original.clone();
            x.properties[3].param = param;
            cases.push((format!("staff_parameter{param}"), x));
        }
        let mut x = original.clone();
        x.properties.push(x.properties[3].clone());
        cases.push(("duplicate_staff".into(), x));
        let mut x = original.clone();
        for param in [35, 36, 37] {
            let mut p = x.properties[3].clone();
            p.param = param;
            x.properties.push(p);
        }
        cases.push(("four_staffmods".into(), x));
        assert_eq!(cases.len(), 16);
        for (name, item) in cases {
            let report = evaluate(&item);
            assert_eq!(report.missing_expected_stats, 0, "{name}");
            assert_eq!(
                report.value_in_range_stats, report.total_expected_stats,
                "{name}"
            );
            assert!(report.intrinsic_mismatches > 0, "{name}");
            assert_eq!(metric_outcome(&report), "red", "{name}");
        }
    }

    #[settled_contract_test]
    fn optional_absence_and_unidentified_headers_never_hydrate_rolls() {
        let mut no_staff = orb();
        no_staff.properties.retain(|p| p.stat_id != 107);
        let report = evaluate(&no_staff);
        assert_eq!(report.admitted_staffmod_properties, 0);
        assert_eq!(report.required_automatic_stats, 1);
        assert_eq!(metric_outcome(&report), "green");
        let mut hidden = orb();
        hidden.header.is_identified = false;
        hidden.is_identified = false;
        hidden.properties.clear();
        let report = evaluate(&hidden);
        assert_eq!(report.required_automatic_stats, 0);
        assert_eq!(report.admitted_staffmod_properties, 0);
        assert_eq!(
            report.intrinsic_evaluations[0].evaluation.status,
            "unidentified"
        );
        assert_eq!(metric_outcome(&report), "not_admitted");
        assert!(hidden.properties.is_empty());
        let mut leak = orb();
        leak.header.is_identified = false;
        let report = evaluate(&leak);
        assert_eq!(report.total_expected_stats, 0);
        assert_eq!(metric_outcome(&report), "red");
        assert_eq!(
            report.intrinsic_evaluations[0].evaluation.diagnostics[0].kind,
            "unidentified_property_leak"
        );
    }

    #[settled_contract_test]
    fn normal_and_rare_staffmods_are_observed_before_fixed_template_return() {
        for (file, index, class, count) in [
            (
                "gameplay/normal/act1/TESTASSASSIN_Act1_Rogue_Lv7_Exp58640.d2s",
                24,
                "sor",
                1,
            ),
            (
                "gameplay/normal/act5/TESTDRUID_Quest6_BaalIncomplete.d2s",
                16,
                "dru",
                2,
            ),
        ] {
            let report = evaluate(&item(file, index));
            assert_eq!(report.templated_items_found, 0);
            assert_eq!(report.admitted_staffmod_properties, count);
            assert_eq!(
                report.intrinsic_evaluations[0]
                    .evaluation
                    .host_staffmods_class,
                Some(class)
            );
            assert_eq!(report.intrinsic_mismatches, 0);
            assert_eq!(metric_outcome(&report), "not_admitted");
        }
    }

    #[settled_contract_test]
    fn intrinsic_partial_alpha_and_overlap_contexts_do_not_suppress_properties() {
        let original = orb();
        let mut alpha = original.clone();
        alpha.header.save_is_alpha = true;
        assert_eq!(evaluate(&alpha).intrinsic_items_observed, 0);
        let mut missing = original.clone();
        missing.body.d2r_v3_auxiliary = None;
        assert_eq!(evaluate(&missing).intrinsic_items_observed, 0);
        let mut unknown_level = original.clone();
        unknown_level.level = None;
        let unknown = super::super::intrinsics::evaluate(&unknown_level, false, &[]).unwrap();
        assert_eq!(unknown.evaluation.status, "partial");
        assert!(!unknown.admitted_property_indices.contains(&3));
        assert!(
            unknown
                .evaluation
                .unadmitted_contexts
                .contains(&"staffmod_class_layout_or_level")
        );
        let mut incomplete = original.clone();
        incomplete.properties_complete = false;
        let report = evaluate(&incomplete);
        assert_eq!(
            report.intrinsic_evaluations[0].evaluation.status,
            "unadmitted"
        );
        assert!(
            report.item_evaluations[0]
                .extra_parsed_stat_ids
                .contains(&7)
        );
        let mut rare = original.clone();
        rare.quality = Some(ItemQuality::Rare);
        let r = super::super::intrinsics::evaluate(&rare, false, &[]).unwrap();
        assert_eq!(r.evaluation.status, "partial");
        assert!(!r.admitted_property_indices.contains(&0));
        for known in [
            ItemStatRange {
                stat_id: 7,
                param: 0,
                min: 1,
                max: 5,
            },
            ItemStatRange {
                stat_id: 107,
                param: 43,
                min: 1,
                max: 3,
            },
        ] {
            let r = super::super::intrinsics::evaluate(&original, false, &[known]).unwrap();
            assert_eq!(r.evaluation.status, "partial");
            let index = if known.stat_id == 7 { 0 } else { 3 };
            assert!(!r.admitted_property_indices.contains(&index));
        }
    }

    #[settled_contract_test]
    fn intrinsic_merge_and_actual_v3_index_contracts_are_preserved() {
        let mut merged = evaluate(&orb());
        merged.merge(evaluate(&orb()));
        assert_eq!(merged.required_automatic_stats, 2);
        assert_eq!(merged.matched_automatic_stats, 2);
        assert_eq!(merged.value_in_range_automatic_stats, 2);
        assert_eq!(merged.admitted_staffmod_properties, 2);
        assert_eq!(merged.intrinsic_evaluations.len(), 2);
        assert_eq!(merged.intrinsic_mismatches, 0);
        assert_eq!(metric_outcome(&merged), "green");
        let p = path("gameplay/normal/act1/TESTDRUID_Quest1_DenOfEvil_Done_PreAkara.d2s");
        let receipt = observe(&fs::read(&p).unwrap(), &p);
        assert_eq!(receipt.status, "ok");
        assert_eq!(receipt.metric_outcome, "green");
        assert_eq!(receipt.report.total_expected_stats, 5);
        assert_eq!(receipt.report.extra_unmatched_stats, 0);
        let row = &receipt.report.intrinsic_evaluations[0];
        assert_eq!(row.item_index, 19);
        assert_eq!(row.item_index_contract, INDEX_CONTRACT);
    }
}
