use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::forensic::v105::axioms::V105PropertyNudgeAxiom;
use d2r_core::domain::item::axiom_meta::ForensicAudit;
use d2r_core::domain::item::subdomains::nudge::NudgeCombinator;
use d2r_macros::{hypothesis_test, invariant_test};
use std::io::Cursor;

// Freeze the pre-migration policy independently of the production data table.
// This characterizes existing behavior, not universal game-format authority.
fn legacy_residue(version: u8) -> u8 {
    [3, 0, 3, 1, 1, 5]
        .get(version as usize)
        .copied()
        .unwrap_or(0)
}

#[hypothesis_test]
fn all_header_versions_preserve_legacy_residue_policy() {
    for version in u8::MIN..=u8::MAX {
        assert_eq!(
            V105PropertyNudgeAxiom.get_nudge(version),
            legacy_residue(version),
            "residue policy changed for header version {version}"
        );
    }
}

#[invariant_test]
fn residue_cursor_alignment_and_admission_are_preserved() {
    let payload = [0x96, 0x69];
    for version in u8::MIN..=u8::MAX {
        for compact in [false, true] {
            for recovery in [false, true] {
                for runeword in [false, true] {
                    let reader = BitReader::endian(Cursor::new(&payload[..]), LittleEndian);
                    let mut cursor = BitCursor::new(reader);
                    cursor.read_bits::<u8>(2).unwrap();
                    let mut audit = ForensicAudit::default();
                    NudgeCombinator
                        .apply_property_residue_nudge(
                            &mut cursor,
                            version,
                            recovery,
                            compact,
                            runeword,
                            &mut audit,
                        )
                        .unwrap();
                    let consumed = if compact || recovery || runeword {
                        0
                    } else {
                        legacy_residue(version) as u64
                    };
                    assert_eq!(cursor.pos(), 2 + consumed);
                    assert_eq!(audit.findings.len(), usize::from(consumed > 0));
                    let expected = ((u16::from_le_bytes(payload) >> (2 + consumed)) & 7) as u8;
                    assert_eq!(cursor.read_bits::<u8>(3).unwrap(), expected);
                }
            }
        }
    }
}

#[invariant_test]
fn incomplete_residue_keeps_position_bits_and_audit_unchanged() {
    let payload = [0x96, 0x69];
    for version in [0, 2, 3, 4, 5] {
        let reader = BitReader::endian(Cursor::new(&payload[..]), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.read_bits::<u8>(2).unwrap();
        cursor.set_limit(2 + u64::from(legacy_residue(version)) - 1);
        let mut audit = ForensicAudit::default();
        NudgeCombinator
            .apply_property_residue_nudge(&mut cursor, version, false, false, false, &mut audit)
            .unwrap();
        assert_eq!(cursor.pos(), 2);
        assert!(audit.findings.is_empty());
        cursor.set_limit(16);
        assert_eq!(
            cursor.read_bits::<u8>(3).unwrap(),
            ((u16::from_le_bytes(payload) >> 2) & 7) as u8
        );
    }
}
