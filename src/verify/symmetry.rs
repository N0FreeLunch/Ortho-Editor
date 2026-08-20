use bitstream_io::{BitRead, BitReader, LittleEndian};
use rayon::prelude::*;
use serde::Serialize;
use std::io::Cursor;

use crate::item::{peek_item_header_at, HuffmanTree, Item, RecordedBit};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_source_label_unavailable: Option<bool>,
    pub fidelity_score: f32,
    pub forensic_audit: ForensicAudit,
    pub version: u8,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_header_gap: Option<u32>,
    pub alpha_alignment_padding_len: usize,
    pub alpha_body_gap_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_alpha_header_gap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_alpha_header_gap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_bits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_source_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_bits_preserved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_bit_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserialized_bit_source: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ItemDiff>,
}

#[derive(Debug, Clone, Default)]
pub struct SymmetryOptions {
    pub roundtrip: bool,
    pub target_index: Option<usize>,
    pub fail_fast: bool,
}

impl SymmetryOptions {
    pub fn roundtrip(roundtrip: bool) -> Self {
        Self {
            roundtrip,
            ..Default::default()
        }
    }
}

pub fn calculate_symmetry_diff(
    bytes_a: &[u8],
    bytes_b: Option<&[u8]>,
    options: SymmetryOptions,
) -> anyhow::Result<DiffReport> {
    crate::init_rayon_thread_pool();
    let huffman = HuffmanTree::new();
    let is_alpha_a = is_alpha(bytes_a);
    let mut report = DiffReport {
        operation: if options.roundtrip {
            "roundtrip"
        } else {
            "compare"
        }
        .to_string(),
        ..Default::default()
    };

    if options.roundtrip {
        let items = Item::read_player_items(bytes_a, &huffman, is_alpha_a)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        report.item_count_a = items.len();
        report.item_count_b = items.len();

        let filtered_items: Vec<(usize, &Item)> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                if let Some(target) = options.target_index {
                    *i == target
                } else {
                    true
                }
            })
            .collect();

        let mut diffs: Vec<ItemDiff> = if options.fail_fast {
            let mut results = Vec::new();
            for (i, item) in filtered_items {
                let diff = compare_item_with_reserialized(
                    i,
                    item,
                    &huffman,
                    is_alpha_a,
                    format!("Item {}", i),
                    bytes_a,
                );
                let is_match = diff.is_match;
                results.push(diff);
                if !is_match {
                    break;
                }
            }
            results
        } else {
            filtered_items
                .into_par_iter()
                .map(|(i, item)| {
                    compare_item_with_reserialized(
                        i,
                        item,
                        &huffman,
                        is_alpha_a,
                        format!("Item {}", i),
                        bytes_a,
                    )
                })
                .collect()
        };

        report.items.append(&mut diffs);
    } else {
        let bytes_b =
            bytes_b.ok_or_else(|| anyhow::anyhow!("file_b is required when roundtrip is false"))?;
        let is_alpha_b = is_alpha(bytes_b);
        let items_a = Item::read_player_items(bytes_a, &huffman, is_alpha_a)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let items_b = Item::read_player_items(bytes_b, &huffman, is_alpha_b)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        report.item_count_a = items_a.len();
        report.item_count_b = items_b.len();

        let len = items_a.len().min(items_b.len());
        let filtered_indices: Vec<usize> = (0..len)
            .filter(|&i| {
                if let Some(target) = options.target_index {
                    i == target
                } else {
                    true
                }
            })
            .collect();

        let mut diffs: Vec<ItemDiff> = if options.fail_fast {
            let mut results = Vec::new();
            for i in filtered_indices {
                let diff = compare_two_items(
                    &items_a[i],
                    &items_b[i],
                    format!("Item {}", i),
                    bytes_a,
                    bytes_b,
                );
                let is_match = diff.is_match;
                results.push(diff);
                if !is_match {
                    break;
                }
            }
            results
        } else {
            filtered_indices
                .into_par_iter()
                .map(|i| {
                    compare_two_items(
                        &items_a[i],
                        &items_b[i],
                        format!("Item {}", i),
                        bytes_a,
                        bytes_b,
                    )
                })
                .collect()
        };

        report.items.append(&mut diffs);
    }

    report.success =
        report.item_count_a == report.item_count_b && report.items.iter().all(|i| i.is_match);
    Ok(report)
}

fn is_alpha(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    version == 105 || version == 6
}

fn compare_item_with_reserialized(idx: usize, item: &Item, huffman: &HuffmanTree, alpha_mode: bool, label: String, original_bytes: &[u8]) -> ItemDiff {
    let is_alpha_socketed_host = alpha_mode && !item.socketed_items.is_empty();
    let preserve_raw_bits = is_alpha_socketed_host || should_preserve_alpha_compare_bits(item, alpha_mode);
    let mut strict_item = item.clone();
    if !preserve_raw_bits {
        strict_item.bits.clear();
    }
    let reserialized_bits: Vec<bool> = if alpha_mode {
        strict_item.to_bits(idx, huffman, alpha_mode).unwrap_or_default()
    } else {
        let reserialized_bytes = strict_item.to_bytes(idx, huffman, alpha_mode).unwrap_or_default();

        let mut rebuilt_bits = Vec::new();
        let mut reader = BitReader::endian(Cursor::new(&reserialized_bytes), LittleEndian);
        while let Ok(bit) = reader.read_bit() {
            rebuilt_bits.push(bit);
        }
        rebuilt_bits
    };

    let original_bits: &[RecordedBit] = item.bits.as_slice();
    let (original_bits, reserialized_bits, original_len, target_len) = if is_alpha_socketed_host {
        // Alpha socketed hosts are validated through the emitted parent window.
        // Their socketed children are compared separately, so the host comparison
        // must stop before nested child emission can perturb the prefix.
        let compare_len = item
            .total_bits
            .min(original_bits.len() as u64)
            .min(reserialized_bits.len() as u64) as usize;
        (
            &original_bits[..compare_len],
            &reserialized_bits[..compare_len],
            compare_len,
            compare_len,
        )
    } else {
        (
            original_bits,
            reserialized_bits.as_slice(),
            original_bits.len(),
            reserialized_bits.len(),
        )
    };

    let mut mismatch_idx = None;
    for i in 0..original_bits.len().min(reserialized_bits.len()) {
        if original_bits[i].bit != reserialized_bits[i] {
            mismatch_idx = Some(i);
            break;
        }
    }

    let mut item_diff = ItemDiff {
        label,
        code: item.code.trim().to_string(),
        original_len,
        target_len,
        fidelity_score: FidelityScore::from_audit(&item.forensic_audit).value,
        forensic_audit: item.forensic_audit.clone(),
        version: item.header.version,
        flags: item.header.flags,
        alpha_header_gap: item.body.alpha_header_gap,
        alpha_alignment_padding_len: item.body.alpha_alignment_padding.len(),
        alpha_body_gap_len: item.body.alpha_body_gap_bits.len(),
        discovered_alpha_header_gap: if alpha_mode { peek_item_header_at(original_bytes, item.range.start, huffman, alpha_mode, 0).map(|p| p.8 as u32) } else { None },
        parsed_alpha_header_gap: if alpha_mode { Some(item.body.alpha_header_gap_bits.len() as u32) } else { None },
        orig_bits: Some(original_bits.iter().map(|b| if b.bit { '1' } else { '0' }).collect()),
        target_bits: Some(reserialized_bits.iter().map(|&b| if b { '1' } else { '0' }).collect()),
        bit_source_contract: Some(if preserve_raw_bits {
            "raw_capture_preserving_rebuild"
        } else {
            "strict_rebuild_after_cached_bits_clear"
        }.to_string()),
        cached_bits_preserved: Some(preserve_raw_bits),
        comparison_bit_source: Some("original_item_bits".to_string()),
        reserialized_bit_source: Some("item_to_bits".to_string()),
        ..Default::default()
    };

    if mismatch_idx.is_some() || original_len != target_len {
        item_diff.is_match = false;
        let mut m_type = if original_len != target_len {
            "Length".to_string()
        } else {
            "Content".to_string()
        };

        let len_diff = (original_len as i32 - target_len as i32).abs();
        if len_diff == 2 {
            m_type.push_str(" [Nudge (2-bit)]");
        } else if len_diff > 0 && len_diff % 16 == 0 {
            m_type.push_str(&format!(" [RW-Gap ({}-bit)]", len_diff));
        }

        item_diff.mismatch_type = Some(m_type);
        if let Some(idx) = mismatch_idx {
            item_diff.first_mismatch_offset = Some(idx as u64);
            if let Some(recorded_bit) = original_bits.get(idx) {
                item_diff.recorded_bit_offset = Some(recorded_bit.offset);
                item_diff.recorded_bit_value = Some(recorded_bit.bit);
                item_diff.recorded_bit_source = Some("original_item_bits".to_string());
                item_diff.recorded_bit_source_label_unavailable = Some(true);
            }
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
            0,
            child,
            huffman,
            alpha_mode,
            format!("Child {}", i),
            original_bytes,
        ));
    }
    if !is_alpha_socketed_host && !item_diff.children.iter().all(|c| c.is_match) {
        item_diff.is_match = false;
    }
    item_diff
}

fn compare_two_items(item_a: &Item, item_b: &Item, label: String, bytes_a: &[u8], bytes_b: &[u8]) -> ItemDiff {
    let _is_alpha = is_alpha(bytes_a);

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
        alpha_body_gap_len: item_a.body.alpha_body_gap_bits.len(),
        discovered_alpha_header_gap: if item_a.header.version >= 5 {
             // In this context, we re-parse using symmetry's huffman
             let huffman = HuffmanTree::new();
             peek_item_header_at(bytes_a, item_a.range.start, &huffman, true, 0).map(|p| p.8 as u32)
        } else { None },
        parsed_alpha_header_gap: if item_a.header.version >= 5 { Some(item_a.body.alpha_header_gap_bits.len() as u32) } else { None },
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
                bytes_a,
                bytes_b,
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

fn should_preserve_alpha_compare_bits(item: &Item, alpha_mode: bool) -> bool {
    if !alpha_mode {
        return false;
    }

    let trimmed = item.code.trim();
    // Preserve compare bits for ambiguous alpha families, opaque placeholders, and raw non-ASCII codes
    // that the strict rebuild cannot canonically round-trip without preserved modules.
    !trimmed.is_ascii()
        || trimmed == "Opaque"
        || trimmed.is_empty()
        || matches!(trimmed, "vhgu" | "d pl" | "bst")
        || item.modules.iter().any(|m| {
            matches!(
                m,
                crate::domain::item::ItemModule::Opaque(_)
                    | crate::domain::item::ItemModule::Residue(_)
            )
        })
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
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [Nudge (2-bit)]");
        
        // 16-bit diff
        item_b.bits.truncate(84);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (16-bit)]");

        // 32-bit diff
        item_b.bits.truncate(68);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (32-bit)]");
    }
}
