//! Validation contracts shared by property grading and policy orchestration.

use crate::item::ItemBitRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WarningConfidenceTier {
    /// 1. Hard Invariant: Absolute physical integrity violation (e.g., bit desync, ghost items)
    HardInvariant,
    /// 2. Settled Contract: Violation against fixed game templates (Unique, Set, Runeword specs)
    SettledContract,
    /// 3. Evolving Hypothesis: Observation divergence against evolving heuristic models (e.g. Rare defense rolling, alvl)
    EvolvingHypothesis,
    /// 4. Advisory: Pure informational advisory
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValidationWarning {
    pub message: String,
    pub tier: WarningConfidenceTier,
    pub category: &'static str,
}

impl ValidationWarning {
    pub fn new(
        message: impl Into<String>,
        tier: WarningConfidenceTier,
        category: &'static str,
    ) -> Self {
        Self {
            message: message.into(),
            tier,
            category,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub spec_name: String,
    pub is_perfect: bool,
    pub score: f32, // Overall perfection score
    pub stats: Vec<StatValidation>,
    pub warnings: Vec<String>,
    pub detailed_warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    // Project every legacy message before appending the detailed batch.
    pub(super) fn extend_detailed_warnings(&mut self, warnings: Vec<ValidationWarning>) {
        for w in &warnings {
            self.warnings.push(w.message.clone());
        }
        self.detailed_warnings.extend(warnings);
    }
}

#[derive(Debug, Clone)]
pub struct StatValidation {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub current: i32,
    pub min: i32,
    pub max: i32,
    pub is_perfect: bool,
    pub score: f32,
    pub status: StatValidationStatus,
    pub range: ItemBitRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatValidationStatus {
    InRange,
    OutOfRange,
    MissingOnItem,
    UnexpectedOnItem,
}
