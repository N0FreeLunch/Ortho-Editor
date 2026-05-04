/// Represents the level of confidence in a forensic axiom or discovery.
///
/// The levels are ordered from lowest to highest confidence, allowing for
/// range checks and minimum confidence propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intentionality {
    /// A deliberate structural design by the game developers.
    Structural,
    /// An unintended side effect or artifact of the game's implementation (e.g., "garbage" bits).
    Artifactual,
    /// The intentionality has not yet been determined.
    Undetermined,
}

/// Metadata associated with a forensic axiom.
#[derive(Debug, Clone, PartialEq, Eq)]
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
}
