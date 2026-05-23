#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/serialization_symmetry_invalid.rs");
}

use d2r_macros::serialization_symmetry;

#[serialization_symmetry(align = true, checksum = "Xor8", seed = 0x87)]
pub struct AlphaV105HeaderPayload {
    pub flags: u32,
    pub level: u16,
}

#[serialization_symmetry(align = false)]
pub struct RawBitwiseBlock {
    pub raw_data: Vec<u8>,
}

#[serialization_symmetry(align = true, tentative = true, confidence = "Low")]
pub struct AlphaV105BodyTentativeSeam {
    pub raw_payload: Vec<u8>,
}

#[test]
fn test_symmetry_codegen() {
    assert_eq!(AlphaV105HeaderPayload::SER_ALIGN, true);
    assert_eq!(AlphaV105HeaderPayload::align_required(), true);
    assert_eq!(AlphaV105HeaderPayload::checksum_algorithm(), Some("Xor8"));
    assert_eq!(AlphaV105HeaderPayload::checksum_seed(), Some(0x87));
    assert_eq!(AlphaV105HeaderPayload::is_tentative(), false);
    assert_eq!(AlphaV105HeaderPayload::confidence_level(), None);

    assert_eq!(RawBitwiseBlock::SER_ALIGN, false);
    assert_eq!(RawBitwiseBlock::align_required(), false);
    assert_eq!(RawBitwiseBlock::checksum_algorithm(), None);
    assert_eq!(RawBitwiseBlock::checksum_seed(), None);
    assert_eq!(RawBitwiseBlock::is_tentative(), false);
    assert_eq!(RawBitwiseBlock::confidence_level(), None);

    assert_eq!(AlphaV105BodyTentativeSeam::SER_ALIGN, true);
    assert_eq!(AlphaV105BodyTentativeSeam::align_required(), true);
    assert_eq!(AlphaV105BodyTentativeSeam::checksum_algorithm(), None);
    assert_eq!(AlphaV105BodyTentativeSeam::checksum_seed(), None);
    assert_eq!(AlphaV105BodyTentativeSeam::is_tentative(), true);
    assert_eq!(AlphaV105BodyTentativeSeam::confidence_level(), Some("Low"));
}
