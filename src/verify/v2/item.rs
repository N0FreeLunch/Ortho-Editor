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
        for item in &items {
            forensic_audit.extend(item.forensic_audit.clone());
            let item_bits = match item.to_bytes(&huffman, alpha_mode) {
                Ok(b) => b,
                Err(e) => {
                    issues.push(ReportIssue {
                        kind: "item_parse".to_string(),
                        message: format!("Item to_bytes ({}): {}", item.code, e),
                        bit_offset: None,
                    });
                    continue;
                }
            };
            if let Err(e) = Item::from_bytes(&item_bits, &huffman, alpha_mode) {
                issues.push(ReportIssue {
                    kind: "item_parse".to_string(),
                    message: format!("Item round-trip parse failure ({}): {}", item.code, e),
                    bit_offset: parsing_error_offset(&e.error),
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
                if expected != items.len() {
                    issues.push(ReportIssue {
                        kind: "jm_coherence".into(),
                        message: format!("JM header count ({}) != parsed items ({})", expected, items.len()),
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

fn parsing_error_offset(error: &ParsingError) -> Option<u64> {
    match error {
        ParsingError::InvalidHuffmanBit { bit_offset }
        | ParsingError::UnexpectedSegmentEnd { bit_offset }
        | ParsingError::BitSymmetryFailure { bit_offset } => Some(*bit_offset),
        ParsingError::InvalidStatId { bit_offset, .. } => Some(*bit_offset),
        _ => None,
    }
}
