use d2r_core::domain::forensic::v105::MercenaryState;
use d2r_core::save::map_core_sections;
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::OutputManager;
use serde::Serialize;
use std::{env, fs, process};

#[derive(Serialize)]
struct MercenaryReport {
    metadata: ReportMetadata,
    status: String,
    mercenary: Option<MercenaryJson>,
    issues: Vec<String>,
}

#[derive(Serialize)]
struct ReportMetadata {
    tool: String,
    file: String,
}

#[derive(Serialize)]
struct MercenaryJson {
    class_id: u8,
    class_name: String,
    subtype_id: u8,
    subtype_name: String,
    hireling_id: u8,
    experience: u32,
    name_id: u16,
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
                if is_json {
                    let report = MercenaryReport {
                        metadata: ReportMetadata {
                            tool: "d2save_mercenary_audit".to_string(),
                            file: path.clone(),
                        },
                        status: "Fail".to_string(),
                        mercenary: None,
                        issues: vec![format!("Cannot read file: {}", e)],
                    };
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    om.println(&format!("=== File: {} ===\n  [ERROR] Cannot read file: {}", path, e));
                }
                all_ok = false;
                continue;
            }
        };

        let audit_result = audit_mercenary(path, &bytes, verbose);
        match audit_result {
            Ok(report) => {
                if is_json {
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    print_report_text(&mut om, path, &report, verbose);
                }
                if report.status == "Fail" {
                    all_ok = false;
                }
            }
            Err(e) => {
                if is_json {
                    let report = MercenaryReport {
                        metadata: ReportMetadata {
                            tool: "d2save_mercenary_audit".to_string(),
                            file: path.clone(),
                        },
                        status: "Fail".to_string(),
                        mercenary: None,
                        issues: vec![format!("Audit failed: {}", e)],
                    };
                    om.json(&serde_json::to_string(&report)?);
                } else {
                    om.println(&format!("=== File: {} ===\n  [ERROR] Audit failed: {}", path, e));
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

fn audit_mercenary(path: &str, bytes: &[u8], _verbose: bool) -> anyhow::Result<MercenaryReport> {
    let map = map_core_sections(bytes).map_err(|e| anyhow::anyhow!("Map error: {}", e))?;
    let w4_data = map.w4_pos.map(|pos| {
        let w4_end = map.jf_pos.unwrap_or(bytes.len());
        &bytes[pos..w4_end]
    });

    let mut issues = Vec::new();

    let (merc, raw_w4_present) = if let Some(w4) = w4_data {
        let merc = MercenaryState::from_hybrid(bytes, Some(w4));
        (Some(merc), true)
    } else {
        issues.push("w4 section NOT found".to_string());
        let merc = MercenaryState::from_hybrid(bytes, None);
        (Some(merc), false)
    };

    let mercenary_json = merc.map(|merc| {
        let class_name = match merc.class_id {
            0 => {
                if merc.hireling_id >= 8 {
                    "Desert Warrior (Act 2)".to_string()
                } else {
                    "Rogue (Act 1)".to_string()
                }
            }
            1 => "Iron Wolf (Act 3)".to_string(),
            9 => "Barbarian (Act 5)".to_string(),
            _ => format!("Unknown({})", merc.class_id),
        };

        let subtype_name = if merc.class_id == 1 {
            match merc.subtype_id {
                15 => "Fire".to_string(),
                16 => "Cold".to_string(),
                17 => "Lightning".to_string(),
                _ => "Unknown Element".to_string(),
            }
        } else {
            "N/A".to_string()
        };

        if raw_w4_present {
            if let Some(w4) = w4_data {
                let has_marker = w4.starts_with(b"w4");
                let c_off = if has_marker { 6 } else { 4 };
                let raw_class = w4.get(c_off).copied().unwrap_or(0);
                if raw_class != merc.class_id {
                    issues.push(format!(
                        "Alignment Drift Detected! MercenaryState class ({}) != raw class ({})",
                        merc.class_id, raw_class
                    ));
                }
            }
        }

        MercenaryJson {
            class_id: merc.class_id,
            class_name,
            subtype_id: merc.subtype_id,
            subtype_name,
            hireling_id: merc.hireling_id,
            experience: merc.experience,
            name_id: merc.name_id,
        }
    });

    let status = if issues.iter().any(|i| i.contains("error") || i.contains("Drift")) {
        "Fail".to_string()
    } else {
        "Ok".to_string()
    };

    Ok(MercenaryReport {
        metadata: ReportMetadata {
            tool: "d2save_mercenary_audit".to_string(),
            file: path.to_string(),
        },
        status,
        mercenary: mercenary_json,
        issues,
    })
}

fn print_report_text(om: &mut OutputManager, path: &str, report: &MercenaryReport, verbose: bool) {
    om.println(&format!("=== File: {} ===", path));
    if let Some(merc) = &report.mercenary {
        om.println("Hybrid Decoded:");
        om.println(&format!("  Class:    {} ({})", merc.class_id, merc.class_name));
        om.println(&format!("  Subtype:  {} ({})", merc.subtype_id, merc.subtype_name));
        om.println(&format!("  ID (H169):{}", merc.hireling_id));
        om.println(&format!(
            "  Experience: {} (0x{:08X})",
            merc.experience, merc.experience
        ));
        om.println(&format!("  Name ID:   {}", merc.name_id));
    }
    
    for issue in &report.issues {
        if issue.contains("Drift") || issue.contains("NOT found") {
            om.println(&format!("  [WARN] {}", issue));
        } else {
            om.println(&format!("  [INFO] {}", issue));
        }
    }
    
    if verbose {
        // Output extra debug bytes if needed
    }
    om.println("");
}
