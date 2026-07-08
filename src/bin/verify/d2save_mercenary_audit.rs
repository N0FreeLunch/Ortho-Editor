use d2r_core::domain::forensic::v105::{MercenaryEquipmentItem, MercenaryFooter, MercenaryState};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::map_core_sections;
use d2r_core::verify::OutputManager;
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::forensics::ForensicIssue;
use d2r_core::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::{env, fs, process};

#[derive(Serialize)]
struct MercenaryPayload {
    mercenary: Option<MercenaryJson>,
}

#[derive(Serialize)]
struct MercenaryJson {
    hireling_id: u8,
    class_id: u8,
    class_name: String,
    subtype_id: u8,
    subtype_name: String,
    experience: u32,
    expected_level: u8,
    name_id: u16,
    equipment: MercenaryEquipmentJson,
}

#[derive(Serialize)]
struct MercenaryEquipmentJson {
    count: usize,
    items: Vec<MercenaryItemJson>,
    footer_ok: bool,
}

#[derive(Serialize)]
struct MercenaryItemJson {
    code: String,
    slot: String,
    location: u8,
    mode: u8,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_mercenary_audit");
    parser
        .add_flag("json", "Emit report in JSON format")
        .long("json");
    parser
        .add_flag("verbose", "Emit raw bytes and diagnostic info")
        .short('v')
        .long("verbose");
    parser.add_arg("files", "Save files to audit").repeated();

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            eprintln!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let files = parsed.get_vec("files").cloned().unwrap_or_default();
    if files.is_empty() {
        anyhow::bail!("{}", parser.usage());
    }

    let mut om = OutputManager::new("d2save_mercenary_audit", &parsed);
    let is_json = om.is_json();
    let verbose = parsed.is_set("verbose");

    let mut all_ok = true;

    for path in &files {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("Cannot read file: {}", e);
                if is_json {
                    let metadata = ReportMetadata::new(
                        "d2save_mercenary_audit",
                        path,
                        env!("CARGO_PKG_VERSION"),
                    );
                    let payload = MercenaryPayload { mercenary: None };
                    let report = Report::new(metadata, ReportStatus::Fail)
                        .with_results(payload)
                        .with_issues(vec![ReportIssue {
                            kind: "IoError".to_string(),
                            message: msg.clone(),
                            bit_offset: None,
                        }]);
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    om.println(&format!("=== File: {} ===\n  [ERROR] {}", path, msg));
                }
                all_ok = false;
                continue;
            }
        };

        let audit_result = audit_mercenary(path, &bytes, verbose);
        match audit_result {
            Ok((payload, issues, status)) => {
                if is_json {
                    let metadata = ReportMetadata::new(
                        "d2save_mercenary_audit",
                        path,
                        env!("CARGO_PKG_VERSION"),
                    );
                    let report_status = if status == "Fail" {
                        ReportStatus::Fail
                    } else {
                        ReportStatus::Ok
                    };

                    let report_issues = issues
                        .iter()
                        .map(|i| ReportIssue {
                            kind: i.kind.clone(),
                            message: i.message.clone(),
                            bit_offset: i.bit_offset,
                        })
                        .collect();

                    let report = Report::new(metadata, report_status)
                        .with_results(payload)
                        .with_issues(report_issues);
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    print_report_text(&mut om, path, &payload, &issues, verbose);
                }
                if status == "Fail" {
                    all_ok = false;
                }
            }
            Err(e) => {
                let msg = format!("Audit failed: {}", e);
                if is_json {
                    let metadata = ReportMetadata::new(
                        "d2save_mercenary_audit",
                        path,
                        env!("CARGO_PKG_VERSION"),
                    );
                    let payload = MercenaryPayload { mercenary: None };
                    let report = Report::new(metadata, ReportStatus::Fail)
                        .with_results(payload)
                        .with_issues(vec![ReportIssue {
                            kind: "InternalError".to_string(),
                            message: msg.clone(),
                            bit_offset: None,
                        }]);
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    om.println(&format!("=== File: {} ===\n  [ERROR] {}", path, msg));
                }
                all_ok = false;
            }
        }
    }

    if all_ok {
        process::exit(0);
    } else {
        process::exit(1);
    }
}

fn audit_mercenary(
    path: &str,
    bytes: &[u8],
    _verbose: bool,
) -> anyhow::Result<(MercenaryPayload, Vec<ForensicIssue>, String)> {
    let map = map_core_sections(bytes).map_err(|e| anyhow::anyhow!("Map error: {}", e))?;
    let w4_data = map.w4_pos.map(|pos| {
        let w4_end = map.jf_pos.unwrap_or(bytes.len());
        &bytes[pos..w4_end]
    });

    let mut issues = Vec::new();

    let (merc, raw_w4_present) = if let Some(w4) = w4_data {
        let merc = MercenaryState::from_hybrid(bytes, Some(w4));
        if merc.exists() {
            (Some(merc), true)
        } else {
            (None, false)
        }
    } else {
        let merc_header = MercenaryState::from_hybrid(bytes, None);
        if merc_header.exists() {
            issues.push(ForensicIssue::new("SectionMissing", "w4 section is missing"));
            (Some(merc_header), false)
        } else {
            (None, false)
        }
    };

    // Equipment Audit (Slice 4)
    let mut equipment_items = Vec::new();
    let mut footer_ok = false;

    if let Some(jf) = map.jf_pos {
        // Find the JM section containing jf
        let mut merc_jm_idx = None;
        for (i, &pos) in map.jm_positions.iter().enumerate() {
            if pos <= jf {
                merc_jm_idx = Some(i);
            }
        }

        if let Some(idx) = merc_jm_idx {
            let jm_pos = map.jm_positions[idx];
            let next_pos = map
                .jm_positions
                .get(idx + 1)
                .copied()
                .unwrap_or(bytes.len());
            let item_count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);

            // Footer check
            if let (Some(kf), Some(lf)) = (map.kf_pos, map.lf_pos) {
                let footer = MercenaryFooter::from_bytes(&bytes[kf..]);
                footer_ok = footer.is_standard();
                if !footer_ok {
                    issues.push(
                        ForensicIssue::new(
                            "LayoutAnomaly",
                            "Non-standard mercenary footer detected",
                        )
                        .with_offset(kf as u64 * 8),
                    );
                }

                if lf + 4 < next_pos {
                    // Items follow footer
                    let huffman = HuffmanTree::new();
                    let is_alpha = bytes
                        .get(4..8)
                        .map(|b| u32::from_le_bytes(b.try_into().unwrap_or([0; 4])) == 105)
                        .unwrap_or(false);

                    // In Alpha v105, items in the mercenary section follow the lf marker
                    let items_start = lf + 4;
                    let items_data = &bytes[items_start..next_pos];

                    match Item::read_section(
                        items_data,
                        items_start as u64 * 8,
                        item_count,
                        &huffman,
                        is_alpha,
                        false,
                    ) {
                        Ok(items) => {
                            for it in items {
                                if it.is_residue() {
                                    continue;
                                }

                                let merc_it = MercenaryEquipmentItem {
                                    code: it.code.trim().to_string(),
                                    location: it.location,
                                    mode: it.mode,
                                    x: it.x,
                                    y: it.y,
                                };

                                let slot = merc_it.slot_name();

                                if it.mode != 1 || it.location != 1 {
                                    issues.push(
                                        ForensicIssue::new("SemanticViolation", &format!("Mercenary item {} in invalid mode/location (mode={}, loc={})", it.code.trim(), it.mode, it.location))
                                            .with_offset(it.range.start)
                                    );
                                }

                                equipment_items.push(MercenaryItemJson {
                                    code: merc_it.code,
                                    slot,
                                    location: merc_it.location,
                                    mode: merc_it.mode,
                                });
                            }
                        }
                        Err(e) => {
                            issues.push(
                                ForensicIssue::new(
                                    "ParseFailure",
                                    &format!("Failed to parse mercenary items: {}", e),
                                )
                                .with_offset(items_start as u64 * 8),
                            );
                        }
                    }
                }
            } else {
                issues.push(
                    ForensicIssue::new("SectionMissing", "Mercenary kf/lf markers missing")
                        .with_offset(jm_pos as u64 * 8),
                );
            }
        }
    }

    let mercenary_json = merc.map(|merc| {
        let class_name = merc.class_name();
        let subtype_name = merc.subtype_name();

        if raw_w4_present {
            if let Some(w4) = w4_data {
                let has_marker = w4.starts_with(b"w4");
                let c_off = if has_marker { 6 } else { 4 };
                let raw_class = w4.get(c_off).copied().unwrap_or(0);
                if raw_class != merc.class_id {
                    let w4_pos = map.w4_pos.unwrap_or(0);
                    issues.push(
                        ForensicIssue::new("AlignmentDrift", &format!(
                            "Alignment Drift Detected! MercenaryState class ({}) != raw class ({})",
                            merc.class_id, raw_class
                        )).with_offset(w4_pos as u64 * 8)
                    );
                }
            }
        }

        MercenaryJson {
            hireling_id: merc.hireling_id,
            class_id: merc.class_id,
            class_name,
            subtype_id: merc.subtype_id,
            subtype_name,
            experience: merc.experience,
            expected_level: merc.expected_level(),
            name_id: merc.name_id,
            equipment: MercenaryEquipmentJson {
                count: equipment_items.len(),
                items: equipment_items,
                footer_ok,
            },
        }
    });

    let status = if issues.iter().any(|i| {
        i.kind == "AlignmentDrift"
            || i.kind == "ParseFailure"
            || i.kind == "SemanticViolation"
            || i.kind == "IoError"
            || i.kind == "InternalError"
            || i.message.contains("error")
            || i.message.contains("Failed")
            || i.message.contains("Non-standard")
            || i.message.contains("missing")
            || i.message.contains("invalid")
    }) {
        "Fail".to_string()
    } else {
        "Ok".to_string()
    };

    Ok((
        MercenaryPayload {
            mercenary: mercenary_json,
        },
        issues,
        status,
    ))
}

fn print_report_text(
    om: &mut OutputManager,
    path: &str,
    payload: &MercenaryPayload,
    issues: &[ForensicIssue],
    verbose: bool,
) {
    om.println(&format!("=== File: {} ===", path));
    if let Some(merc) = &payload.mercenary {
        om.println("Hybrid Decoded:");
        om.println(&format!(
            "  Class:    {} ({})",
            merc.class_id, merc.class_name
        ));
        om.println(&format!(
            "  Subtype:  {} ({})",
            merc.subtype_id, merc.subtype_name
        ));
        om.println(&format!("  ID:        {}", merc.hireling_id));
        om.println(&format!(
            "  Experience: {} (0x{:08X}) -> Expected Level: {}",
            merc.experience, merc.experience, merc.expected_level
        ));
        om.println(&format!("  Name ID:   {}", merc.name_id));

        om.println(&format!(
            "Equipment: {} items (Footer: {})",
            merc.equipment.count,
            if merc.equipment.footer_ok {
                "Standard"
            } else {
                "NON-STANDARD"
            }
        ));
        for it in &merc.equipment.items {
            om.println(&format!("  - {} in slot {}", it.code, it.slot));
        }
    }

    for issue in issues {
        let label = match issue.kind.as_str() {
            "AlignmentDrift" | "ParseFailure" | "SemanticViolation" | "IoError"
            | "InternalError" => "[ERROR]",
            "SectionMissing" | "LayoutAnomaly" => "[WARN]",
            _ => "[INFO]",
        };
        let offset_str = issue
            .bit_offset
            .map(|o| format!(" @ bit {}", o))
            .unwrap_or_default();
        om.println(&format!("  {} {}{}", label, issue.message, offset_str));
    }

    if verbose {
        // Output extra debug bytes if needed
    }
    om.println("");
}
