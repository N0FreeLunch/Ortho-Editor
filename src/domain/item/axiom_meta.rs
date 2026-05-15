/// Represents the level of confidence in a forensic axiom or discovery.
///
/// The levels are ordered from lowest to highest confidence, allowing for
/// range checks and minimum confidence propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]

pub enum Confidence {
    /// Highly speculative or potentially flawed observation.
    Fragile = 0,
    /// An educated guess or initial observation that lacks broad verification.
    Speculative = 1,
    /// A pattern that is emerging across multiple fixtures but not yet fully understood.
    EmergingHypothesis = 2,
    /// A strong pattern with high predictive power and consistent behavior.
    StrongPattern = 3,
    /// A ground truth verified through definitive evidence (e.g., game code or binary symmetry).
    VerifiedTruth = 4,
}

/// Represents the nature of the observed behavior or bitstream artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]

pub enum Intentionality {
    /// A deliberate structural design by the game developers.
    Structural,
    /// An unintended side effect or artifact of the game's implementation (e.g., "garbage" bits).
    Artifactual,
    /// The intentionality has not yet been determined.
    Undetermined,
}

/// Metadata associated with a forensic axiom.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]

pub struct ForensicMetadata {
    /// The confidence level of the axiom.
    pub confidence: Confidence,
    /// The intentionality of the behavior.
    pub intentionality: Intentionality,
    /// A detailed rationale or evidence supporting this axiom's state.
    pub rationale: String,
}

impl ForensicMetadata {
    /// Creates a new forensic metadata instance.
    pub fn new(confidence: Confidence, intentionality: Intentionality, rationale: impl Into<String>) -> Self {
        Self {
            confidence,
            intentionality,
            rationale: rationale.into(),
        }
    }

    /// Aggregates multiple metadata instances into a single summary.
    ///
    /// The resulting confidence is the minimum of all parts (weakest link).
    /// The intentionality is preserved if all parts agree; otherwise, it is marked as Undetermined.
    /// Rationales are joined with semicolons.
    pub fn aggregate(parts: &[ForensicMetadata]) -> Self {
        if parts.is_empty() {
            return Self::new(Confidence::VerifiedTruth, Intentionality::Undetermined, "No parts provided");
        }

        let mut min_confidence = Confidence::VerifiedTruth;
        let mut common_intentionality = Some(parts[0].intentionality);
        let mut rationales = Vec::new();

        for part in parts {
            if part.confidence < min_confidence {
                min_confidence = part.confidence;
            }
            if common_intentionality != Some(part.intentionality) {
                common_intentionality = None;
            }
            rationales.push(part.rationale.as_str());
        }

        Self::new(
            min_confidence,
            common_intentionality.unwrap_or(Intentionality::Undetermined),
            rationales.join("; "),
        )
    }
}

/// A trait for types that act as forensic axioms, providing metadata about their status.
pub trait ForensicAxiom {
    /// Returns the forensic metadata associated with this axiom.
    fn metadata(&self) -> ForensicMetadata;
}

/// A trait for complex axioms composed of multiple child axioms.
pub trait CompositeForensicAxiom: ForensicAxiom {
    /// Returns the metadata of all constituent parts.
    fn parts(&self) -> Vec<ForensicMetadata>;

    /// Derives aggregate metadata from all constituent parts.
    fn derive_metadata(&self) -> ForensicMetadata {
        ForensicMetadata::aggregate(&self.parts())
    }
}

/// The "Seam" pattern allows declarative axioms to delegate complex or
/// version-specific logic to external forensic handlers.
///
/// Following ADR-0012 (Hybrid Management), a Seam provides a flexible
/// extension point within a stable structural core.
///
/// # Example
/// ```
/// pub trait MyAxiomSeam {
///     fn check_plausibility(&self, data: &[u8]) -> bool;
/// }
///
/// pub struct MyAxiom {
///     pub seam: Option<Box<dyn MyAxiomSeam>>,
/// }
/// ```
pub trait ForensicSeam: Send + Sync {
    /// Returns the name of the seam for debugging and reporting.
    fn name(&self) -> &str;
}

/// Helper macro to implement `ForensicAxiom` for a struct.
#[macro_export]
macro_rules! impl_forensic_axiom {
    ($name:ident, $confidence:expr, $intentionality:expr, $rationale:expr) => {
        impl $crate::domain::item::axiom_meta::ForensicAxiom for $name {
            fn metadata(&self) -> $crate::domain::item::axiom_meta::ForensicMetadata {
                $crate::domain::item::axiom_meta::ForensicMetadata::new(
                    $confidence,
                    $intentionality,
                    $rationale,
                )
            }
        }
    };
}

impl Confidence {
    /// Returns a numerical weight for this confidence level (0.0 to 1.0).
    pub fn weight(&self) -> f32 {
        match self {
            Confidence::Fragile => 0.0,
            Confidence::Speculative => 0.1,
            Confidence::EmergingHypothesis => 0.4,
            Confidence::StrongPattern => 0.8,
            Confidence::VerifiedTruth => 1.0,
        }
    }
}

/// A numerical representation of the reliability of a parsed object.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct FidelityScore {
    /// The score value, ranging from 0.0 (completely unreliable) to 1.0 (verified truth).
    pub value: f32,
}

impl FidelityScore {
    /// Calculates a fidelity score from a forensic audit.
    ///
    /// The score is determined by the minimum confidence encountered (the weakest link)
    /// and the average confidence of all findings.
    pub fn from_audit(audit: &ForensicAudit) -> Self {
        if audit.findings.is_empty() {
            return Self { value: 1.0 };
        }

        let min_weight = audit.combined_confidence.weight();
        let total_weight: f32 = audit.findings.iter().map(|f| f.confidence.weight()).sum();
        let avg_weight = total_weight / audit.findings.len() as f32;

        // The final score is the minimum of the weakest link and the average.
        // This ensures that one "Fragile" part drags the whole score down.
        Self {
            value: min_weight.min(avg_weight),
        }
    }
}

/// Cumulative record of forensic findings during a pipeline execution.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]

pub struct ForensicAudit {
    /// The combined confidence level (minimum of all recorded findings).
    pub combined_confidence: Confidence,
    /// Detailed list of all metadata recorded during the process.
    pub findings: Vec<ForensicMetadata>,
}

impl Default for ForensicAudit {
    fn default() -> Self {
        Self::new()
    }
}

impl ForensicAudit {
    /// Creates a new, empty forensic audit with maximum confidence.
    pub fn new() -> Self {
        Self {
            combined_confidence: Confidence::VerifiedTruth,
            findings: Vec::new(),
        }
    }

    /// Records a new piece of forensic metadata and updates the combined confidence.
    pub fn record(&mut self, meta: ForensicMetadata) {
        if meta.confidence < self.combined_confidence {
            self.combined_confidence = meta.confidence;
        }
        self.findings.push(meta);
    }

    /// Extends this audit with findings from another audit.
    pub fn extend(&mut self, other: ForensicAudit) {
        if other.combined_confidence < self.combined_confidence {
            self.combined_confidence = other.combined_confidence;
        }
        self.findings.extend(other.findings);
    }

    /// Generates a human-readable audit report.

    pub fn report(&self) -> String {
        let score = FidelityScore::from_audit(self);
        let mut report = String::new();
        
        report.push_str("[FORENSIC AUDIT REPORT]\n");
        report.push_str("-----------------------\n");
        report.push_str(&format!("Fidelity Score: {:.1}%\n", score.value * 100.0));
        report.push_str(&format!("Overall Status: {:?}\n\n", self.combined_confidence));
        
        report.push_str(&format!("Findings ({}):\n", self.findings.len()));
        for finding in &self.findings {
            report.push_str(&format!(
                "- [{:?}] [{:?}] {}\n",
                finding.confidence,
                finding.intentionality,
                finding.rationale
            ));
        }
        report.push_str("-----------------------");
        
        report
    }
}

/// A wrapper that carries a result along with its forensic audit trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicResult<T> {
    /// The actual data or an error message.
    pub value: Result<T, String>,
    /// The cumulative forensic evidence accumulated so far.
    pub audit: ForensicAudit,
}

impl<T> ForensicResult<T> {
    /// Creates a successful forensic result with the given data and initial metadata.
    pub fn ok(data: T, meta: ForensicMetadata) -> Self {
        let mut audit = ForensicAudit::new();
        audit.record(meta);
        Self {
            value: Ok(data),
            audit,
        }
    }

    /// Creates a failed forensic result with the given error and initial metadata.
    pub fn err(error: impl Into<String>, meta: ForensicMetadata) -> Self {
        let mut audit = ForensicAudit::new();
        audit.record(meta);
        Self {
            value: Err(error.into()),
            audit,
        }
    }

    /// Transforms the inner value if it is `Ok`, recording new forensic metadata.
    pub fn map_forensic<U, F>(self, f: F) -> ForensicResult<U>
    where
        F: FnOnce(T) -> (U, ForensicMetadata),
    {
        match self.value {
            Ok(data) => {
                let (new_data, new_meta) = f(data);
                let mut new_audit = self.audit;
                new_audit.record(new_meta);
                ForensicResult {
                    value: Ok(new_data),
                    audit: new_audit,
                }
            }
            Err(e) => ForensicResult {
                value: Err(e),
                audit: self.audit,
            },
        }
    }

    /// Chains another forensic operation if the current result is `Ok`.
    pub fn and_then_forensic<U, F>(self, f: F) -> ForensicResult<U>
    where
        F: FnOnce(T) -> ForensicResult<U>,
    {
        match self.value {
            Ok(data) => {
                let next_result = f(data);
                let mut combined_audit = self.audit;
                
                // Merge findings from the next result into our audit
                for finding in next_result.audit.findings {
                    combined_audit.record(finding);
                }
                
                ForensicResult {
                    value: next_result.value,
                    audit: combined_audit,
                }
            }
            Err(e) => ForensicResult {
                value: Err(e),
                audit: self.audit,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_ordering() {
        assert!(Confidence::Fragile < Confidence::Speculative);
        assert!(Confidence::Speculative < Confidence::EmergingHypothesis);
        assert!(Confidence::EmergingHypothesis < Confidence::StrongPattern);
        assert!(Confidence::StrongPattern < Confidence::VerifiedTruth);
        
        assert!(Confidence::VerifiedTruth >= Confidence::StrongPattern);
        assert!(Confidence::Fragile <= Confidence::Fragile);
    }

    #[test]
    fn test_metadata_creation() {
        let rationale = "Observed across 50+ items in version 1.14d";
        let meta = ForensicMetadata::new(
            Confidence::StrongPattern,
            Intentionality::Structural,
            rationale,
        );

        assert_eq!(meta.confidence, Confidence::StrongPattern);
        assert_eq!(meta.intentionality, Intentionality::Structural);
        assert_eq!(meta.rationale, rationale);
    }

    #[test]
    fn test_confidence_min_max() {
        use std::cmp;
        
        let c1 = Confidence::Fragile;
        let c2 = Confidence::VerifiedTruth;
        
        assert_eq!(cmp::min(c1, c2), Confidence::Fragile);
        assert_eq!(cmp::max(c1, c2), Confidence::VerifiedTruth);
    }

    #[test]
    fn test_forensic_axiom_trait_and_macro() {
        struct DummyAxiom;

        impl_forensic_axiom!(
            DummyAxiom,
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Built-in language feature"
        );

        let axiom = DummyAxiom;
        let meta = axiom.metadata();

        assert_eq!(meta.confidence, Confidence::VerifiedTruth);
        assert_eq!(meta.intentionality, Intentionality::Structural);
        assert_eq!(meta.rationale, "Built-in language feature");
    }

    #[test]
    fn test_confidence_aggregation() {
        let p1 = ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "P1 ok");
        let p2 = ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "P2 weak");
        let p3 = ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "P3 strong");

        let agg = ForensicMetadata::aggregate(&[p1, p2, p3]);

        assert_eq!(agg.confidence, Confidence::Fragile); // Weakest link
        assert_eq!(agg.intentionality, Intentionality::Undetermined); // Mixed intentionality
        assert!(agg.rationale.contains("P1 ok"));
        assert!(agg.rationale.contains("P2 weak"));
    }

    #[test]
    fn test_composite_forensic_axiom() {
        struct MyComposite;

        impl ForensicAxiom for MyComposite {
            fn metadata(&self) -> ForensicMetadata {
                self.derive_metadata()
            }
        }

        impl CompositeForensicAxiom for MyComposite {
            fn parts(&self) -> Vec<ForensicMetadata> {
                vec![
                    ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "Part A"),
                    ForensicMetadata::new(Confidence::Speculative, Intentionality::Structural, "Part B"),
                ]
            }
        }

        let composite = MyComposite;
        let meta = composite.metadata();

        assert_eq!(meta.confidence, Confidence::Speculative);
        assert_eq!(meta.intentionality, Intentionality::Structural);
        assert_eq!(meta.rationale, "Part A; Part B");
    }

    #[test]
    fn test_forensic_audit_confidence_downgrade() {
        let mut audit = ForensicAudit::new();
        assert_eq!(audit.combined_confidence, Confidence::VerifiedTruth);

        audit.record(ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "Ok"));
        assert_eq!(audit.combined_confidence, Confidence::StrongPattern);

        audit.record(ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "Suspicious"));
        assert_eq!(audit.combined_confidence, Confidence::Fragile);

        audit.record(ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "Perfect"));
        assert_eq!(audit.combined_confidence, Confidence::Fragile); // Stays at minimum
    }

    #[test]
    fn test_forensic_result_pipeline_chaining() {
        let initial_meta = ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "Start");
        let result = ForensicResult::ok(10, initial_meta);

        let final_result = result
            .map_forensic(|val| {
                (val * 2, ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "Step 1"))
            })
            .and_then_forensic(|val| {
                let meta = ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "Step 2");
                if val > 15 {
                    ForensicResult::ok(val + 5, meta)
                } else {
                    ForensicResult::err("Too small", meta)
                }
            });

        assert_eq!(final_result.value, Ok(25));
        assert_eq!(final_result.audit.combined_confidence, Confidence::Fragile);
        assert_eq!(final_result.audit.findings.len(), 3);
        assert_eq!(final_result.audit.findings[0].rationale, "Start");
        assert_eq!(final_result.audit.findings[1].rationale, "Step 1");
        assert_eq!(final_result.audit.findings[2].rationale, "Step 2");
    }

    #[test]
    fn test_forensic_result_error_propagation() {
        let meta1 = ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "P1");
        let meta2 = ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "P2");
        
        let res: ForensicResult<i32> = ForensicResult::err("Initial Error", meta1);
        let final_res = res.map_forensic(|val| (val + 1, meta2));

        assert_eq!(final_res.value, Err("Initial Error".to_string()));
        assert_eq!(final_res.audit.combined_confidence, Confidence::VerifiedTruth);
        assert_eq!(final_res.audit.findings.len(), 1);
    }

    #[test]
    fn test_fidelity_score_calculation() {
        // Case 1: Perfect audit
        let mut audit = ForensicAudit::new();
        audit.record(ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "Ok"));
        assert_eq!(FidelityScore::from_audit(&audit).value, 1.0);

        // Case 2: One Fragile finding
        audit.record(ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "Bad"));
        assert_eq!(FidelityScore::from_audit(&audit).value, 0.0);

        // Case 3: Mixed findings
        let mut audit2 = ForensicAudit::new();
        audit2.record(ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "A"));
        audit2.record(ForensicMetadata::new(Confidence::VerifiedTruth, Intentionality::Structural, "B"));
        // Combined min is StrongPattern (0.8). Average is (0.8 + 1.0) / 2 = 0.9.
        // Min(0.8, 0.9) = 0.8.
        assert_eq!(FidelityScore::from_audit(&audit2).value, 0.8);
    }

    #[test]
    fn test_forensic_audit_report() {
        let mut audit = ForensicAudit::new();
        audit.record(ForensicMetadata::new(Confidence::StrongPattern, Intentionality::Structural, "Logic A confirmed"));
        audit.record(ForensicMetadata::new(Confidence::Fragile, Intentionality::Artifactual, "Bit pattern B suspicious"));
        
        let report = audit.report();
        
        assert!(report.contains("[FORENSIC AUDIT REPORT]"));
        assert!(report.contains("Fidelity Score: 0.0%")); // Because of Fragile
        assert!(report.contains("Overall Status: Fragile"));
        assert!(report.contains("Findings (2):"));
        assert!(report.contains("- [StrongPattern] [Structural] Logic A confirmed"));
        assert!(report.contains("- [Fragile] [Artifactual] Bit pattern B suspicious"));
    }
}
