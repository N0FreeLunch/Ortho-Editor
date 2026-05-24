#[test]
fn rhythm_alignment_ui_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/invalid_rhythm.rs");
    t.pass("tests/ui/valid_rhythm.rs");
}
