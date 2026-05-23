use d2r_macros::serialization_symmetry;

// Violation 1: align = false with checksum set (forbidden parameter combo)
#[serialization_symmetry(align = false, checksum = "Xor8", seed = 0x87)]
struct TestStructInvalid1 {
    field: u32,
}

// Violation 2: seed is set but checksum is none (forbidden parameter combo)
#[serialization_symmetry(align = true, seed = 0x87)]
struct TestStructInvalid2 {
    field: u32,
}

fn main() {}
