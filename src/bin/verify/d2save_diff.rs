use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use std::env;
use std::fs;
use std::process;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffDetail {
    pub offset: usize,
    pub val_a: u8,
    pub val_b: u8,
    pub region: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffResult {
    pub file_a: String,
    pub file_b: String,
    pub size_a: usize,
    pub size_b: usize,
    pub jm_a: Option<usize>,
    pub jm_b: Option<usize>,
    pub header_diff_count: usize,
    pub item_diff_count: usize,
    pub total_diff_count: usize,
    pub length_diff: isize,
    pub details: Vec<DiffDetail>,
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
    let mut parser = ArgParser::new("d2save_diff")
        .description("Compares two D2R save files and highlights byte-level differences in header and item sections");

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

    let mut om = OutputManager::new("d2save_diff", &parsed);

    let path_a = parsed.get("file_a").unwrap();
    let path_b = parsed.get("file_b").unwrap();

    let bytes_a = match fs::read(path_a) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Cannot read '{}': {}", path_a, e);
            process::exit(1);
        }
    };
    let bytes_b = match fs::read(path_b) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Cannot read '{}': {}", path_b, e);
            process::exit(1);
        }
    };

    let jm_a = find_first_jm(&bytes_a);
    let jm_b = find_first_jm(&bytes_b);

    let common_len = bytes_a.len().min(bytes_b.len());
    let mut diff_offsets = Vec::new();

    for i in 0..common_len {
        if bytes_a[i] != bytes_b[i] {
            diff_offsets.push(i);
        }
    }

    let header_end = jm_a.unwrap_or(0).min(jm_b.unwrap_or(0));
    let header_diffs: Vec<usize> = diff_offsets
        .iter()
        .copied()
        .filter(|&i| i < header_end)
        .collect();
    let item_diffs: Vec<usize> = diff_offsets
        .iter()
        .copied()
        .filter(|&i| i >= header_end)
        .collect();

    let details: Vec<DiffDetail> = diff_offsets
        .iter()
        .take(100) // Collect more details for JSON, but maybe limit for sanity
        .map(|&i| {
            let a_val = bytes_a[i];
            let b_val = bytes_b[i];
            let region = if i < header_end {
                "header".to_string()
            } else {
                "items".to_string()
            };
            DiffDetail {
                offset: i,
                val_a: a_val,
                val_b: b_val,
                region,
            }
        })
        .collect();

    let result = DiffResult {
        file_a: path_a.clone(),
        file_b: path_b.clone(),
        size_a: bytes_a.len(),
        size_b: bytes_b.len(),
        jm_a,
        jm_b,
        header_diff_count: header_diffs.len(),
        item_diff_count: item_diffs.len(),
        total_diff_count: diff_offsets.len(),
        length_diff: bytes_b.len() as isize - bytes_a.len() as isize,
        details,
    };

    let metadata = ReportMetadata::new("d2save_diff", path_a, "0.1.0");
    let status = if result.total_diff_count == 0 && result.length_diff == 0 {
        ReportStatus::Ok
    } else {
        ReportStatus::Warn
    };

    let report = Report::new(metadata, status).with_results(result.clone());

    if om.is_json() {
        let json = serde_json::to_string_pretty(&report).unwrap();
        om.json(&json);
    } else {
        om.println("=== d2save_diff ===");
        om.println(&format!("  A: {} ({} bytes)", path_a, bytes_a.len()));
        om.println(&format!("  B: {} ({} bytes)", path_b, bytes_b.len()));
        om.println(&format!("  First JM in A: {:?}", jm_a));
        om.println(&format!("  First JM in B: {:?}", jm_b));

        om.println("");
        om.println("[DIFF SUMMARY]");
        om.println(&format!(
            "  Header diffs  (0..{header_end}): {} bytes",
            result.header_diff_count
        ));
        om.println(&format!(
            "  Item diffs    ({header_end}..{}): {} bytes",
            common_len, result.item_diff_count
        ));
        if result.length_diff != 0 {
            om.println(&format!("  Length diff: {} bytes", result.length_diff));
        }

        om.println("");
        om.println("[DETAILS] (first 30 diffs)");
        om.println(&format!(
            "  {:>8}  {:>10}  {:>10}",
            "Offset", "A (hex)", "B (hex)"
        ));
        om.println(&format!("  {:->8}  {:->10}  {:->10}", "", "", ""));
        for detail in result.details.iter().take(30) {
            om.println(&format!(
                "  {:>8}  0x{:02X} ({:>3})   0x{:02X} ({:>3})   [{}]",
                detail.offset,
                detail.val_a,
                detail.val_a,
                detail.val_b,
                detail.val_b,
                detail.region
            ));
        }
        if result.total_diff_count > 30 {
            om.println(&format!(
                "  ... and {} more diffs",
                result.total_diff_count - 30
            ));
        }
        if result.total_diff_count == 0 && result.length_diff == 0 {
            om.println("  [IDENTICAL] No differences found.");
        }
    }
}
