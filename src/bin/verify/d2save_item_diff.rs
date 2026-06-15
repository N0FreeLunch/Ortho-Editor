use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportIssue, ReportMetadata, ReportStatus};
use std::env;
use std::fs;
use std::process;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiffDetail {
    relative_offset: usize,
    absolute_offset_a: usize,
    absolute_offset_b: usize,
    value_a: u8,
    value_b: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ItemDiffReport {
    file_a: String,
    file_b: String,
    size_a: usize,
    size_b: usize,
    jm_a: Option<usize>,
    jm_b: Option<usize>,
    item_a_len: usize,
    item_b_len: usize,
    length_diff: isize,
    diff_count: usize,
    identical: bool,
    details: Vec<DiffDetail>,
}

fn find_first_jm(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'J' && bytes[i + 1] == b'M' {
            return Some(i);
        }
    }
    None
}

fn main() {
    let mut parser = ArgParser::new("d2save_item_diff").description(
        "Compares item sections of two D2R save files after aligning to the first JM marker",
    );

    parser.add_spec(ArgSpec::positional(
        "file_a",
        "path to the first save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "file_b",
        "path to the second save file (.d2s)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let mut out = OutputManager::new("d2save_item_diff", &parsed);

    let path_a = parsed.get("file_a").unwrap();
    let path_b = parsed.get("file_b").unwrap();

    let bytes_a = match fs::read(path_a) {
        Ok(b) => b,
        Err(e) => {
            let message = format!("[ERROR] Cannot read '{}': {}", path_a, e);
            if out.is_json() {
                let report = Report::<ItemDiffReport>::new(
                    ReportMetadata::new("d2save_item_diff", path_a, env!("CARGO_PKG_VERSION")),
                    ReportStatus::Fail,
                )
                .with_issues(vec![ReportIssue {
                    kind: "ReadError".to_string(),
                    message: message.clone(),
                    bit_offset: None,
                }]);
                out.json(&serde_json::to_string_pretty(&report).unwrap());
            } else {
                eprintln!("{}", message);
            }
            process::exit(1);
        }
    };
    let bytes_b = match fs::read(path_b) {
        Ok(b) => b,
        Err(e) => {
            let message = format!("[ERROR] Cannot read '{}': {}", path_b, e);
            if out.is_json() {
                let report = Report::<ItemDiffReport>::new(
                    ReportMetadata::new("d2save_item_diff", path_b, env!("CARGO_PKG_VERSION")),
                    ReportStatus::Fail,
                )
                .with_issues(vec![ReportIssue {
                    kind: "ReadError".to_string(),
                    message: message.clone(),
                    bit_offset: None,
                }]);
                out.json(&serde_json::to_string_pretty(&report).unwrap());
            } else {
                eprintln!("{}", message);
            }
            process::exit(1);
        }
    };

    let header = "=== d2save_item_diff (JM Aligned) ===";
    out.println(header);
    out.println(&format!("  A: {} ({} bytes)", path_a, bytes_a.len()));
    out.println(&format!("  B: {} ({} bytes)", path_b, bytes_b.len()));

    let jm_a_idx = find_first_jm(&bytes_a);
    let jm_b_idx = find_first_jm(&bytes_b);

    if jm_a_idx.is_none() || jm_b_idx.is_none() {
        let message = "[ERROR] Missing JM in one or both files.".to_string();
        if out.is_json() {
            let report = Report::<ItemDiffReport>::new(
                ReportMetadata::new("d2save_item_diff", path_a, env!("CARGO_PKG_VERSION")),
                ReportStatus::Fail,
            )
            .with_issues(vec![ReportIssue {
                kind: "MissingJM".to_string(),
                message: message.clone(),
                bit_offset: None,
            }]);
            out.json(&serde_json::to_string_pretty(&report).unwrap());
        } else {
            eprintln!("{}", message);
        }
        process::exit(1);
    }

    let a_start = jm_a_idx.unwrap();
    let b_start = jm_b_idx.unwrap();

    out.println(&format!("  First JM in A: offset {}", a_start));
    out.println(&format!("  First JM in B: offset {}", b_start));

    let items_a = &bytes_a[a_start..];
    let items_b = &bytes_b[b_start..];

    let common_len = items_a.len().min(items_b.len());
    let mut diffs = Vec::new();

    for i in 0..common_len {
        if items_a[i] != items_b[i] {
            diffs.push(i);
        }
    }

    let details: Vec<DiffDetail> = diffs
        .iter()
        .copied()
        .map(|i| DiffDetail {
            relative_offset: i,
            absolute_offset_a: a_start + i,
            absolute_offset_b: b_start + i,
            value_a: items_a[i],
            value_b: items_b[i],
        })
        .collect();

    let identical = diffs.is_empty() && items_a.len() == items_b.len();
    let report_payload = ItemDiffReport {
        file_a: path_a.clone(),
        file_b: path_b.clone(),
        size_a: bytes_a.len(),
        size_b: bytes_b.len(),
        jm_a: jm_a_idx,
        jm_b: jm_b_idx,
        item_a_len: items_a.len(),
        item_b_len: items_b.len(),
        length_diff: items_b.len() as isize - items_a.len() as isize,
        diff_count: diffs.len(),
        identical,
        details,
    };

    let status = if identical {
        ReportStatus::Ok
    } else {
        ReportStatus::Warn
    };
    let report = Report::new(
        ReportMetadata::new("d2save_item_diff", path_a, env!("CARGO_PKG_VERSION")),
        status,
    )
    .with_results(report_payload);

    out.println("");
    out.println("[ITEM SECTION DIFF SUMMARY]");
    out.println(&format!("  Items A length: {} bytes", items_a.len()));
    out.println(&format!("  Items B length: {} bytes", items_b.len()));

    if items_a.len() != items_b.len() {
        out.println(&format!(
            "  Length diff: {} bytes",
            items_b.len() as isize - items_a.len() as isize
        ));
    }

    out.println(&format!("  Total differences: {} bytes", diffs.len()));

    if out.is_json() {
        out.json(&serde_json::to_string_pretty(&report).unwrap());
        process::exit(if identical { 0 } else { 1 });
    }

    if identical {
        out.summary("  [IDENTICAL] The Item Sections (JM onwards) are 100% strictly identical.");
        process::exit(0);
    }

    out.println("");
    out.println("[DETAILS] (first 30 differences in Item Section)");
    out.println(&format!(
        "  {:>10}  {:>10}  {:>10}  {:>10}",
        "Rel Offset", "Abs A", "A (hex)", "B (hex)"
    ));
    out.println(&format!("  {:->10}  {:->10}  {:->10}  {:->10}", "", "", "", ""));
    for &i in diffs.iter().take(30) {
        let a_val = items_a[i];
        let b_val = items_b[i];
        let abs_a = a_start + i;
        out.println(&format!(
            "  {:>10}  {:>10}  0x{:02X} ({:>3})   0x{:02X} ({:>3})",
            i, abs_a, a_val, a_val, b_val, b_val
        ));
    }
    if diffs.len() > 30 {
        out.println(&format!("  ... and {} more diffs", diffs.len() - 30));
    }

    process::exit(1);
}
