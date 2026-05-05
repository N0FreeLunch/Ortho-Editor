use crate::save::{recalculate_checksum, Save};
use crate::verify::ReportIssue;
use crate::domain::item::axiom_meta::{ForensicAudit, FidelityScore};
use super::{DomainVerifier, DomainReport};

pub struct HeaderVerifier;

impl DomainVerifier for HeaderVerifier {
    fn verify(&self, bytes: &[u8], alpha_mode: bool) -> DomainReport {
        let mut issues = Vec::new();
        let audit = ForensicAudit::new();

        let save = match Save::from_bytes(bytes) {
            Ok(s) => s,
            Err(err) => {
                issues.push(ReportIssue {
                    kind: "header_parse".to_string(),
                    message: format!("Header parse: {}", err),
                    bit_offset: None,
                });
                return DomainReport {
                    issues,
                    audit,
                    fidelity_score: 0.0,
                };
            }
        };

        // 1. File Size Check
        let header_size = save.header.file_size as usize;
        let actual_size = bytes.len();
        if header_size != actual_size {
            issues.push(ReportIssue {
                kind: "file_size".to_string(),
                message: format!(
                    "File size header: {} bytes, actual: {} bytes",
                    header_size, actual_size
                ),
                bit_offset: None,
            });
        }

        // 2. Checksum Check
        let stored_checksum = save.header.checksum;
        match recalculate_checksum(bytes) {
            Ok(calculated_checksum) => {
                if stored_checksum != calculated_checksum {
                    issues.push(ReportIssue {
                        kind: if alpha_mode { "checksum_info".to_string() } else { "checksum".to_string() },
                        message: format!(
                            "stored=0x{:08X}, calculated=0x{:08X}",
                            stored_checksum, calculated_checksum
                        ),
                        bit_offset: None,
                    });
                }
            }
            Err(err) => {
                issues.push(ReportIssue {
                    kind: "checksum".to_string(),
                    message: format!("recalculation error: {}", err),
                    bit_offset: None,
                });
            }
        }

        DomainReport {
            issues,
            fidelity_score: FidelityScore::from_audit(&audit).value,
            audit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::v2::DomainVerifier;

    #[test]
    fn test_header_verifier_corrupted_bytes() {
        let verifier = HeaderVerifier;
        let bytes = vec![0u8; 10]; // Too small for header
        let report = verifier.verify(&bytes, false);
        assert!(!report.issues.is_empty());
        assert_eq!(report.issues[0].kind, "header_parse");
    }

    #[test]
    fn test_header_verifier_size_mismatch() {
        let verifier = HeaderVerifier;
        // Create a synthetic valid header but mutate the size
        let mut bytes = vec![0u8; 1024];
        // Minimal header structure to pass Save::from_bytes
        bytes[0..4].copy_from_slice(&0xAA55AA55u32.to_le_bytes()); // magic
        bytes[4..8].copy_from_slice(&0x00000060u32.to_le_bytes()); // version 96
        bytes[8..12].copy_from_slice(&2000u32.to_le_bytes()); // file size in header = 2000
        
        let report = verifier.verify(&bytes, false);
        // Should have size mismatch because actual is 1024
        assert!(report.issues.iter().any(|i| i.kind == "file_size"));
    }
}
