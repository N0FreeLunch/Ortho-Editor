#[test]
fn forensic_sensor_ui_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/invalid_trigger.rs");
    t.compile_fail("tests/ui/missing_target.rs");
    t.pass("tests/ui/valid_sensor.rs");
}
