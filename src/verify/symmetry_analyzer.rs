use crate::domain::item::{BitSegment, Item};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub item_idx: usize,
    pub item_code: String,
    pub bit_offset: u64,
    pub segments: Vec<BitSegment>,
}

impl From<&Item> for TimelineEntry {
    fn from(item: &Item) -> Self {
        Self {
            item_idx: 0, // Placeholder, usually set during batch processing
            item_code: item.code.clone(),
            bit_offset: item.range.start,
            segments: item.segments.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymmetryReport {
    pub mismatch_offset: u64,
    pub rupture_field: String,
    pub dna_class: String,
    pub prescription: String,
    pub raw_mismatch: Option<String>,
}

pub fn analyze_symmetry(diff_json_path: &str, timeline_json_path: &str) -> Result<SymmetryReport> {
    let diff_content = fs::read_to_string(diff_json_path)
        .with_context(|| format!("Failed to read diff JSON: {}", diff_json_path))?;

    let mismatch_offset = extract_mismatch_offset(&diff_content)
        .context("Could not find mismatch_offset in diff JSON")?;

    let timeline_content = fs::read_to_string(timeline_json_path)
        .with_context(|| format!("Failed to read timeline JSON: {}", timeline_json_path))?;
    let entries: Vec<TimelineEntry> =
        serde_json::from_str(&timeline_content).context("Failed to parse timeline JSON")?;

    analyze_symmetry_memory(mismatch_offset, &entries)
}

pub fn analyze_symmetry_memory(
    mismatch_offset: u64,
    entries: &[TimelineEntry],
) -> Result<SymmetryReport> {
    if let Some((entry, seg, distance)) = resolve_segment_match(mismatch_offset, entries) {
        let dna_class = classify_dna(seg);
        let prescription = generate_prescription(&dna_class, seg);
        let raw_mismatch = if distance == 0 {
            None
        } else {
            Some(format!(
                "Nearest-field fallback: mismatch_offset {} landed {} bits away from '{}' in item '{}'",
                mismatch_offset, distance, seg.label, entry.item_code
            ))
        };

        return Ok(SymmetryReport {
            mismatch_offset,
            rupture_field: seg.label.clone(),
            dna_class,
            prescription,
            raw_mismatch,
        });
    }

    anyhow::bail!(
        "No field in timeline matches the mismatch_offset {}",
        mismatch_offset
    )
}

pub fn analyze_item_dna(item: &Item, mismatch_offset: u64) -> Result<SymmetryReport> {
    let mut entry = TimelineEntry::from(item);
    if entry.segments.is_empty() {
        entry.segments.push(BitSegment {
            start: 0,
            end: item.range.end.saturating_sub(item.range.start),
            label: format!("ItemBody::{}", item.code.trim()),
            depth: 0,
        });
    }

    analyze_symmetry_memory(mismatch_offset, &[entry])
}

fn classify_dna(seg: &BitSegment) -> String {
    let label = seg.label.to_lowercase();
    if label.contains("stat") || label.contains("prop") {
        "stat_alignment_drift".to_string()
    } else if label.contains("quantity") || label.contains("durability") {
        "compact_geometry_shift".to_string()
    } else {
        "bit_rhythm_rupture".to_string()
    }
}

fn generate_prescription(dna_class: &str, seg: &BitSegment) -> String {
    match dna_class {
        "stat_alignment_drift" => format!(
            "Rupture at '{}'. Check if Stat ID mapping or bit-width for this property has changed in Alpha v105.",
            seg.label
        ),
        "compact_geometry_shift" => format!(
            "Rupture at '{}'. Verify if this item type uses a different compact layout (is_compact) or alignment padding.",
            seg.label
        ),
        _ => format!(
            "Rupture at '{}'. Inspect field definition ({} bits) or predecessor bit consumption.",
            seg.label, seg.end - seg.start
        ),
    }
}

fn resolve_segment_match<'a>(
    mismatch_offset: u64,
    entries: &'a [TimelineEntry],
) -> Option<(&'a TimelineEntry, &'a BitSegment, u64)> {
    let mut best_match: Option<(&TimelineEntry, &BitSegment, u64)> = None;

    for entry in entries {
        for seg in &entry.segments {
            let abs_start = entry.bit_offset.saturating_add(seg.start);
            let abs_end = entry.bit_offset.saturating_add(seg.end);
            let distance = if mismatch_offset < abs_start {
                abs_start - mismatch_offset
            } else if mismatch_offset >= abs_end {
                mismatch_offset.saturating_sub(abs_end).saturating_add(1)
            } else {
                0
            };

            if distance == 0 {
                return Some((entry, seg, 0));
            }

            let replace = match best_match {
                None => true,
                Some((best_entry, best_seg, best_distance)) => {
                    distance < best_distance
                        || (distance == best_distance
                            && abs_start < best_entry.bit_offset.saturating_add(best_seg.start))
                }
            };

            if replace {
                best_match = Some((entry, seg, distance));
            }
        }
    }

    best_match
}

fn extract_mismatch_offset(content: &str) -> Option<u64> {
    // Look for "mismatch_offset": <number>
    if let Some(pos) = content.find("\"mismatch_offset\"") {
        let remainder = &content[pos + "\"mismatch_offset\"".len()..];
        if let Some(colon_pos) = remainder.find(':') {
            let value_part = &remainder[colon_pos + 1..];
            let end_pos = value_part
                .find(|c: char| {
                    !c.is_numeric() && c != ' ' && c != '\t' && c != '\n' && c != '\r' && c != ','
                })
                .unwrap_or(value_part.len());
            let val_str = value_part[..end_pos].trim();
            return val_str.parse::<u64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_item_dna_falls_back_to_nearest_segment_when_mismatch_lands_after_end() {
        let item = Item {
            code: "tsc".to_string(),
            range: crate::domain::item::ItemBitRange {
                start: 100,
                end: 180,
            },
            segments: vec![BitSegment {
                start: 0,
                end: 16,
                label: "AlphaPropertyResidueNudge".to_string(),
                depth: 0,
            }],
            ..Item::default()
        };

        let report = analyze_item_dna(&item, 140).expect("fallback diagnosis");
        assert_eq!(report.rupture_field, "AlphaPropertyResidueNudge");
        assert_eq!(report.dna_class, "stat_alignment_drift");
        assert!(report
            .raw_mismatch
            .as_deref()
            .unwrap_or("")
            .contains("Nearest-field fallback"));
    }

    #[test]
    fn analyze_item_dna_synthesizes_range_segment_when_segments_are_missing() {
        let item = Item {
            code: "ww".to_string(),
            range: crate::domain::item::ItemBitRange {
                start: 8061,
                end: 8180,
            },
            ..Item::default()
        };

        let report = analyze_item_dna(&item, 8180).expect("synthetic range diagnosis");
        assert_eq!(report.rupture_field, "ItemBody::ww");
        assert_eq!(report.dna_class, "bit_rhythm_rupture");
        assert!(report
            .raw_mismatch
            .as_deref()
            .unwrap_or("")
            .contains("Nearest-field fallback"));
    }
}
