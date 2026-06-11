use d2r_core::domain::character::skills::parse_skill_section;
use d2r_core::save::{AttributeSection, Save, class_skill_base_id, map_core_sections};
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::forensics::ForensicIssue;
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusScanResult {
    pub header: HeaderSummary,
    pub attributes: Vec<AttributeSummary>,
    pub skills: Vec<SkillSummary>,
    pub jm_markers: Vec<JmMarkerSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeaderSummary {
    pub name: String,
    pub level: u32,
    pub class: u32,
    pub class_name: String,
    pub file_size: u32,
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttributeSummary {
    pub stat_id: u32,
    pub name: String,
    pub raw_value: i64,
    pub actual_value: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillSummary {
    pub skill_id: u32,
    pub name: String,
    pub level: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JmMarkerSummary {
    pub index: usize,
    pub offset: usize,
    pub bit_offset: usize,
    pub count: u16,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_status_inspect")
        .description("Inspects the status (attributes, skills, and markers) of a D2R save file");

    parser.add_arg("save_file", "path to the save file (.d2s)");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let path = parsed.get("save_file").unwrap();
    let is_json = parsed.is_json();

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            let err_msg = format!("Cannot read '{}': {}", path, e);
            if is_json {
                let report: Report<StatusScanResult> = Report::new(
                    ReportMetadata::new("d2save_status_inspect", path, env!("CARGO_PKG_VERSION")),
                    ReportStatus::Fail,
                )
                .with_forensic_issues(vec![ForensicIssue::new("IOError", &err_msg)]);
                println!("{}", serde_json::to_string_pretty(&report)?);
                std::process::exit(1);
            }
            anyhow::bail!(err_msg);
        }
    };

    let save = match Save::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("Cannot parse D2R header: {}", e);
            if is_json {
                let report: Report<StatusScanResult> = Report::new(
                    ReportMetadata::new("d2save_status_inspect", path, env!("CARGO_PKG_VERSION")),
                    ReportStatus::Fail,
                )
                .with_forensic_issues(vec![ForensicIssue::new("HeaderParseError", &err_msg)]);
                println!("{}", serde_json::to_string_pretty(&report)?);
                std::process::exit(1);
            }
            anyhow::bail!(err_msg);
        }
    };

    let mut issues = Vec::new();
    let mut header_summary = HeaderSummary {
        name: save.header.char_name.clone(),
        level: save.header.char_level as u32,
        class: save.header.char_class as u32,
        class_name: d2r_core::save::class_name(save.header.char_class).to_string(),
        file_size: save.header.file_size,
        version: save.header.version,
    };

    let map = match map_core_sections(&bytes) {
        Ok(m) => m,
        Err(e) => {
            let err_msg = format!("Failed to map sections: {}", e);
            if is_json {
                let report: Report<StatusScanResult> = Report::new(
                    ReportMetadata::new("d2save_status_inspect", path, env!("CARGO_PKG_VERSION")),
                    ReportStatus::Fail,
                )
                .with_forensic_issues(vec![ForensicIssue::new("SectionMapError", &err_msg)]);
                println!("{}", serde_json::to_string_pretty(&report)?);
                std::process::exit(1);
            }
            anyhow::bail!(err_msg);
        }
    };

    let mut attribute_summaries = Vec::new();
    match AttributeSection::parse(&bytes, map.gf_pos, map.if_pos) {
        Ok(attrs) => {
            let is_alpha = save.header.version == 105;
            for entry in &attrs.entries {
                let name = d2r_core::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|s| s.id == entry.stat_id)
                    .map(|s| s.name.as_ref())
                    .unwrap_or("Unknown");

                attribute_summaries.push(AttributeSummary {
                    stat_id: entry.stat_id,
                    name: name.to_string(),
                    raw_value: entry.raw_value as i64,
                    actual_value: attrs
                        .actual_value(entry.stat_id, is_alpha)
                        .unwrap_or(entry.raw_value as i32),
                });
            }
        }
        Err(e) => {
            issues.push(ForensicIssue::new("AttributeParseError", &e.to_string()).with_offset(map.gf_pos as u64 * 8));
        }
    }

    let mut skill_summaries = Vec::new();
    let jm0 = map.jm_positions.first().copied();
    match parse_skill_section(&bytes, map.if_pos, jm0) {
        Ok(skills) => {
            if let Some(base_id) = class_skill_base_id(save.header.char_class) {
                let class_skills = skills.iter_skills(base_id);
                for skill_level in class_skills {
                    if skill_level.level > 0 {
                        let skill_name = d2r_core::data::skills::SKILLS
                            .iter()
                            .find(|s| s.id == skill_level.skill_id)
                            .map(|s| s.key)
                            .unwrap_or("Unknown Skill");
                        skill_summaries.push(SkillSummary {
                            skill_id: skill_level.skill_id,
                            name: skill_name.to_string(),
                            level: skill_level.level,
                        });
                    }
                }
            } else {
                // If unknown class, we still might want to expose raw slots in JSON, 
                // but for now let's follow the text mode logic.
            }
        }
        Err(e) => {
            issues.push(ForensicIssue::new("SkillParseError", &e.to_string()).with_offset(map.if_pos as u64 * 8));
        }
    }

    let mut jm_marker_summaries = Vec::new();
    for (i, &pos) in map.jm_positions.iter().enumerate() {
        let count = if pos + 4 <= bytes.len() {
            u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]])
        } else {
            0
        };
        jm_marker_summaries.push(JmMarkerSummary {
            index: i,
            offset: pos,
            bit_offset: pos * 8,
            count,
        });
    }

    if is_json {
        let status = if issues.is_empty() { ReportStatus::Ok } else { ReportStatus::Warn };
        let report = Report::new(
            ReportMetadata::new("d2save_status_inspect", path, env!("CARGO_PKG_VERSION")),
            status,
        )
        .with_results(StatusScanResult {
            header: header_summary,
            attributes: attribute_summaries,
            skills: skill_summaries,
            jm_markers: jm_marker_summaries,
        })
        .with_forensic_issues(issues);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("=== SAVE STATUS INSPECT: {} ===", path);
        println!("Header Name:  {}", header_summary.name);
        println!("Header Level: {}", header_summary.level);
        println!(
            "Header Class: {} ({})",
            header_summary.class,
            header_summary.class_name
        );
        println!("File Size:    {}", header_summary.file_size);

        println!("\n--- Attributes (gf section at {}) ---", map.gf_pos);
        if attribute_summaries.is_empty() && !issues.iter().any(|i| i.kind == "AttributeParseError") {
             println!("  No attributes found.");
        }
        for attr in &attribute_summaries {
            println!(
                "  StatID {:>3} {:<20}: Raw={} Actual={}",
                attr.stat_id,
                attr.name,
                attr.raw_value,
                attr.actual_value
            );
        }
        for issue in issues.iter().filter(|i| i.kind == "AttributeParseError") {
            println!("  [ERROR] {}", issue.message);
        }

        println!("\n--- Skills (if section at {}) ---", map.if_pos);
        if skill_summaries.is_empty() && !issues.iter().any(|i| i.kind == "SkillParseError") {
            println!("  No skills with level > 0 found.");
        }
        for skill in &skill_summaries {
            println!(
                "  SkillID {:>3} {:<20}: Level={}",
                skill.skill_id, skill.name, skill.level
            );
        }
        for issue in issues.iter().filter(|i| i.kind == "SkillParseError") {
            println!("  [ERROR] {}", issue.message);
        }

        println!("\n--- Item Sections (JM markers) ---");
        for jm in &jm_marker_summaries {
            println!(
                "  JM[{}]: offset={} (bit {}), count={}",
                jm.index,
                jm.offset,
                jm.bit_offset,
                jm.count
            );
        }
    }

    Ok(())
}

