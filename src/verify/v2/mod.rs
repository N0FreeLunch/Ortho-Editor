use crate::verify::ReportIssue;
use crate::domain::item::axiom_meta::ForensicAudit;

pub mod header;
pub mod progression;
pub mod item;

#[derive(Debug, Clone)]
pub struct DomainReport {
    pub issues: Vec<ReportIssue>,
    pub audit: ForensicAudit,
    pub fidelity_score: f32,
    pub rhythmic_fidelity: f32,
}

pub trait DomainVerifier {
    fn verify(&self, bytes: &[u8], alpha_mode: bool) -> DomainReport;
}
