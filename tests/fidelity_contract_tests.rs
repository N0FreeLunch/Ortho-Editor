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
    owner = "TestAlignment",
    required_proof = "targeted_fixture"
)]
struct TestAlignment;

#[test]
fn generated_constants_are_consumed_by_the_explicit_registry() {
    assert_eq!(TestAlignment::FIDELITY_METRIC_VERSION, "item_fidelity_v1");
    assert_eq!(TestAlignment::FIDELITY_PRESERVATION, "exact");

    let contracts = item_fidelity_contracts();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].owner, "AlphaV105HeaderGapAlignment");
    assert_eq!(contracts[0].required_proof, "targeted_fixture");
}
