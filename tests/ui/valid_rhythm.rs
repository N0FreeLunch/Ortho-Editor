use d2r_macros::rhythm_alignment;

#[rhythm_alignment(width = 80, gap = "alpha_v0", versions = [0, 1, 5])]
struct ValidEquipmentSlot {
    pub flags: u32,
}

fn main() {
    // Associated constants & helper methods check
    assert_eq!(ValidEquipmentSlot::RHYTHM_WIDTH, 80);
    assert_eq!(ValidEquipmentSlot::slot_width(), 80);
    assert_eq!(ValidEquipmentSlot::RHYTHM_GAP, Some("alpha_v0"));
    assert_eq!(ValidEquipmentSlot::alignment_gap(), Some("alpha_v0"));
    assert_eq!(ValidEquipmentSlot::RHYTHM_VERSIONS, &[0, 1, 5]);
    assert_eq!(ValidEquipmentSlot::supported_versions(), &[0, 1, 5]);
}
