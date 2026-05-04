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
}

/// A trait for types that act as forensic axioms, providing metadata about their status.
pub trait ForensicAxiom {
    /// Returns the forensic metadata associated with this axiom.
    fn metadata(&self) -> ForensicMetadata;
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
}
