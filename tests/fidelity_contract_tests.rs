#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/fidelity_contract_invalid.rs");
}

use d2r_core::domain::item::entity::item_fidelity_contracts;
use d2r_macros::fidelity_contract;

#[fidelity_contract(
    metric_version = "item_fidelity_v1",
    format_family = "alpha_v105",
    section = "test_alignment",
    preservation = "exact",
    semantic_coverage = "none",
    semantic_disposition = "not_applicable",
    evidence_level = "none",
    owner = "TestAlignment",
    required_proof = "targeted_fixture"
)]
struct TestAlignment;

#[test]
fn generated_constants_are_consumed_by_the_explicit_registry() {
    assert_eq!(TestAlignment::FIDELITY_METRIC_VERSION, "item_fidelity_v1");
    assert_eq!(TestAlignment::FIDELITY_PRESERVATION, "exact");
    assert_eq!(
        TestAlignment::FIDELITY_SEMANTIC_DISPOSITION,
        "not_applicable"
    );

    let contracts = item_fidelity_contracts();
    assert_eq!(contracts.len(), 2);
    assert_eq!(contracts[0].owner, "AlphaV105HeaderGapAlignment");
    assert_eq!(contracts[0].required_proof, "targeted_fixture");
    assert_eq!(contracts[1].section, "ae7_7pw_identity");
    assert_eq!(contracts[1].semantic_disposition, "deferred");
    assert_eq!(contracts[1].evidence_level, "hypothesis");
    assert_eq!(contracts[1].required_proof, "authority_fixture");
}
