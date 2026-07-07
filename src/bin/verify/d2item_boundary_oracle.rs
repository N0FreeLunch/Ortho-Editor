use d2r_core::item::{HuffmanTree, peek_item_header_at};
use d2r_core::verify::OutputManager;
use d2r_core::verify::args::{ArgParser, ArgSpec};
use d2r_core::verify::desync::dump_bits_at;
use d2r_core::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::path::PathBuf;

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AnchorKind {
    PreviousJm,
    NextJm,
}

#[derive(Serialize, Debug)]
pub struct Anchor {
    pub kind: AnchorKind,
    pub bit_offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resynced: Option<bool>,
}

#[derive(Serialize, Debug)]
pub struct TargetSpan {
    pub start_bit: u64,
    pub end_bit: u64,
    pub policy: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LocalClosure {
    Passed,
    Failed,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DownstreamStatus {
    Quarantined,
    Actionable,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    LocalClosed,
    LocalOpen,
    SameBudgetPatchOnly,
    UnsafeNoRightAnchor,
}

#[derive(Serialize, Debug)]
pub struct BoundaryReport {
    pub fixture: PathBuf,
    pub domain: String,
    pub item_index: Option<usize>,
    pub left_anchor: Anchor,
    pub target_span: TargetSpan,
    pub right_anchor: Anchor,
    pub local_closure: LocalClosure,
    pub downstream_status: DownstreamStatus,
    pub verdict: Verdict,
    pub allowed_next_action: String,
    pub hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_dump: Option<String>,
}

fn main() {
    let mut parser = ArgParser::new("d2item_boundary_oracle");
    parser.add_spec(ArgSpec::option(
        "fixture",
        None,
        Some("fixture"),
        "Path to save file (d2s)",
    ));
    parser.add_spec(ArgSpec::option(
        "domain",
        None,
        Some("domain"),
        "Domain to isolate (e.g. item.compact, item.summary, item.stats.huffman)",
    ));
    parser.add_spec(
        ArgSpec::option(
            "item-index",
            None,
            Some("item-index"),
            "Index of the item in the save",
        )
        .optional(),
    );
    parser.add_spec(ArgSpec::flag(
        "dump-span",
        None,
        Some("dump-span"),
        "Dump the bitstream of the target span",
    ));

    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let mut out = OutputManager::new("d2item_boundary_oracle", &parsed);

    let fixture_str = parsed
        .get("fixture")
        .cloned()
        .unwrap_or_else(|| "dummy.d2s".to_string());
    let fixture_path = PathBuf::from(&fixture_str);
    let domain = parsed
        .get("domain")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let item_index = parsed
        .get("item-index")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let dump_span = parsed.is_set("dump-span");

    let bytes = match std::fs::read(&fixture_path) {
        Ok(b) => b,
        Err(e) => {
            if out.is_json() {
                let report: Report<()> = Report::new(
                    ReportMetadata::new(
                        "d2item_boundary_oracle",
                        &fixture_str,
                        env!("CARGO_PKG_VERSION"),
                    ),
                    ReportStatus::Fail,
                )
                .with_issues(vec![ReportIssue {
                    kind: "io_error".to_string(),
                    message: format!("Failed to read fixture: {}", e),
                    bit_offset: None,
                }]);
                out.json(&serde_json::to_string_pretty(&report).unwrap());
            } else {
                out.println(&format!(
                    "Error: Failed to read fixture '{}': {}",
                    fixture_str, e
                ));
            }
            std::process::exit(1);
        }
    };

    // Simple byte-aligned JM marker search
    let mut jm_offsets = Vec::new();
    for i in 0..(bytes.len().saturating_sub(1)) {
        if bytes[i] == 0x4A && bytes[i + 1] == 0x4D {
            // 'J', 'M'
            jm_offsets.push(i as u64 * 8);
        }
    }

    let (left_bit, right_bit, verdict, hint, refined_start) = if jm_offsets.is_empty() {
        (
            0,
            0,
            Verdict::UnsafeNoRightAnchor,
            "No JM markers found in file.".to_string(),
            None,
        )
    } else if item_index >= jm_offsets.len() {
        (
            jm_offsets.last().copied().unwrap_or(0),
            (bytes.len() as u64 * 8),
            Verdict::UnsafeNoRightAnchor,
            format!(
                "Item index {} out of range (found {} markers).",
                item_index,
                jm_offsets.len()
            ),
            None,
        )
    } else {
        let left = jm_offsets[item_index];
        let right = if item_index + 1 < jm_offsets.len() {
            jm_offsets[item_index + 1]
        } else {
            bytes.len() as u64 * 8
        };

        let v = if item_index + 1 < jm_offsets.len() {
            Verdict::LocalOpen
        } else {
            Verdict::UnsafeNoRightAnchor
        };

        let mut refined_start = None;
        let mut refined_hint = format!(
            "Isolated item {} using byte-aligned JM markers.",
            item_index
        );

        if domain == "item.stats.huffman" {
            let huffman = HuffmanTree::new();
            let version_le = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
            let is_alpha = version_le == 6 || version_le == 105;

            // Try brute-forcing the item start within a small window after the JM marker.
            // This accounts for various section header lengths (32, 38, etc.) and potential alignment drifts.
            let mut found_res = None;
            let mut used_left = left;

            for offset in 0..64 {
                if let Some(res) = peek_item_header_at(&bytes, left + offset, &huffman, is_alpha, 0)
                {
                    found_res = Some(res);
                    used_left = left + offset;
                    break;
                }
            }

            if let Some(res) = found_res {
                let is_compact = res.6;
                let header_len = res.7;
                if !is_compact {
                    refined_start = Some(used_left + header_len);
                    refined_hint = format!(
                        "Isolated item {} (header_len={}, offset_from_jm={}) and refined target_span for stats.",
                        item_index,
                        header_len,
                        used_left - left
                    );
                } else {
                    refined_hint = format!(
                        "Isolated item {} (compact, no stats payload). Coarse start retained.",
                        item_index
                    );
                }
            } else {
                refined_hint = format!(
                    "Isolated item {} (header parse failed at JM and section offsets). Coarse start retained.",
                    item_index
                );
            }
        }

        (left, right, v, refined_hint, refined_start)
    };

    let start_bit = refined_start.unwrap_or(left_bit);
    let end_bit = right_bit;

    let mut visual_dump = None;
    if dump_span {
        if start_bit < end_bit && start_bit < (bytes.len() as u64 * 8) {
            let count = (end_bit - start_bit).min(1024 * 1024) as u32; // Limit to 1Mb bits
            let dump = dump_bits_at(&bytes, start_bit, count);

            if count > 2048 {
                let artifact_dir = PathBuf::from("agent_artifacts");
                if !artifact_dir.exists() {
                    let _ = std::fs::create_dir_all(&artifact_dir);
                }
                let file_name = format!(
                    "oracle_dump_{}_{}.bits",
                    item_index,
                    domain.replace(".", "_")
                );
                let file_path = artifact_dir.join(file_name);
                if std::fs::write(&file_path, &dump).is_ok() {
                    visual_dump = Some(format!("artifact:{}", file_path.display()));
                } else {
                    visual_dump = Some(dump);
                }
            } else {
                visual_dump = Some(dump);
            }
        }
    }

    let report = BoundaryReport {
        fixture: fixture_path,
        domain: domain.clone(),
        item_index: Some(item_index),
        left_anchor: Anchor {
            kind: AnchorKind::PreviousJm,
            bit_offset: left_bit,
            confidence: Some(1.0),
            resynced: None,
        },
        target_span: TargetSpan {
            start_bit,
            end_bit,
            policy: "same_budget_patch_only".to_string(),
        },
        right_anchor: Anchor {
            kind: AnchorKind::NextJm,
            bit_offset: right_bit,
            confidence: if right_bit < (bytes.len() as u64 * 8) {
                Some(1.0)
            } else {
                Some(0.5)
            },
            resynced: Some(false),
        },
        local_closure: LocalClosure::Failed,
        downstream_status: DownstreamStatus::Quarantined,
        verdict,
        allowed_next_action: "fix_target_span_only".to_string(),
        hint,
        visual_dump,
    };

    if out.is_json() {
        let final_report = Report::new(
            ReportMetadata::new(
                "d2item_boundary_oracle",
                &fixture_str,
                env!("CARGO_PKG_VERSION"),
            ),
            ReportStatus::Ok,
        )
        .with_results(report);
        out.json(&serde_json::to_string_pretty(&final_report).unwrap());
    } else {
        out.println(&format!(
            "Boundary Oracle Report for domain: {}",
            report.domain
        ));
        out.println(&format!("  Item Index: {}", item_index));
        out.println(&format!("  Left Anchor (bit):  {}", left_bit));
        out.println(&format!(
            "  Target Span Start:  {}",
            report.target_span.start_bit
        ));
        out.println(&format!("  Right Anchor (bit): {}", right_bit));
        out.println(&format!("  Verdict: {:?}", report.verdict));
        out.println(&format!("  Hint: {}", report.hint));

        if let Some(dump) = &report.visual_dump {
            if dump.starts_with("artifact:") {
                out.println(&format!("  Visual Dump: (saved to {})", &dump[9..]));
            } else {
                out.println("  Visual Dump:");
                out.println(&format!("    {}", dump));
            }
        }

        out.println("\n  (Use --json for machine-readable bitstream isolation boundaries)");
    }
}
