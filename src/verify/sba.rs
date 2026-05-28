use serde::{Serialize, Deserialize};
use anyhow::{Result};
use crate::item::{Item, BitSegment, ItemBitRange};
use crate::verify::{ReportIssue};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SbaBaseline {
    pub fixture: String,
    pub items: Vec<SbaItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SbaItem {
    pub path: String,
    pub code: String,
    pub range: ItemBitRange,
    pub segments: Vec<BitSegment>,
    pub bits: Vec<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SbaJsonPayload {
    pub fixture: String,
    pub baseline: String,
    pub mode: String,
    pub item_count: usize,
    pub mismatch_count: usize,
}

pub fn flatten_item(item: &Item, path: &str, result: &mut Vec<SbaItem>) {
    result.push(SbaItem {
        path: path.to_string(),
        code: item.code.clone(),
        range: item.range,
        segments: item.segments.clone(),
        bits: item.bits.iter().map(|rb| rb.bit).collect(),
    });

    for (i, socketed) in item.socketed_items.iter().enumerate() {
        let sub_path = format!("{}.{}", path, i);
        flatten_item(socketed, &sub_path, result);
    }
}

pub fn verify_baseline(expected: &SbaBaseline, actual: &SbaBaseline, issues: &mut Vec<ReportIssue>) -> Result<()> {
    if expected.items.len() != actual.items.len() {
        let msg = format!(
            "Item count mismatch: expected {}, found {}",
            expected.items.len(),
            actual.items.len()
        );
        issues.push(ReportIssue { kind: "structure".to_string(), message: msg.clone(), bit_offset: None });
        anyhow::bail!(msg);
    }

    for (i, (exp_item, act_item)) in expected.items.iter().zip(actual.items.iter()).enumerate() {
        if exp_item.path != act_item.path {
            let msg = format!("Item #{} path mismatch: expected {}, found {}", i, exp_item.path, act_item.path);
            issues.push(ReportIssue { kind: "structure".to_string(), message: msg.clone(), bit_offset: None });
            anyhow::bail!(msg);
        }
        if exp_item.code != act_item.code {
            let msg = format!("Item {} code mismatch: expected {}, found {}", exp_item.path, exp_item.code, act_item.code);
            issues.push(ReportIssue { kind: "data".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start) });
            anyhow::bail!(msg);
        }
        
        if exp_item.segments.len() != act_item.segments.len() {
            let msg = format!(
                "Item {} segment count mismatch: expected {}, found {}",
                exp_item.path,
                exp_item.segments.len(),
                act_item.segments.len()
            );
            issues.push(ReportIssue { kind: "structural_segment".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start) });
            anyhow::bail!(msg);
        }

        for (j, (exp_seg, act_seg)) in exp_item.segments.iter().zip(act_item.segments.iter()).enumerate() {
            if exp_seg.label != act_seg.label {
                let msg = format!(
                    "Item {} segment #{} label mismatch: expected {}, found {}",
                    exp_item.path, j, exp_seg.label, act_seg.label
                );
                issues.push(ReportIssue { kind: "structural_label".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start + act_seg.start as u64) });
                anyhow::bail!(msg);
            }
            if exp_seg.start != act_seg.start || exp_seg.end != act_seg.end {
                let msg = format!(
                    "Item {} segment #{} ({}) bit range mismatch: expected {}-{}, found {}-{}",
                    exp_item.path, j, exp_seg.label, exp_seg.start, exp_seg.end, act_seg.start, act_seg.end
                );
                issues.push(ReportIssue { kind: "structural_range".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start + act_seg.start as u64) });
                anyhow::bail!(msg);
            }

            // Value comparison
            let exp_start = exp_seg.start as usize;
            let exp_end = exp_seg.end as usize;
            let act_start = act_seg.start as usize;
            let act_end = act_seg.end as usize;

            if exp_start > exp_item.bits.len() || exp_end > exp_item.bits.len() || exp_start > exp_end {
                let msg = format!(
                    "Item {} segment #{} ({}) expected range {}-{} out of bounds (bits len: {})",
                    exp_item.path, j, exp_seg.label, exp_start, exp_end, exp_item.bits.len()
                );
                issues.push(ReportIssue { kind: "structural_oob".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start + act_seg.start as u64) });
                anyhow::bail!(msg);
            }
            if act_start > act_item.bits.len() || act_end > act_item.bits.len() || act_start > act_end {
                let msg = format!(
                    "Item {} segment #{} ({}) actual range {}-{} out of bounds (bits len: {})",
                    act_item.path, j, act_seg.label, act_start, act_end, act_item.bits.len()
                );
                issues.push(ReportIssue { kind: "structural_oob".to_string(), message: msg.clone(), bit_offset: Some(act_item.range.start + act_seg.start as u64) });
                anyhow::bail!(msg);
            }

            let exp_bits = &exp_item.bits[exp_start..exp_end];
            let act_bits = &act_item.bits[act_start..act_end];
            if exp_bits != act_bits {
                let msg = format!(
                    "Item {} segment #{} ({}) value mismatch",
                    exp_item.path, j, exp_seg.label
                );
                issues.push(ReportIssue {
                    kind: "structural_value".to_string(),
                    message: msg.clone(),
                    bit_offset: Some(act_item.range.start + act_seg.start as u64),
                });
                anyhow::bail!(msg);
            }
        }
    }

    Ok(())
}
