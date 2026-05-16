use super::{DomainReport, DomainVerifier};
use crate::domain::item::axiom_meta::{ForensicAudit, FidelityScore};
use crate::item::{HuffmanTree, Item, ParsingError};
use crate::save::find_jm_markers;
use crate::verify::ReportIssue;

pub struct ItemVerifier;

impl DomainVerifier for ItemVerifier {
    fn verify(&self, bytes: &[u8], alpha_mode: bool) -> DomainReport {
        let mut issues = Vec::new();
        let huffman = HuffmanTree::new();
        let mut forensic_audit = ForensicAudit::new();

        // 1. Parse items
        let items = match Item::read_player_items(bytes, &huffman, alpha_mode) {
            Ok(items) => items,
            Err(err) => {
                issues.push(ReportIssue {
                    kind: "item_parse".to_string(),
                    message: format!("{}", err),
                    bit_offset: parsing_error_offset(&err.error),
                });
                return DomainReport {
                    issues,
                    audit: forensic_audit,
                    fidelity_score: 0.0,
                };
            }
        };

        // 2. Round-trip validation
        for (idx, item) in items.iter().enumerate() {
            forensic_audit.extend(item.forensic_audit.clone());
            
            // Slice 4: Opaque/SemiOpaque items are preserved as-is; skip round-trip parse check 
            // if we already know they are in a forensic isolation state.
            if item.is_opaque() || item.is_semi_opaque() {
                continue;
            }

            let item_bits_vec = match item.to_bits(idx, &huffman, alpha_mode) {
                Ok(b) => b,
                Err(e) => {
                    issues.push(ReportIssue {
                        kind: "item_parse".to_string(),
                        message: format!("Item to_bits ({}): {}", item.code, e),
                        bit_offset: None,
                    });
                    continue;
                }
            };
            if let Err(e) = Item::from_bytes(&item.to_bytes(idx, &huffman, alpha_mode).unwrap(), &huffman, alpha_mode) {
                issues.push(ReportIssue {
                    kind: "item_parse".to_string(),
                    message: format!("Item round-trip parse failure ({}): {}", item.code, e),
                    bit_offset: parsing_error_offset(&e.error),
                });
            }

            let original_bits: Vec<bool> = item.bits.iter().map(|rb| rb.bit).collect();
            let emitted_bits = item_bits_vec;
            
            let mut mismatch_idx = None;
            let len = original_bits.len().min(emitted_bits.len());
            for i in 0..len {
                if original_bits[i] != emitted_bits[i] {
                    mismatch_idx = Some(i);
                    break;
                }
            }

            if mismatch_idx.is_some() || original_bits.len() != emitted_bits.len() {
                let idx = mismatch_idx.unwrap_or(len);
                issues.push(ReportIssue {
                    kind: "item_parity".to_string(),
                    message: format!(
                        "Item parity mismatch ({}): mismatch at bit {} (Orig Len: {}, Emit Len: {}). Bits near mismatch: Original: {}, Emitted: {}",
                        item.code.trim(),
                        idx,
                        original_bits.len(),
                        emitted_bits.len(),
                        format_bits_at(&original_bits, idx, 32),
                        format_bits_at(&emitted_bits, idx, 32)
                    ),
                    bit_offset: Some(item.range.start),
                });
            }
        }

        // 3. JM Markers & Coherence
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
                let semantic_count = items.iter().filter(|it| !it.is_residue()).count();
                if expected != semantic_count {
                    issues.push(ReportIssue {
                        kind: "jm_coherence".into(),
                        message: format!("JM header count ({}) != parsed items ({})", expected, semantic_count),
                        bit_offset: Some((jm0 + 2) as u64 * 8),
                    });
                }
            }
        }

        let fidelity_score = FidelityScore::from_audit(&forensic_audit).value;

        DomainReport {
            issues,
            audit: forensic_audit,
            fidelity_score,
        }
    }
}

fn format_bits_at(bits: &[bool], start: usize, count: usize) -> String {
    let mut s = String::new();
    let display_start = if start > 8 { start - 8 } else { 0 };
    for i in display_start..(start + count).min(bits.len()) {
        if i == start { s.push('['); }
        s.push(if bits[i] { '1' } else { '0' });
        if i == start { s.push(']'); }
        if (i + 1) % 8 == 0 { s.push(' '); }
    }
    s
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
