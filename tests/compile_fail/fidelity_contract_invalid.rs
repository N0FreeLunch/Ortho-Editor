use d2r_macros::fidelity_contract;

#[fidelity_contract(
    metric_version = "item_fidelity_v1",
    format_family = "alpha_v105",
    section = "invalid_alignment",
    preservation = "guessed",
    semantic_coverage = "none",
    owner = "InvalidAlignment",
    required_proof = "targeted_fixture"
)]
struct InvalidAlignment;

fn main() {}
