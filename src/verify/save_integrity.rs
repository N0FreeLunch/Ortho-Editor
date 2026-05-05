use crate::domain::header::axiom::{ACTIVE_ACT_OFFSET, EXPANSION_FLAG_OFFSET, PROGRESS_FLAG_OFFSET};
use crate::domain::progression::axiom::{V105_NPC_OFFSET, V105_QUEST_OFFSET, V105_WAYPOINT_OFFSET};
use crate::domain::item::axiom_meta::{ForensicAudit, FidelityScore};
use crate::item::{HuffmanTree, Item, ParsingError};
use crate::save::{find_jm_markers, map_core_sections, recalculate_checksum, Save};
use crate::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};
use crate::verify::v2::{DomainVerifier, header::HeaderVerifier, progression::ProgressionVerifier};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct D2SaveVerifyPayload {
    pub header_version: u32,
    pub alpha_mode: bool,
    pub file_size_header: usize,
    pub file_size_actual: usize,
    pub file_size_delta: i64,
    pub checksum_stored: String,
    pub checksum_calculated: Option<String>,
    pub jm_marker_count: usize,
    pub active_act: u8,
    pub progression_flag: u8,
    pub expansion_flag: u8,
    pub issue_count: usize,
    pub fidelity_score: f32,
    pub forensic_audit: ForensicAudit,
}

pub fn verify_save_integrity(path: &str, bytes: &[u8]) -> (Report<D2SaveVerifyPayload>, bool) {
    let mut issues = Vec::new();
    let mut fail = false;

    // Modular V2 Verifiers
    let header_verifier = HeaderVerifier;
    let prog_verifier = ProgressionVerifier;

    let save = match Save::from_bytes(bytes) {
        Ok(s) => s,
        Err(err) => {
            issues.push(ReportIssue {
                kind: "header_parse".to_string(),
                message: format!("Header parse: {}", err),
                bit_offset: None,
            });
            let report = Report::<D2SaveVerifyPayload>::new(
                ReportMetadata::new("d2save_verify", path, "corrupted"),
                ReportStatus::Fail,
            )
            .with_issues(issues)
            .with_hints(vec!["Header is corrupted or in an unsupported format.".to_string()])
            .with_results(D2SaveVerifyPayload {
                header_version: 0,
                alpha_mode: false,
                file_size_header: 0,
                file_size_actual: bytes.len(),
                file_size_delta: 0,
                checksum_stored: "0x00000000".to_string(),
                checksum_calculated: None,
                jm_marker_count: 0,
                active_act: 0,
                progression_flag: 0,
                expansion_flag: 0,
                issue_count: 1,
                fidelity_score: 0.0,
                forensic_audit: ForensicAudit::new(),
            });
            return (report, true);
        }
    };

    let alpha_mode = save.header.version == 105;
    let huffman = HuffmanTree::new();

    // 1. Header V2 Integration
    let header_report = header_verifier.verify(bytes, alpha_mode);
    for issue in header_report.issues {
        // Maintain legacy fail rules
        if issue.kind == "header_parse" || issue.kind == "file_size" || issue.kind == "checksum" {
            fail = true;
        }
        issues.push(issue);
    }

    // 2. Structural Checks (Alpha v105 specific, kept for Slice 4 stability)
    if alpha_mode {
        if let Ok(map) = map_core_sections(bytes) {
            for (name, found, expected) in [
                ("Woo!", map.woo_pos, V105_QUEST_OFFSET),
                ("WS", map.ws_pos, V105_WAYPOINT_OFFSET),
                ("w4", map.w4_pos, V105_NPC_OFFSET),
            ] {
                match found {
                    Some(pos) if pos != expected => issues.push(ReportIssue {
                        kind: "structural".into(),
                        message: format!(
                            "Alpha v105: {} marker displaced (found at 0x{:03X}, expected 0x{:03X})",
                            name, pos, expected
                        ),
                        bit_offset: Some(pos as u64 * 8),
                    }),
                    None => {
                        issues.push(ReportIssue {
                            kind: "structural".into(),
                            message: format!("Alpha v105: {} marker missing", name),
                            bit_offset: None,
                        });
                        fail = true;
                    }
                    _ => {}
                }
            }
        } else {
            issues.push(ReportIssue {
                kind: "structural".into(),
                message: "Alpha v105: Failed to map core sections".into(),
                bit_offset: None,
            });
            fail = true;
        }
    }

    // 3. Item Parsing and Round-trip
    let items = match Item::read_player_items(bytes, &huffman, alpha_mode) {
        Ok(items) => items,
        Err(err) => {
            issues.push(ReportIssue {
                kind: "item_parse".to_string(),
                message: format!("{}", err),
                bit_offset: parsing_error_offset(&err.error),
            });
            fail = true;
            Vec::new()
        }
    };

    for item in &items {
        let item_bits = match item.to_bytes(&huffman, alpha_mode) {
            Ok(b) => b,
            Err(e) => {
                issues.push(ReportIssue {
                    kind: "item_parse".to_string(),
                    message: format!("Item to_bytes ({}): {}", item.code, e),
                    bit_offset: None,
                });
                fail = true;
                continue;
            }
        };
        if let Err(e) = Item::from_bytes(&item_bits, &huffman, alpha_mode) {
            issues.push(ReportIssue {
                kind: "item_parse".to_string(),
                message: format!("Item round-trip parse failure ({}): {}", item.code, e),
                bit_offset: parsing_error_offset(&e.error),
            });
            fail = true;
        }
    }

    // 4. Progression V2 Integration
    let prog_report = prog_verifier.verify(bytes, alpha_mode);
    for issue in prog_report.issues {
        if issue.kind == "progression_parse" {
            fail = true;
        }
        issues.push(issue);
    }

    // 5. JM Coherence (Item Domain Bridge)
    let jm_markers = find_jm_markers(bytes);
    if jm_markers.is_empty() {
        issues.push(ReportIssue {
            kind: "jm_markers".to_string(),
            message: "No JM markers found".to_string(),
            bit_offset: None,
        });
    }

    if let Some(&jm0) = jm_markers.first() {
        if jm0 + 3 < bytes.len() {
            let expected = u16::from_le_bytes([bytes[jm0 + 2], bytes[jm0 + 3]]) as usize;
            if expected != items.len() {
                issues.push(ReportIssue {
                    kind: "jm_coherence".into(),
                    message: format!("JM header count ({}) != parsed items ({})", expected, items.len()),
                    bit_offset: Some((jm0 + 2) as u64 * 8),
                });
                fail = true;
            }
        }
    }

    // 6. Forensic Audit Aggregation
    let mut forensic_audit = prog_report.audit;
    for item in &items {
        forensic_audit.extend(item.forensic_audit.clone());
    }

    let fidelity_score = FidelityScore::from_audit(&forensic_audit).value;
    let hints = synthesize_hints(&issues);
    let issue_count = issues.len();
    let status = if fail { ReportStatus::Fail } else { ReportStatus::Ok };
    let version = format!("0x{:04X}", save.header.version);
    
    let stored_checksum = save.header.checksum;
    let calculated_checksum_opt = recalculate_checksum(bytes).ok();

    let report = Report::<D2SaveVerifyPayload>::new(
        ReportMetadata::new("d2save_verify", path, &version),
        status,
    )
    .with_issues(issues)
    .with_hints(hints)
    .with_results(D2SaveVerifyPayload {
        header_version: save.header.version,
        alpha_mode,
        file_size_header: save.header.file_size as usize,
        file_size_actual: bytes.len(),
        file_size_delta: bytes.len() as i64 - save.header.file_size as i64,
        checksum_stored: format!("0x{:08X}", stored_checksum),
        checksum_calculated: calculated_checksum_opt.map(|c| format!("0x{:08X}", c)),
        jm_marker_count: jm_markers.len(),
        active_act: if alpha_mode && bytes.len() > ACTIVE_ACT_OFFSET {
            bytes[ACTIVE_ACT_OFFSET]
        } else {
            0
        },
        progression_flag: if alpha_mode && bytes.len() > PROGRESS_FLAG_OFFSET {
            bytes[PROGRESS_FLAG_OFFSET]
        } else {
            0
        },
        expansion_flag: if alpha_mode && bytes.len() > EXPANSION_FLAG_OFFSET {
            bytes[EXPANSION_FLAG_OFFSET]
        } else {
            0
        },
        issue_count,
        fidelity_score,
        forensic_audit,
    });

    (report, fail)
}

fn parsing_error_offset(error: &ParsingError) -> Option<u64> {
    match error {
        ParsingError::InvalidHuffmanBit { bit_offset }
        | ParsingError::UnexpectedSegmentEnd { bit_offset }
        | ParsingError::BitSymmetryFailure { bit_offset } => Some(*bit_offset),
        ParsingError::InvalidStatId { bit_offset, .. } => Some(*bit_offset),
        _ => None,
    }
}

fn synthesize_hints(issues: &[ReportIssue]) -> Vec<String> {
    let mut hints = Vec::new();
    for issue in issues {
        match issue.kind.as_str() {
            "header_parse" => hints.push("Header is corrupted or in an unsupported format.".to_string()),
            "item_parse" => {
                if let Some(offset) = issue.bit_offset {
                    hints.push(format!("Investigate bit-width or alignment logic near bit offset {}.", offset));
                } else {
                    hints.push("Check item data structure or Huffman encoding table.".to_string());
                }
            }
            "file_size" => {
                hints.push("File size in header must match the actual byte count. Truncation suspected.".to_string())
            }
            "checksum" => hints.push("Checksum must be refreshed after any file mutation (lives at offset 12).".to_string()),
            "checksum_info" => hints.push("Alpha v105: Checksum mismatch ignored for forensic bit-perfect baseline.".to_string()),
            "jm_markers" => hints.push("Missing JM markers suggest the file is not a valid character save or is severely truncated.".to_string()),
            "structural" => hints.push("Alpha v105: Structural markers (Woo!, WS, w4) are missing (critical) or displaced (non-fatal).".to_string()),
            "jm_coherence" => hints.push("JM header item count does not match parsed item count; file may be truncated or structurally corrupted.".to_string()),
            _ => {}
        }
    }
    hints.sort();
    hints.dedup();
    hints
}
