use d2r_macros::forensic_sensor;

#[forensic_sensor(target = "signature", trigger = "sometimes")]
fn test_func() {
}

fn main() {}
