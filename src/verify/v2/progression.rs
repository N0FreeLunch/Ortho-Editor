use crate::verify::v2::{DomainVerifier, DomainReport};
use crate::verify::ReportIssue;
use crate::domain::progression::Progression;
use crate::domain::item::axiom_meta::FidelityScore;

pub struct ProgressionVerifier;

impl DomainVerifier for ProgressionVerifier {
    fn verify(&self, bytes: &[u8], alpha_mode: bool) -> DomainReport {
        let res = Progression::from_bytes(bytes, alpha_mode);
        
        let mut issues = Vec::new();
        let audit = res.audit;
        
        match res.value {
            Ok(prog) => {
                // In Slice 2, we propagate audit findings as issues for visibility.
                for finding in &audit.findings {
                    issues.push(ReportIssue {
                        kind: "progression_audit".to_string(),
                        message: format!("[{:?}] {}", finding.confidence, finding.rationale),
                        bit_offset: None,
                    });
                }

                // In Slice 3, we add semantic issues for additive CLI output.
                let completed_act5: Vec<String> = prog.quests.quests()
                    .iter()
                    .filter(|q| q.difficulty() == prog.difficulty && q.act() == 5 && q.is_completed())
                    .map(|q| q.name().to_string())
                    .collect();
                
                if !completed_act5.is_empty() {
                    issues.push(ReportIssue {
                        kind: "progression_semantic".to_string(),
                        message: format!("Completed Quests (Act 5): {}", completed_act5.join(", ")),
                        bit_offset: None,
                    });
                }

                let active_waypoints: Vec<String> = prog.waypoints.waypoints()
                    .iter()
                    .filter(|w| w.is_active())
                    .map(|w| w.name().to_string())
                    .collect();

                if !active_waypoints.is_empty() {
                    issues.push(ReportIssue {
                        kind: "progression_semantic".to_string(),
                        message: format!("Active Waypoints: {}", active_waypoints.join(", ")),
                        bit_offset: None,
                    });
                }
            }
            Err(e) => {
                issues.push(ReportIssue {
                    kind: "progression_parse".to_string(),
                    message: e,
                    bit_offset: None,
                });
            }
        }
        
        DomainReport {
            issues,
            audit: audit.clone(),
            fidelity_score: FidelityScore::from_audit(&audit).value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::progression::axiom::PROG_START_FILE;

    #[test]
    fn test_progression_verifier_minimal() {
        let verifier = ProgressionVerifier;
        let bytes = vec![0u8; PROG_START_FILE + 100];
        let report = verifier.verify(&bytes, true);
        
        assert!(!report.audit.findings.is_empty(), "Should record progression axioms");
        assert!(report.fidelity_score > 0.0);
    }

    #[test]
    fn test_progression_verifier_retail_smoke() {
        let verifier = ProgressionVerifier;
        let bytes = vec![0u8; 1000];
        let report = verifier.verify(&bytes, false);
        
        // Retail path currently returns empty audit but Ok value
        assert!(report.issues.is_empty());
        assert_eq!(report.fidelity_score, 1.0);
    }
}
