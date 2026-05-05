use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::save_integrity::verify_save_integrity;
use d2r_core::verify::symmetry::{calculate_symmetry_diff, ItemDiff};
use d2r_core::verify::FailureCategory;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

#[derive(Debug, Serialize, Clone)]
struct MismatchRow {
    file: String,
    item_label: String,
    code: String,
    mismatch_type: String,
    segment: String,
    first_mismatch_offset: Option<usize>,
}

#[derive(Debug, Serialize)]
struct BatchEntry {
    file: String,
    integrity_ok: bool,
    symmetry_ok: bool,
    baseline_match: Option<bool>,
    baseline_mismatch_count: usize,
    shadow_match: Option<bool>,
    shadow_mismatch_count: usize,
    failure_category: Option<FailureCategory>,
    integrity_issues: usize,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BatchReport {
    dir: String,
    total_files: usize,
    success_files: usize,
    failed_files: usize,
    entries: Vec<BatchEntry>,
    mismatch_rows: Vec<MismatchRow>,
}

fn collect_mismatches(item: &ItemDiff, parent_label: &str, file_name: &str, rows: &mut Vec<MismatchRow>) {
    let current_label = if parent_label.is_empty() {
        item.label.clone()
    } else {
        format!("{} -> {}", parent_label, item.label)
    };

    if !item.is_match {
        rows.push(MismatchRow {
            file: file_name.to_string(),
            item_label: current_label.clone(),
            code: item.code.clone(),
            mismatch_type: item.mismatch_type.clone().unwrap_or_default(),
            segment: item.segment.clone().unwrap_or_default(),
            first_mismatch_offset: item.first_mismatch_offset.map(|o| o as usize),
        });
    }

    for child in &item.children {
        collect_mismatches(child, &current_label, file_name, rows);
    }
}

fn audit_baseline(original_bytes: &[u8], actual_bytes: &[u8]) -> (bool, usize) {
    if original_bytes.len() != actual_bytes.len() {
        return (false, 1);
    }

    let mut mismatch_count = 0;
    for (i, (&orig, &recon)) in original_bytes.iter().zip(actual_bytes.iter()).enumerate() {
        if orig != recon {
            let xor = orig ^ recon;
            for bit in 0..8 {
                if (xor >> bit) & 1 != 0 {
                    let bit_offset = i * 8 + bit;
                    // Justified noise: Header Checksum (bytes 12-15, bits 96-127)
                    if !(96..=127).contains(&bit_offset) {
                        mismatch_count += 1;
                    }
                }
            }
        }
    }
    (mismatch_count == 0, mismatch_count)
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_batch");
    parser.add_spec(ArgSpec::option("dir", None, Some("dir"), "Directory that contains .d2s files"));
    parser.add_spec(ArgSpec::option("report", None, Some("report"), "Output Markdown report file"));
    parser.add_spec(ArgSpec::flag("refexp", None, Some("refexp"), "Use experimental next-gen engine and show shadow comparison"));

    let parsed = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let dir_str = parsed.get("dir").ok_or_else(|| anyhow::anyhow!("--dir <PATH> is required"))?;
    let report_path = parsed.get("report");
    let mut entries = Vec::new();
    let mut all_mismatch_rows = Vec::new();

    let dir_path = Path::new(dir_str);
    let original_dir = dir_path.parent().map(|p| p.join("original")).unwrap_or_else(|| PathBuf::from("tests/fixtures/savegames/original"));

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("d2s") {
            continue;
        }

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                entries.push(BatchEntry {
                    file: file_name,
                    integrity_ok: false,
                    symmetry_ok: false,
                    baseline_match: None,
                    baseline_mismatch_count: 0,
                    shadow_match: None,
                    shadow_mismatch_count: 0,
                    failure_category: Some(FailureCategory::ToolError),
                    integrity_issues: 0,
                    error: Some(format!("Failed to read file: {}", e)),
                });
                continue;
            }
        };

        // Phase 1: Integrity
        let (integrity_report, integrity_failed) = verify_save_integrity(&file_name, &bytes);
        
        // Phase 2: Symmetry
        let symmetry_res = calculate_symmetry_diff(&bytes, None, true);
        
        // Phase 3: Baseline
        let mut baseline_match = None;
        let mut baseline_mismatch_count = 0;
        let original_path = original_dir.join(&file_name);
        if original_path.exists() {
            if let Ok(orig_bytes) = fs::read(original_path) {
                let (m, c) = audit_baseline(&orig_bytes, &bytes);
                baseline_match = Some(m);
                baseline_mismatch_count = c;
            }
        }

        // Phase 4: Shadow Audit
        let mut shadow_match = None;
        let mut shadow_mismatch_count = 0;
        if parsed.is_set("refexp") {
            let shadow = d2r_core::verify::run_shadow_audit(&file_name, &bytes);
            shadow_match = Some(shadow.is_match);
            shadow_mismatch_count = shadow.mismatch_count;
        }

        let mut failure_category = None;
        let mut symmetry_ok = false;
        let mut error = None;

        match symmetry_res {
            Ok(symmetry) => {
                symmetry_ok = symmetry.success;
                if integrity_failed {
                    failure_category = Some(FailureCategory::Integrity);
                } else if !symmetry.success {
                    failure_category = Some(FailureCategory::Symmetry);
                    for item in &symmetry.items {
                        collect_mismatches(item, "", &file_name, &mut all_mismatch_rows);
                    }
                } else if baseline_match == Some(false) {
                    failure_category = Some(FailureCategory::Baseline);
                } else if shadow_match == Some(false) {
                    failure_category = Some(FailureCategory::ShadowMismatch);
                }
            }
            Err(e) => {
                failure_category = Some(FailureCategory::ToolError);
                error = Some(e.to_string());
            }
        }

        entries.push(BatchEntry {
            file: file_name,
            integrity_ok: !integrity_failed,
            symmetry_ok,
            baseline_match,
            baseline_mismatch_count,
            shadow_match,
            shadow_mismatch_count,
            failure_category,
            integrity_issues: integrity_report.issues.len(),
            error,
        });
    }

    let total_files = entries.len();
    let success_files = entries.iter().filter(|e| e.integrity_ok && e.symmetry_ok).count();
    let report = BatchReport {
        dir: dir_str.to_string(),
        total_files,
        success_files,
        failed_files: total_files.saturating_sub(success_files),
        entries,
        mismatch_rows: all_mismatch_rows,
    };

    if let Some(out_md) = report_path {
        generate_markdown_report(&report, out_md)?;
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }

    Ok(())
}

fn generate_markdown_report(report: &BatchReport, path: &str) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Symmetry Forensic Report\n\n");
    md.push_str("## Summary\n\n");
    md.push_str(&format!("- **Total Files Processed:** {}\n", report.total_files));
    md.push_str(&format!("- **Failed/Mismatch Files:** {}\n", report.failed_files));
    md.push_str(&format!("- **Total Mismatch Rows:** {}\n\n", report.mismatch_rows.len()));

    md.push_str("## File Integrity Summary\n\n");
    md.push_str("| File | Integrity | Symmetry | Baseline | Shadow | Bits | Note |\n");
    md.push_str("| :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n");

    for entry in &report.entries {
        let int_status = if entry.integrity_ok { "✅ OK" } else { "❌ FAIL" };
        let sym_status = if entry.symmetry_ok { "✅ OK" } else { "❌ FAIL" };
        let base_status = match entry.baseline_match {
            Some(true) => "✅ OK",
            Some(false) => "❌ FAIL",
            None => "-",
        };
        let shadow_status = match entry.shadow_match {
            Some(true) => "✅ OK",
            Some(false) => "❌ FAIL",
            None => "-",
        };
        let bits = if entry.baseline_match.is_none() && entry.shadow_match.is_none() { 
            "-".to_string() 
        } else { 
            format!("{}/{}", entry.baseline_mismatch_count, entry.shadow_mismatch_count)
        };
        let note = if entry.symmetry_ok && entry.baseline_match == Some(false) { 
            "⚠️ **Semantic Shift Risk**" 
        } else if entry.shadow_match == Some(false) {
            "⚠️ **Engine Divergence**"
        } else { 
            "" 
        };
        
        md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n", entry.file, int_status, sym_status, base_status, shadow_status, bits, note));
    }
    md.push_str("\n");

    if report.mismatch_rows.is_empty() {
        md.push_str("### ✅ ALL CLEAR\n\n");
        md.push_str("No symmetry mismatches were detected in the processed batch.\n");
    } else {
        // Top Mismatch Segments
        md.push_str("## Top Mismatch Segments\n\n");
        md.push_str("| Segment | Count |\n");
        md.push_str("| :--- | :--- |\n");
        let mut segment_counts = HashMap::new();
        for row in &report.mismatch_rows {
            *segment_counts.entry(row.segment.clone()).or_insert(0) += 1;
        }
        let mut segments: Vec<_> = segment_counts.into_iter().collect();
        segments.sort_by(|a, b| b.1.cmp(&a.1));
        for (seg, count) in segments {
            let label = if seg.is_empty() { "Unknown Segment" } else { &seg };
            md.push_str(&format!("| {} | {} |\n", label, count));
        }
        md.push_str("\n");

        // Mismatch Types
        md.push_str("## Mismatch Types\n\n");
        md.push_str("| Type | Count |\n");
        md.push_str("| :--- | :--- |\n");
        let mut type_counts = HashMap::new();
        for row in &report.mismatch_rows {
            *type_counts.entry(row.mismatch_type.clone()).or_insert(0) += 1;
        }
        let mut types: Vec<_> = type_counts.into_iter().collect();
        types.sort_by(|a, b| b.1.cmp(&a.1));
        for (t, count) in types {
            let label = if t.is_empty() { "Unknown Type" } else { &t };
            md.push_str(&format!("| {} | {} |\n", label, count));
        }
        md.push_str("\n");

        // Detailed Table
        md.push_str("## Detailed Mismatches\n\n");
        md.push_str("| File | Item Label | Code | Segment | Offset | Type |\n");
        md.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");
        for row in &report.mismatch_rows {
            let offset = row.first_mismatch_offset.map(|o| o.to_string()).unwrap_or_else(|| "-".to_string());
            md.push_str(&format!("| {} | {} | `{}` | {} | {} | {} |\n", 
                row.file, row.item_label, row.code, 
                if row.segment.is_empty() { "-" } else { &row.segment },
                offset,
                if row.mismatch_type.is_empty() { "-" } else { &row.mismatch_type }));
        }
        md.push_str("\n");
        
        md.push_str("## Actionable Clues\n\n");
        md.push_str("1. **Content Mismatches** in specific segments (e.g., `Stats`) usually indicate a field size or mapping error.\n");
        md.push_str("2. **Length Mismatches** often point to missing or extra bits in the bitstream serialization.\n");
        md.push_str("3. **ChildCount Mismatches** suggest issues with socketed items or nested structures.\n");
    }

    fs::write(path, md)?;
    Ok(())
}
