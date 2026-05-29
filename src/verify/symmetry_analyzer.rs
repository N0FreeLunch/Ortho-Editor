use serde::{Serialize, Deserialize};
use crate::domain::item::BitSegment;
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub item_idx: usize,
    pub item_code: String,
    pub bit_offset: u64,
    pub segments: Vec<BitSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymmetryReport {
    pub mismatch_offset: u64,
    pub rupture_field: String,
    pub dna_class: String,
    pub prescription: String,
}

pub fn analyze_symmetry(diff_json_path: &str, timeline_json_path: &str) -> Result<SymmetryReport> {
    let diff_content = fs::read_to_string(diff_json_path)
        .with_context(|| format!("Failed to read diff JSON: {}", diff_json_path))?;
    
    // Extract mismatch_offset from diff_content
    // Since we don't want to depend on a specific JSON structure for diff, 
    // we use a simple search like SAY did, but more robust.
    let mismatch_offset = extract_mismatch_offset(&diff_content)
        .context("Could not find mismatch_offset in diff JSON")?;

    let timeline_content = fs::read_to_string(timeline_json_path)
        .with_context(|| format!("Failed to read timeline JSON: {}", timeline_json_path))?;
    let entries: Vec<TimelineEntry> = serde_json::from_str(&timeline_content)
        .context("Failed to parse timeline JSON")?;

    for entry in entries {
        for seg in entry.segments {
            let abs_start = entry.bit_offset + seg.start;
            let abs_end = entry.bit_offset + seg.end;
            
            if mismatch_offset >= abs_start && mismatch_offset < abs_end {
                return Ok(SymmetryReport {
                    mismatch_offset,
                    rupture_field: seg.label.clone(),
                    dna_class: "bit_rhythm_rupture".to_string(), // Placeholder or inferred
                    prescription: format!(
                        "Inspect the field definition for '{}' (expecting {} bits) or check if predecessor fields consumed extra bits.",
                        seg.label, seg.end - seg.start
                    ),
                });
            }
        }
    }

    anyhow::bail!("No field in timeline matches the mismatch_offset {}", mismatch_offset)
}

fn extract_mismatch_offset(content: &str) -> Option<u64> {
    // Look for "mismatch_offset": <number>
    if let Some(pos) = content.find("\"mismatch_offset\"") {
        let remainder = &content[pos + "\"mismatch_offset\"".len()..];
        if let Some(colon_pos) = remainder.find(':') {
            let value_part = &remainder[colon_pos + 1..];
            let end_pos = value_part.find(|c: char| !c.is_numeric() && c != ' ' && c != '\t' && c != '\n' && c != '\r' && c != ',').unwrap_or(value_part.len());
            let val_str = value_part[..end_pos].trim();
            return val_str.parse::<u64>().ok();
        }
    }
    None
}
