use d2r_macros::forensic_sensor;

#[forensic_sensor(target = "signature", trigger = "on_desync")]
fn valid1() {}

#[forensic_sensor(target = "signature", trigger = "always", bit_offset = 10, label = "Test")]
fn valid2() {}

fn main() {}
