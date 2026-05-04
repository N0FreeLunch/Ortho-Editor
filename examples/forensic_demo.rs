use d2r_core::impl_forensic_axiom;
use d2r_core::domain::item::axiom_meta::{
    Confidence, Intentionality, ForensicMetadata, ForensicAxiom, 
    CompositeForensicAxiom, ForensicAudit, ForensicResult, FidelityScore,
};

// 1. Define some dummy axioms using the macro
struct BitDriftAxiom;
impl_forensic_axiom!(BitDriftAxiom, Confidence::Speculative, Intentionality::Artifactual, "Observed 1-bit drift in Larzuk v105 fixtures.");

struct HeaderMarkerAxiom;
impl_forensic_axiom!(HeaderMarkerAxiom, Confidence::VerifiedTruth, Intentionality::Structural, "JM marker is the canonical item start.");

// 2. Define a composite axiom
struct ItemAxiomSet {
    drift: BitDriftAxiom,
    marker: HeaderMarkerAxiom,
}

impl ForensicAxiom for ItemAxiomSet {
    fn metadata(&self) -> ForensicMetadata {
        self.derive_metadata() 
    }
}

impl CompositeForensicAxiom for ItemAxiomSet {
    fn parts(&self) -> Vec<ForensicMetadata> {
        vec![self.drift.metadata(), self.marker.metadata()]
    }
}

fn main() {
    println!("=== D2R Forensic Metadata Infrastructure Demo ===\n");

    // 3. Demonstrate Metadata & Trait
    let drift = BitDriftAxiom;
    let meta = drift.metadata();
    println!("Axiom: BitDrift");
    println!("  Confidence: {:?}", meta.confidence);
    println!("  Rationale:  {}\n", meta.rationale);

    // 4. Demonstrate Pipeline (Railway Pattern)
    println!("Executing Forensic Pipeline...");
    let pipeline = ForensicResult::ok(100, HeaderMarkerAxiom.metadata())
        .map_forensic(|val| {
            // Successive stage adds more metadata
            (val + 50, BitDriftAxiom.metadata())
        });

    println!("  Final Value: {:?}", pipeline.value);
    println!("  Combined Confidence: {:?}\n", pipeline.audit.combined_confidence);

    // 5. Demonstrate Composite Aggregation
    let suite = ItemAxiomSet { drift: BitDriftAxiom, marker: HeaderMarkerAxiom };
    let suite_meta = suite.metadata();
    println!("Composite Suite:");
    println!("  Aggregated Confidence: {:?} (Expected: Speculative)", suite_meta.confidence);
    println!("  Aggregated Rationale:   {}\n", suite_meta.rationale);

    // 6. Demonstrate Fidelity Scoring & Reporting
    let mut audit = ForensicAudit::new();
    audit.record(HeaderMarkerAxiom.metadata());
    audit.record(BitDriftAxiom.metadata());
    
    let score = FidelityScore::from_audit(&audit);
    println!("{}", audit.report());
    println!("Final Fidelity Score: {:.2}%", score.value * 100.0);
}
