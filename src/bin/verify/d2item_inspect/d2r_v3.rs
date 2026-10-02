//! Explicit, read-only bridge to the owned D2R v3 primary-list reader.
use d2r_core::domain::stats::ItemProperty;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::ParsedArgs;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;

const SCHEMA: &str = "d2item-inspect-d2r-v3/v1";
const ALLOWED: &[&str] = &[
    "file",
    "dialect",
    "json",
    "output",
    "token-efficient",
    "section-item",
    "coordinate-bit",
];

#[derive(Debug, PartialEq, Eq)]
enum Selector {
    Summary,
    Item(usize),
    Coordinate(u64),
}

fn error(kind: &str, message: impl Into<String>, hint: &str) -> Value {
    json!({"kind": kind, "message": message.into(), "hint": hint})
}

fn selection(args: &ParsedArgs) -> Result<Selector, Value> {
    let mut unsupported: Vec<_> = args
        .values
        .keys()
        .chain(
            args.flags
                .iter()
                .filter(|(_, enabled)| **enabled)
                .map(|(key, _)| key),
        )
        .filter(|key| !ALLOWED.contains(&key.as_str()))
        .cloned()
        .collect();
    unsupported.sort();
    if !unsupported.is_empty() {
        return Err(error("unsupported_option", format!("Unsupported D2R v3 options: {}", unsupported.join(", ")),
            "Use --section-item or --coordinate-bit; legacy marker/phase probes are not D2R v3 probes."));
    }
    if args.values.values().any(|values| values.len() != 1) {
        return Err(error(
            "duplicate_option",
            "Supply each value option exactly once",
            "Remove repeated selectors or dialect values.",
        ));
    }
    if args.get("file").is_none() {
        return Err(error(
            "missing_file",
            "A full .d2s file is required",
            "Pass a single file, not --corpus-dir.",
        ));
    }
    if args.is_set("token-efficient") && args.get("output").is_none() {
        return Err(error("missing_output", "D2R v3 token-efficient mode requires an explicit output path",
            "Use --output with a new absolute artifact path; no cwd-based private path is inferred."));
    }
    match (args.get("section-item"), args.get("coordinate-bit")) {
        (Some(_), Some(_)) => Err(error(
            "selector_conflict",
            "Use only one selector",
            "Choose --section-item or --coordinate-bit.",
        )),
        (Some(value), None) => value.parse().map(Selector::Item).map_err(|_| {
            error(
                "invalid_selector",
                "section-item must be a non-negative integer",
                "Use a zero-based counted primary-list index.",
            )
        }),
        (None, Some(value)) => value.parse().map(Selector::Coordinate).map_err(|_| {
            error(
                "invalid_selector",
                "coordinate-bit must be a non-negative integer",
                "Use an absolute file bit coordinate.",
            )
        }),
        (None, None) => Ok(Selector::Summary),
    }
}

fn property_list(kind: &str, ordinal: usize, properties: &[ItemProperty]) -> Value {
    json!({
        "kind": kind, "ordinal": ordinal,
        "stats": properties.iter().filter(|property| property.stat_id != 511).map(|property| json!({
            "id": property.stat_id, "name": property.name, "parameter": property.param,
            "raw_value": property.raw_value, "value": property.value,
            "range": {"start": property.range.start, "end": property.range.end},
            "range_basis": "parser-owned-absolute-span; companion stats may omit an ID",
            "is_opaque": property.is_opaque,
        })).collect::<Vec<_>>(),
        "control_markers": properties.iter().filter(|property| property.stat_id == 511).map(|property| json!({
            "id": property.stat_id, "range": {"start": property.range.start, "end": property.range.end},
        })).collect::<Vec<_>>(),
    })
}

fn record(item: &Item) -> Value {
    let body_end = item
        .socketed_items
        .first()
        .map_or(item.range.end, |child| child.range.start);
    let base = property_list("base", 0, &item.properties);
    let mut lists = vec![base.clone()];
    if !item.runeword_attributes.is_empty() {
        lists.push(property_list("runeword", 0, &item.runeword_attributes));
    }
    lists.extend(
        item.set_attributes
            .iter()
            .enumerate()
            .map(|(ordinal, properties)| property_list("set", ordinal, properties)),
    );
    json!({
        "code": item.code.trim(),
        "quality": item.header.quality.map(|quality| format!("{quality:?}")),
        "unique_id": item.unique_id,
        "is_runeword": item.header.is_runeword,
        "range": {"start": item.range.start, "end": item.range.end},
        "body_range": {"start": item.range.start, "end": body_end},
        "bit_length": item.range.end.saturating_sub(item.range.start),
        "mode": item.header.mode, "location": item.body.location,
        "x": item.body.x, "y": item.body.y,
        "properties_complete": item.properties_complete,
        "stats": base["stats"],
        "control_markers": base["control_markers"],
        "property_lists": lists,
        "list_basis": "Parser-owned lists in stored order; ordinals are not active Set bonus tiers. Top-level stats/control_markers alias base only.",
        "socket_children": item.socketed_items.iter().map(record).collect::<Vec<_>>(),
    })
}

fn coordinate_owner<'a>(item: &'a Item, bit: u64, path: &mut Vec<usize>) -> Option<&'a Item> {
    // Parent spans include children; the deepest owned body is the coordinate target.
    for (index, child) in item.socketed_items.iter().enumerate() {
        path.push(index);
        if let Some(owner) = coordinate_owner(child, bit, path) {
            return Some(owner);
        }
        path.pop();
    }
    (item.range.start <= bit && bit < item.range.end).then_some(item)
}

fn envelope(args: &ParsedArgs) -> Value {
    json!({
        "schema_version": SCHEMA, "dialect": "d2r-v3", "requested_dialect": args.get("dialect"),
        "evidence_surface": "controlled-decoder", "fixture": args.get("file"),
        "status": "error", "top_level_item_count": Value::Null, "selected": [], "errors": [],
        "coverage_limit": "Owned primary list and existing decoder fields only; no independent semantic oracle, game-load proof or new-option claim.",
    })
}

fn report(bytes: &[u8], args: &ParsedArgs, selector: &Selector) -> Value {
    let mut result = envelope(args);
    let items = match Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()) {
        Ok(items) => items,
        Err(failure) => {
            result["errors"] = json!([{
                "kind": "primary_list_parse_failure", "message": failure.to_string(),
                "bit_offset": failure.bit_offset, "context": failure.context_stack,
                "hint": failure.hint.unwrap_or_else(|| "Preserve the input and verify the full-save prefix and explicit dialect; do not scan for a later JM.".to_string()),
            }]);
            return result;
        }
    };
    result["top_level_item_count"] = json!(items.len());
    let selected = match selector {
        Selector::Summary => None,
        Selector::Item(index) => {
            result["selector"] = json!({"section_item_index": index});
            items.get(*index).map(|item| (vec![*index], item))
        }
        Selector::Coordinate(bit) => {
            result["selector"] = json!({"coordinate_bit": bit});
            items.iter().enumerate().find_map(|(index, item)| {
                let mut path = vec![index];
                coordinate_owner(item, *bit, &mut path).map(|owner| (path, owner))
            })
        }
    };
    if let Some((path, item)) = selected {
        result["selected"] = json!([{"ownership_path": path, "item": record(item)}]);
    } else if *selector != Selector::Summary {
        result["errors"] = json!([error("selector_not_owned", "No counted primary item or socket child owns the requested selector",
            "Check the primary-list index or absolute bit coordinate. Secondary containers and padding are not fallback targets.")]);
        return result;
    }
    result["status"] = json!("ok");
    result
}

fn emit(result: &Value, args: &ParsedArgs) -> std::io::Result<()> {
    if let Some(path) = args.get("output") {
        // A forensic receipt must never silently replace evidence or hide an I/O failure.
        let mut destination = OpenOptions::new().write(true).create_new(true).open(path)?;
        serde_json::to_writer_pretty(&mut destination, result)?;
        destination.write_all(b"\n")?;
        destination.flush()?;
    }
    if args.is_json() && (!args.is_set("token-efficient") || args.get("output").is_none()) {
        println!("{result}");
    } else {
        let summary = format!(
            "D2R v3: status={}, primary={}, selected={}, errors={}",
            result["status"].as_str().unwrap_or("error"),
            result["top_level_item_count"],
            result["selected"].as_array().map_or(0, Vec::len),
            result["errors"].as_array().map_or(0, Vec::len)
        );
        if args.is_json() {
            eprintln!("{summary}");
        } else {
            println!("{summary}");
            for selected in result["selected"].as_array().into_iter().flatten() {
                let item = &selected["item"];
                println!(
                    "  {} {} bits {}-{} ({} stats)",
                    item["code"],
                    item["quality"],
                    item["range"]["start"],
                    item["range"]["end"],
                    item["stats"].as_array().map_or(0, Vec::len)
                );
            }
            for issue in result["errors"].as_array().into_iter().flatten() {
                eprintln!("{}; hint: {}", issue["message"], issue["hint"]);
            }
        }
    }
    Ok(())
}

pub(super) fn run_if_requested(args: &ParsedArgs) -> Option<i32> {
    let dialect = args.get("dialect")?;
    if dialect == "legacy" {
        return None;
    }
    let mut result = envelope(args);
    if dialect != "d2r-v3" {
        result["errors"] = json!([error(
            "unknown_dialect",
            format!("Unknown dialect: {dialect}"),
            "Use --dialect d2r-v3 or omit the option for the unchanged legacy route."
        )]);
    } else {
        match selection(args) {
            Err(issue) => result["errors"] = json!([issue]),
            Ok(selector) => match fs::read(args.get("file").expect("validated file")) {
                Ok(bytes) => result = report(&bytes, args, &selector),
                Err(issue) => {
                    result["errors"] = json!([error(
                        "input_io_failure",
                        issue.to_string(),
                        "Check the full-save input path and permissions."
                    )])
                }
            },
        }
    }
    if let Err(issue) = emit(&result, args) {
        let failure = error("output_io_failure", issue.to_string(), "Use a new output filename under an existing writable parent; no overwrite or silent fallback is allowed.");
        eprintln!(
            "{}",
            json!({"schema_version": SCHEMA, "status": "error", "errors": [failure]})
        );
        return Some(2);
    }
    Some(if result["status"] == "ok" { 0 } else { 2 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;
    use std::collections::HashMap;

    const AFTER: &str = "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s";
    const BEFORE: &str = "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s";
    const BETA: &str = "tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s";

    fn args() -> ParsedArgs {
        ParsedArgs {
            values: HashMap::from([
                ("file".into(), vec![AFTER.into()]),
                ("dialect".into(), vec!["d2r-v3".into()]),
            ]),
            flags: HashMap::new(),
        }
    }

    fn tuples(list: &Value) -> Vec<Value> {
        list["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|stat| {
                json!([
                    stat["id"],
                    stat["value"],
                    stat["raw_value"],
                    stat["parameter"],
                    stat["range"]["start"],
                    stat["range"]["end"]
                ])
            })
            .collect()
    }

    fn assert_base_aliases(item: &Value) {
        assert_eq!(item["property_lists"][0]["kind"], "base");
        assert_eq!(item["property_lists"][0]["ordinal"], 0);
        assert_eq!(item["stats"], item["property_lists"][0]["stats"]);
        assert_eq!(
            item["control_markers"],
            item["property_lists"][0]["control_markers"]
        );
        assert!(item["list_basis"]
            .as_str()
            .unwrap()
            .contains("not active Set bonus tiers"));
    }

    #[settled_contract_test]
    fn d2r_v3_auxiliary_runeword_projection() {
        let result = report(
            &fs::read(BETA).unwrap(),
            &args(),
            &Selector::Coordinate(9240),
        );
        assert_eq!(result["status"], "ok");
        assert_eq!(result["top_level_item_count"], 16);
        let item = &result["selected"][0]["item"];
        assert_base_aliases(item);
        assert_eq!(item["stats"], json!([]));
        assert_eq!(item["property_lists"].as_array().unwrap().len(), 2);
        let list = &item["property_lists"][1];
        assert_eq!(list["kind"], "runeword");
        assert_eq!(list["ordinal"], 0);
        assert_eq!(
            tuples(list),
            vec![
                json!([17, 60, 60, 0, 9416, 9434]),
                json!([18, 60, 60, 0, 9434, 9443]),
                json!([83, 2, 2, 7, 9443, 9458]),
                json!([198, 10, 10, 25551, 9458, 9490]),
                json!([201, 2, 2, 24778, 9490, 9522]),
            ]
        );
        assert_eq!(
            list["control_markers"],
            json!([{"id":511,"range":{"start":9522,"end":9531}}])
        );
        assert_eq!(item["body_range"], json!({"start":9240,"end":9536}));
        let children = item["socket_children"].as_array().unwrap();
        assert_eq!(children.len(), 3);
        for child in children {
            assert_base_aliases(child);
        }
        let manald = report(
            &fs::read(AFTER).unwrap(),
            &args(),
            &Selector::Coordinate(11320),
        );
        let ring = &manald["selected"][0]["item"];
        assert_base_aliases(ring);
        assert_eq!(ring["property_lists"].as_array().unwrap().len(), 1);
        assert_eq!(ring["stats"].as_array().unwrap().len(), 4);
    }

    #[settled_contract_test]
    fn d2r_v3_auxiliary_set_projection() {
        let fixture =
            "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s";
        let result = report(
            &fs::read(fixture).unwrap(),
            &args(),
            &Selector::Coordinate(9616),
        );
        assert_eq!(result["status"], "ok");
        assert_eq!(result["top_level_item_count"], 38);
        let item = &result["selected"][0]["item"];
        assert_base_aliases(item);
        assert_eq!(item["property_lists"].as_array().unwrap().len(), 2);
        assert_eq!(item["quality"], "Set");
        assert_eq!(item["unique_id"], 29);
        assert_eq!(item["range"], json!({"start":9616,"end":9832}));
        assert_eq!(
            tuples(&item["property_lists"][0]),
            vec![
                json!([34, 2, 2, 0, 9759, 9774]),
                json!([60, 6, 6, 0, 9774, 9790]),
            ]
        );
        let list = &item["property_lists"][1];
        assert_eq!(list["kind"], "set");
        assert_eq!(list["ordinal"], 0);
        assert_eq!(tuples(list), vec![json!([0, 10, 42, 0, 9799, 9816])]);
        assert_eq!(
            list["control_markers"],
            json!([{"id":511,"range":{"start":9816,"end":9825}}])
        );
    }

    #[settled_contract_test]
    fn d2r_v3_auxiliary_order_empty_and_opaque_occurrences() {
        let mut items =
            Item::read_player_items_d2r_v3(&fs::read(BETA).unwrap(), &HuffmanTree::new()).unwrap();
        let mut item = items.remove(13);
        let mut opaque = item.runeword_attributes[0].clone();
        opaque.is_opaque = true;
        opaque.name = "synthetic_opaque".into();
        item.properties.clear();
        item.runeword_attributes.clear();
        item.set_attributes = vec![
            vec![],
            vec![opaque.clone(), opaque.clone()],
            vec![opaque.clone()],
        ];
        let output = record(&item);
        assert_base_aliases(&output);
        let lists = output["property_lists"].as_array().unwrap();
        assert_eq!(lists.len(), 4);
        assert_eq!(lists[0]["stats"], json!([]));
        assert_eq!(lists[0]["control_markers"], json!([]));
        for (ordinal, list) in lists[1..].iter().enumerate() {
            assert_eq!(list["kind"], "set");
            assert_eq!(list["ordinal"], ordinal);
            assert_eq!(list["control_markers"], json!([]));
        }
        assert_eq!(lists[1]["stats"], json!([]));
        let stats = lists[2]["stats"].as_array().unwrap();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0], stats[1]);
        assert_eq!(stats[0]["is_opaque"], true);
        assert_eq!(stats[0]["name"], "synthetic_opaque");
        assert_eq!(
            stats[0]["range"],
            json!({"start":opaque.range.start,"end":opaque.range.end})
        );
        assert_eq!(lists[3]["stats"].as_array().unwrap().len(), 1);
        assert_eq!(lists[3]["stats"][0], stats[0]);
    }

    #[settled_contract_test]
    fn d2r_v3_consumer_manald_values_and_absolute_spans() {
        let result = report(
            &fs::read(AFTER).unwrap(),
            &args(),
            &Selector::Coordinate(11320),
        );
        assert_eq!(result["status"], "ok");
        assert_eq!(result["top_level_item_count"], 33);
        let item = &result["selected"][0]["item"];
        assert_eq!(item["code"], "rin");
        assert_eq!(item["quality"], "Unique");
        assert_eq!(item["unique_id"], 121);
        assert_eq!(item["range"], json!({"start":11320,"end":11536}));
        assert_eq!(item["bit_length"], 216);
        assert_eq!(
            item["control_markers"],
            json!([{"id":511,"range":{"start":11524,"end":11533}}])
        );
        let expected = [
            (7, 20, 52, 11458, 11476),
            (27, 20, 20, 11476, 11493),
            (62, 6, 6, 11493, 11509),
            (74, 8, 38, 11509, 11524),
        ];
        let stats = item["stats"].as_array().unwrap();
        assert_eq!(stats.len(), expected.len());
        for (stat, (id, value, raw, start, end)) in stats.iter().zip(expected) {
            assert_eq!(stat["id"], id);
            assert_eq!(stat["value"], value);
            assert_eq!(stat["raw_value"], raw);
            assert_eq!(stat["range"], json!({"start":start,"end":end}));
        }
        let index = result["selected"][0]["ownership_path"][0].as_u64().unwrap() as usize;
        let indexed = report(&fs::read(AFTER).unwrap(), &args(), &Selector::Item(index));
        assert_eq!(indexed["selected"], result["selected"]);
    }

    #[settled_contract_test]
    fn d2r_v3_consumer_before_cap_and_beta_nested_children() {
        let before = report(
            &fs::read(BEFORE).unwrap(),
            &args(),
            &Selector::Coordinate(8592),
        );
        assert_eq!(before["top_level_item_count"], 33);
        assert_eq!(before["selected"][0]["item"]["code"], "cap");
        assert_eq!(before["selected"][0]["item"]["bit_length"], 168);
        let bytes = fs::read(BETA).unwrap();
        let beta = report(&bytes, &args(), &Selector::Coordinate(9240));
        assert_eq!(beta["top_level_item_count"], 16);
        assert_eq!(
            beta["selected"][0]["item"]["body_range"],
            json!({"start":9240,"end":9536})
        );
        assert_eq!(
            beta["selected"][0]["item"]["range"],
            json!({"start":9240,"end":9800})
        );
        assert_eq!(
            beta["selected"][0]["item"]["socket_children"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let child = report(&bytes, &args(), &Selector::Coordinate(9536));
        assert_eq!(child["selected"][0]["item"]["code"], "r15");
        assert_eq!(
            child["selected"][0]["ownership_path"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let next = report(&bytes, &args(), &Selector::Coordinate(9800));
        assert_eq!(
            next["selected"][0]["ownership_path"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[settled_contract_test]
    fn d2r_v3_consumer_empty_and_malformed_primary_never_fall_back() {
        let mut bytes = fs::read(BETA).unwrap();
        bytes[918..920].copy_from_slice(&0u16.to_le_bytes());
        let empty = report(&bytes, &args(), &Selector::Summary);
        assert_eq!(empty["status"], "ok");
        assert_eq!(empty["top_level_item_count"], 0);
        assert_eq!(
            report(&bytes, &args(), &Selector::Coordinate(7360))["errors"][0]["kind"],
            "selector_not_owned"
        );
        for offset in [0, 833, 884, 916] {
            let mut malformed = fs::read(BETA).unwrap();
            malformed[offset] = 0;
            let failure = report(&malformed, &args(), &Selector::Summary);
            assert_eq!(failure["errors"][0]["kind"], "primary_list_parse_failure");
            assert!(failure["top_level_item_count"].is_null());
        }
    }

    #[settled_contract_test]
    fn d2r_v3_consumer_argument_matrix_and_summary_only() {
        let base = args();
        assert_eq!(selection(&base).unwrap(), Selector::Summary);
        for key in [
            "bit-offset",
            "corpus-dir",
            "marker-section",
            "trace-segments",
            "section-context",
            "compare-file",
            "reparse-limit-bits",
        ] {
            let mut invalid = args();
            invalid.values.insert(key.into(), vec!["1".into()]);
            assert_eq!(
                selection(&invalid).unwrap_err()["kind"],
                "unsupported_option"
            );
        }
        for value in ["-1", "NaN", "184467440737095516160"] {
            let mut invalid = args();
            invalid
                .values
                .insert("coordinate-bit".into(), vec![value.into()]);
            assert_eq!(selection(&invalid).unwrap_err()["kind"], "invalid_selector");
        }
        let mut conflict = args();
        conflict
            .values
            .insert("section-item".into(), vec!["0".into()]);
        conflict
            .values
            .insert("coordinate-bit".into(), vec!["11320".into()]);
        assert_eq!(
            selection(&conflict).unwrap_err()["kind"],
            "selector_conflict"
        );
        let mut duplicate = args();
        duplicate
            .values
            .get_mut("dialect")
            .unwrap()
            .push("legacy".into());
        assert_eq!(
            selection(&duplicate).unwrap_err()["kind"],
            "duplicate_option"
        );
        let mut missing = args();
        missing.flags.insert("token-efficient".into(), true);
        assert_eq!(selection(&missing).unwrap_err()["kind"], "missing_output");
        let summary = report(&fs::read(AFTER).unwrap(), &base, &Selector::Summary);
        assert_eq!(summary["selected"], json!([]));
        assert_eq!(summary["top_level_item_count"], 33);
        assert_eq!(
            report(
                &fs::read(AFTER).unwrap(),
                &base,
                &Selector::Item(usize::MAX)
            )["errors"][0]["kind"],
            "selector_not_owned"
        );
    }
}
