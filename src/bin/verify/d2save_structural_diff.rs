use d2r_core::domain::progression::axiom::{
    V105_HEADER_LEN, V105_NPC_OFFSET, V105_QUEST_OFFSET, V105_WAYPOINT_OFFSET,
};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::fs;
use std::process;

#[derive(Serialize, Clone)]
struct DiffDetail {
    offset: usize,
    label: String,
    a_hex: String,
    b_hex: String,
    is_masked: bool,
}

#[derive(Serialize, Clone)]
struct SectionSummary {
    name: String,
    diff_count: usize,
    is_masked: bool,
}

#[derive(Serialize)]
struct StructuralDiffPayload {
    total_diff_bytes: usize,
    unmasked_diff_bytes: usize,
    length_delta: i64,
    sections: Vec<SectionSummary>,
    details: Vec<DiffDetail>,
}

fn classify_offset(offset: usize, jm0: Option<usize>) -> &'static str {
    if offset < 12 {
        "Header"
    } else if (12..16).contains(&offset) {
        "Checksum"
    } else if (16..V105_QUEST_OFFSET).contains(&offset) {
        "HeaderPadding"
    } else if (V105_QUEST_OFFSET..V105_WAYPOINT_OFFSET).contains(&offset) {
        "Quest"
    } else if (V105_WAYPOINT_OFFSET..V105_NPC_OFFSET).contains(&offset) {
        "Waypoint"
    } else if (V105_NPC_OFFSET..V105_HEADER_LEN).contains(&offset) {
        "NPC"
    } else if let Some(j) = jm0 {
        if offset >= j { "Items" } else { "Other" }
    } else {
        "Other"
    }
}

fn main() {
    let mut parser = ArgParser::new("d2save_structural_diff").description(
        "Compares two D2R save files with Alpha v105 subsystem awareness and checksum masking",
    );

    parser.add_spec(ArgSpec::positional("file_a", "First save file"));
    parser.add_spec(ArgSpec::positional("file_b", "Second save file"));
    parser.add_spec(ArgSpec::flag(
        "all",
        Some('a'),
        Some("all"),
        "Show masked differences",
    ));
    parser.add_spec(ArgSpec::flag(
        "details",
        Some('d'),
        Some("details"),
        "Show detailed offset list",
    ));

    let args: Vec<_> = std::env::args_os().skip(1).collect();
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

    let mut out = OutputManager::new("d2save_structural_diff", &parsed);
    let path_a = parsed.get("file_a").unwrap();
    let path_b = parsed.get("file_b").unwrap();
    let is_json = out.is_json();
    let show_all = parsed.is_set("all");
    let show_details = parsed.is_set("details");

    let bytes_a = fs::read(path_a).expect("Failed to read file_a");
    let bytes_b = fs::read(path_b).expect("Failed to read file_b");

    let common_len = bytes_a.len().min(bytes_b.len());
    let jm_a = find_jm_markers(&bytes_a).first().copied();
    let jm_b = find_jm_markers(&bytes_b).first().copied();
    let jm_ref = jm_a.or(jm_b);

    let mut details = Vec::new();
    let mut section_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for i in 0..common_len {
        if bytes_a[i] != bytes_b[i] {
            let label = classify_offset(i, jm_ref);
            let is_masked = label == "Checksum";

            *section_counts.entry(label.to_string()).or_insert(0) += 1;

            details.push(DiffDetail {
                offset: i,
                label: label.to_string(),
                a_hex: format!("0x{:02X}", bytes_a[i]),
                b_hex: format!("0x{:02X}", bytes_b[i]),
                is_masked,
            });
        }
    }

    let unmasked_count = details.iter().filter(|d| !d.is_masked).count();
    let length_delta = bytes_b.len() as i64 - bytes_a.len() as i64;

    if is_json {
        let mut sections: Vec<_> = section_counts
            .into_iter()
            .map(|(name, diff_count)| SectionSummary {
                is_masked: name == "Checksum",
                name,
                diff_count,
            })
            .collect();
        sections.sort_by(|a, b| a.name.cmp(&b.name));

        let payload = StructuralDiffPayload {
            total_diff_bytes: details.len(),
            unmasked_diff_bytes: unmasked_count,
            length_delta,
            sections,
            details: if show_all {
                details
            } else {
                details.into_iter().filter(|d| !d.is_masked).collect()
            },
        };

        let status = if payload.unmasked_diff_bytes > 0 || length_delta != 0 {
            ReportStatus::Fail
        } else {
            ReportStatus::Ok
        };

        let report = Report::new(
            ReportMetadata::new("d2save_structural_diff", path_a, "Alpha v105"),
            status,
        )
        .with_results(payload);

        out.json(&serde_json::to_string_pretty(&report).unwrap());
    } else {
        out.println("=== d2save_structural_diff ===");
        out.println(&format!("  A: {} ({} bytes)", path_a, bytes_a.len()));
        out.println(&format!("  B: {} ({} bytes)", path_b, bytes_b.len()));
        out.println("");

        if section_counts.is_empty() && length_delta == 0 {
            out.println("  [IDENTICAL] No differences found.");
        } else {
            out.println("[SECTION SUMMARY]");
            let mut keys: Vec<_> = section_counts.keys().collect();
            keys.sort();
            for label in keys {
                let count = section_counts[label];
                let mask_note = if label == "Checksum" { " (MASKED)" } else { "" };
                out.println(&format!("  {:<15}: {:>4} bytes{}", label, count, mask_note));
            }
            if length_delta != 0 {
                out.println(&format!("  {:<15}: {:>4} bytes", "Length Delta", length_delta));
            }
            out.println("");

            if show_details || show_all {
                out.println("[DETAILED DIFFS]");
                out.println(&format!(
                    "  {:>8}  {:<12}  {:>8}  {:>8}",
                    "Offset", "Section", "A", "B"
                ));
                out.println(&format!("  {:->8}  {:->12}  {:->8}  {:->8}", "", "", "", ""));

                for d in &details {
                    if !show_all && d.is_masked {
                        continue;
                    }
                    out.println(&format!(
                        "  {:>8}  {:<12}  {:>8}  {:>8}",
                        d.offset, d.label, d.a_hex, d.b_hex
                    ));
                }

                if !show_all && details.len() > unmasked_count {
                    out.println(&format!(
                        "  ... ({} masked bytes hidden, use --all to see)",
                        details.len() - unmasked_count
                    ));
                }
            } else if unmasked_count == 0 && !details.is_empty() {
                out.println(
                    "  [INFO] Only masked sections (Checksum) differ. Use --all to see details."
                );
            } else if unmasked_count > 0 {
                out.println(&format!(
                    "  [INFO] {} unmasked differences found. Use --details to see offset list.",
                    unmasked_count
                ));
            }
        }
    }
}
