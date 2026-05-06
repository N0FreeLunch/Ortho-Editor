use bitstream_io::{BitRead, BitReader, LittleEndian};
use serde::Serialize;
use std::io::Cursor;

use crate::item::{HuffmanTree, Item};
use crate::domain::item::axiom_meta::{FidelityScore, ForensicAudit};

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiffReport {
    pub success: bool,
    pub operation: String,
    pub item_count_a: usize,
    pub item_count_b: usize,
    pub items: Vec<ItemDiff>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ItemDiff {
    pub label: String,
    pub code: String,
    pub is_match: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch_type: Option<String>,
    pub original_len: usize,
    pub target_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_mismatch_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
    pub fidelity_score: f32,
    pub forensic_audit: ForensicAudit,
    pub version: u8,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_header_gap: Option<u32>,
    pub alpha_alignment_padding_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_bits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bits: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ItemDiff>,
}

pub fn calculate_symmetry_diff(bytes_a: &[u8], bytes_b: Option<&[u8]>, roundtrip: bool) -> anyhow::Result<DiffReport> {
    let huffman = HuffmanTree::new();
    let is_alpha_a = is_alpha(bytes_a);
    let mut report = DiffReport {
        operation: if roundtrip { "roundtrip" } else { "compare" }.to_string(),
        ..Default::default()
    };

    if roundtrip {
        let items = Item::read_player_items(bytes_a, &huffman, is_alpha_a)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        report.item_count_a = items.len();
        report.item_count_b = items.len();
        for (i, item) in items.iter().enumerate() {
            report.items.push(compare_item_with_reserialized(
                item,
                &huffman,
                is_alpha_a,
                format!("Item {}", i),
            ));
        }
    } else {
        let bytes_b = bytes_b.ok_or_else(|| anyhow::anyhow!("file_b is required when roundtrip is false"))?;
        let is_alpha_b = is_alpha(bytes_b);
        let items_a = Item::read_player_items(bytes_a, &huffman, is_alpha_a)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let items_b = Item::read_player_items(bytes_b, &huffman, is_alpha_b)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        report.item_count_a = items_a.len();
        report.item_count_b = items_b.len();
        for i in 0..items_a.len().min(items_b.len()) {
            report
                .items
                .push(compare_two_items(&items_a[i], &items_b[i], format!("Item {}", i)));
        }
    }

    report.success = report.item_count_a == report.item_count_b && report.items.iter().all(|i| i.is_match);
    Ok(report)
}

fn is_alpha(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    version == 105 || version == 6
}

fn compare_item_with_reserialized(item: &Item, huffman: &HuffmanTree, alpha_mode: bool, label: String) -> ItemDiff {
    let mut strict_item = item.clone();
    strict_item.bits.clear();
    let reserialized_bytes = strict_item.to_bytes(huffman, alpha_mode).unwrap_or_default();
    let original_bits = &item.bits;

    let mut rebuilt_bits = Vec::new();
    let mut reader = BitReader::endian(Cursor::new(&reserialized_bytes), LittleEndian);
    for _ in 0..original_bits.len() {
        if let Ok(bit) = reader.read_bit() {
            rebuilt_bits.push(bit);
        } else {
            break;
        }
    }

    let mut mismatch_idx = None;
    for i in 0..original_bits.len().min(rebuilt_bits.len()) {
        if original_bits[i].bit != rebuilt_bits[i] {
            mismatch_idx = Some(i);
            break;
        }
    }

    let mut item_diff = ItemDiff {
        label,
        code: item.code.trim().to_string(),
        original_len: original_bits.len(),
        target_len: rebuilt_bits.len(),
        fidelity_score: FidelityScore::from_audit(&item.forensic_audit).value,
        forensic_audit: item.forensic_audit.clone(),
        version: item.header.version,
        flags: item.header.flags,
        alpha_header_gap: item.body.alpha_header_gap,
        alpha_alignment_padding_len: item.body.alpha_alignment_padding.len(),
        orig_bits: Some(original_bits.iter().map(|b| if b.bit { '1' } else { '0' }).collect()),
        target_bits: Some(rebuilt_bits.iter().map(|&b| if b { '1' } else { '0' }).collect()),
        ..Default::default()
    };

    if mismatch_idx.is_some() || original_bits.len() != rebuilt_bits.len() {
        item_diff.is_match = false;
        let mut m_type = if original_bits.len() != rebuilt_bits.len() {
            "Length".to_string()
        } else {
            "Content".to_string()
        };

        let len_diff = (original_bits.len() as i32 - rebuilt_bits.len() as i32).abs();
        if len_diff == 2 {
            m_type.push_str(" [Nudge (2-bit)]");
        } else if len_diff > 0 && len_diff % 16 == 0 {
            m_type.push_str(&format!(" [RW-Gap ({}-bit)]", len_diff));
        }

        item_diff.mismatch_type = Some(m_type);
        if let Some(idx) = mismatch_idx {
            item_diff.first_mismatch_offset = Some(idx as u64);
            item_diff.segment = Some(
                item
                    .query_bit(idx as u64)
                    .map(|s| s.label)
                    .unwrap_or_else(|| "Unknown".to_string()),
            );
        }
    } else {
        item_diff.is_match = true;
    }

    for (i, child) in item.socketed_items.iter().enumerate() {
        item_diff.children.push(compare_item_with_reserialized(
            child,
            huffman,
            alpha_mode,
            format!("Child {}", i),
        ));
    }
    if !item_diff.children.iter().all(|c| c.is_match) {
        item_diff.is_match = false;
    }
    item_diff
}

fn compare_two_items(item_a: &Item, item_b: &Item, label: String) -> ItemDiff {
    let mut item_diff = ItemDiff {
        label,
        code: item_a.code.trim().to_string(),
        original_len: item_a.bits.len(),
        target_len: item_b.bits.len(),
        fidelity_score: FidelityScore::from_audit(&item_a.forensic_audit).value,
        forensic_audit: item_a.forensic_audit.clone(),
        version: item_a.header.version,
        flags: item_a.header.flags,
        alpha_header_gap: item_a.body.alpha_header_gap,
        alpha_alignment_padding_len: item_a.body.alpha_alignment_padding.len(),
        orig_bits: Some(item_a.bits.iter().map(|b| if b.bit { '1' } else { '0' }).collect()),
        target_bits: Some(item_b.bits.iter().map(|b| if b.bit { '1' } else { '0' }).collect()),
        ..Default::default()
    };

    if item_a.bits.len() != item_b.bits.len() {
        item_diff.is_match = false;
        let mut m_type = "Length".to_string();
        let len_diff = (item_a.bits.len() as i32 - item_b.bits.len() as i32).abs();
        if len_diff == 2 {
            m_type.push_str(" [Nudge (2-bit)]");
        } else if len_diff > 0 && len_diff % 16 == 0 {
            m_type.push_str(&format!(" [RW-Gap ({}-bit)]", len_diff));
        }
        item_diff.mismatch_type = Some(m_type);
    } else {
        let mut mismatch_idx = None;
        for i in 0..item_a.bits.len() {
            if item_a.bits[i].bit != item_b.bits[i].bit {
                mismatch_idx = Some(i);
                break;
            }
        }
        if let Some(idx) = mismatch_idx {
            item_diff.is_match = false;
            item_diff.mismatch_type = Some("Content".to_string());
            item_diff.first_mismatch_offset = Some(idx as u64);
            item_diff.segment = Some(
                item_a
                    .query_bit(idx as u64)
                    .map(|s| s.label)
                    .unwrap_or_else(|| "Unknown".to_string()),
            );
        } else {
            item_diff.is_match = true;
        }
    }

    for i in 0..item_a.socketed_items.len().max(item_b.socketed_items.len()) {
        if i < item_a.socketed_items.len() && i < item_b.socketed_items.len() {
            item_diff.children.push(compare_two_items(
                &item_a.socketed_items[i],
                &item_b.socketed_items[i],
                format!("Child {}", i),
            ));
        } else {
            item_diff.is_match = false;
            item_diff.children.push(ItemDiff {
                label: format!("Child {}", i),
                code: if i < item_a.socketed_items.len() {
                    item_a.socketed_items[i].code.trim().to_string()
                } else {
                    item_b.socketed_items[i].code.trim().to_string()
                },
                is_match: false,
                mismatch_type: Some("ChildCount".to_string()),
                original_len: if i < item_a.socketed_items.len() {
                    item_a.socketed_items[i].bits.len()
                } else {
                    0
                },
                target_len: if i < item_b.socketed_items.len() {
                    item_b.socketed_items[i].bits.len()
                } else {
                    0
                },
                ..Default::default()
            });
        }
    }
    if !item_diff.children.iter().all(|c| c.is_match) {
        item_diff.is_match = false;
    }
    item_diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::entity::{Item, RecordedBit};

    #[test]
    fn test_mismatch_labeling() {
        let mut item_a = Item::default();
        item_a.code = "test".to_string();
        for i in 0..100 {
            item_a.bits.push(RecordedBit { bit: false, offset: i as u64 });
        }
        
        let mut item_b = item_a.clone();
        
        // 2-bit diff
        item_b.bits.truncate(98);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string());
        assert_eq!(diff.mismatch_type.unwrap(), "Length [Nudge (2-bit)]");
        
        // 16-bit diff
        item_b.bits.truncate(84);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string());
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (16-bit)]");

        // 32-bit diff
        item_b.bits.truncate(68);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string());
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (32-bit)]");
    }
}
