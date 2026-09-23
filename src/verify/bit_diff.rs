use super::{VerificationIssue, VerificationReport, Verifier};

pub struct BitDiffVerifier;

impl Verifier for BitDiffVerifier {
    fn verify(&self, fixture: &[u8], reproduced: &[u8]) -> VerificationReport {
        let mut issues = Vec::new();
        let common_len = fixture.len().min(reproduced.len());

        let mut current_start: Option<u64> = None;

        for i in 0..common_len {
            if fixture[i] != reproduced[i] {
                let diff = fixture[i] ^ reproduced[i];
                for bit in 0..8 {
                    let bit_offset = (i as u64 * 8) + bit as u64;
                    let bit_diff = (diff >> bit) & 1 != 0;

                    if bit_diff {
                        if current_start.is_none() {
                            current_start = Some(bit_offset);
                        }
                    } else {
                        if let Some(start) = current_start.take() {
                            issues.push(self.create_issue(
                                start,
                                bit_offset - start,
                                fixture,
                                reproduced,
                            ));
                        }
                    }
                }
            } else {
                if let Some(start) = current_start.take() {
                    issues.push(self.create_issue(
                        start,
                        (i as u64 * 8) - start,
                        fixture,
                        reproduced,
                    ));
                }
            }
        }

        if let Some(start) = current_start {
            issues.push(self.create_issue(
                start,
                (common_len as u64 * 8) - start,
                fixture,
                reproduced,
            ));
        }

        if fixture.len() != reproduced.len() {
            let bit_offset = common_len as u64 * 8;
            let fixture_rem = if fixture.len() > common_len {
                &fixture[common_len..]
            } else {
                &[]
            };
            let reproduced_rem = if reproduced.len() > common_len {
                &reproduced[common_len..]
            } else {
                &[]
            };

            issues.push(VerificationIssue {
                bit_offset,
                bit_length: (fixture_rem.len() as u64 + reproduced_rem.len() as u64) * 8,
                expected: fixture_rem.to_vec(),
                actual: reproduced_rem.to_vec(),
                label: None,
                message: format!(
                    "Length mismatch: fixture={} bytes, reproduced={} bytes",
                    fixture.len(),
                    reproduced.len()
                ),
            });
        }

        if issues.is_empty() {
            VerificationReport::success()
        } else {
            VerificationReport::failure(issues)
        }
    }
}

impl BitDiffVerifier {
    fn create_issue(
        &self,
        bit_offset: u64,
        bit_length: u64,
        fixture: &[u8],
        reproduced: &[u8],
    ) -> VerificationIssue {
        let byte_start = (bit_offset / 8) as usize;
        let byte_end = ((bit_offset + bit_length + 7) / 8) as usize;

        VerificationIssue {
            bit_offset,
            bit_length,
            expected: fixture[byte_start..byte_end.min(fixture.len())].to_vec(),
            actual: reproduced[byte_start..byte_end.min(reproduced.len())].to_vec(),
            label: None,
            message: format!("Bit mismatch at offset {} (len={})", bit_offset, bit_length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_diff_identical() {
        let verifier = BitDiffVerifier;
        let data = vec![0xAA, 0x55, 0xFF, 0x00];
        let report = verifier.verify(&data, &data);
        assert!(report.is_success);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn test_bit_diff_single_bit() {
        let verifier = BitDiffVerifier;
        let fixture = vec![0b0000_0001];
        let reproduced = vec![0b0000_0011]; // Bit 1 differs
        let report = verifier.verify(&fixture, &reproduced);
        assert!(!report.is_success);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].bit_offset, 1);
        assert_eq!(report.issues[0].bit_length, 1);
    }

    #[test]
    fn test_bit_diff_multi_bit_span() {
        let verifier = BitDiffVerifier;
        let fixture = vec![0b0000_0000, 0b0000_0000];
        let reproduced = vec![0b1100_0000, 0b0000_0011]; // Bits 6,7, 8,9 differ
        let report = verifier.verify(&fixture, &reproduced);
        assert!(!report.is_success);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].bit_offset, 6);
        assert_eq!(report.issues[0].bit_length, 4);
    }

    #[test]
    fn test_bit_diff_length_mismatch() {
        let verifier = BitDiffVerifier;
        let fixture = vec![0xAA];
        let reproduced = vec![0xAA, 0xBB];
        let report = verifier.verify(&fixture, &reproduced);
        assert!(!report.is_success);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].bit_offset, 8);
        assert_eq!(report.issues[0].actual, vec![0xBB]);
    }
}
