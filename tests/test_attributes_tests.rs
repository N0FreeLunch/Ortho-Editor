use d2r_macros::{invariant_test, hypothesis_test, radar_test};

#[invariant_test]
fn test_invariant_attribute_execution() {
    let bit_perfect = true;
    assert!(bit_perfect, "Hard invariant must hold");
}

#[hypothesis_test]
fn test_hypothesis_attribute_execution() {
    let observed_count = 34;
    assert_eq!(observed_count, 34, "Hypothesis snapshot observation");
}

#[radar_test]
fn test_radar_attribute_execution() {
    // This test should be ignored by default by cargo test
    panic!("Radar test should be ignored in default cargo test run!");
}
