use std::fs;
use std::path::PathBuf;

use d2r_core::engine::checksum::{Checksum, finalize_save_bytes};
use d2r_core::save::{D2S_MAGIC, Save, find_jm_markers, recalculate_checksum};

mod common;
use common::repo_path;

fn load_fixture(relative: &str) -> Vec<u8> {
    fs::read(repo_path(relative)).expect("fixture should be readable")
}

#[test]
fn save_header_parses_amazon_empty_header() {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    let save = Save::from_bytes(&bytes).expect("header parsing should succeed");

    assert_eq!(save.header.magic, D2S_MAGIC);
    assert_eq!(save.header.version, 105);
    assert_eq!(save.header.file_size as usize, bytes.len());
    assert_eq!(save.header.char_name, "TESTAMAZON");
    assert_eq!(save.header.char_class, 0);
    assert_eq!(save.header.char_level, 1);
    assert_eq!(save.header.active_weapon, 0);
}

#[test]
fn save_header_parses_all_original_amazon_headers() {
    let fixture_dir = repo_path("tests/fixtures/savegames/original");

    for entry in fs::read_dir(fixture_dir).expect("fixture directory should exist") {
        let entry = entry.expect("fixture entry should be readable");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("d2s") {
            continue;
        }

        let bytes = fs::read(&path).expect("fixture should be readable");
        let save = Save::from_bytes(&bytes).expect("fixture header should parse");

        assert_eq!(save.header.magic, D2S_MAGIC, "{}", path.display());
        assert_eq!(save.header.version, 105, "{}", path.display());
        // Skip char_level and char_name checks for "all" fixtures as they vary
    }
}

#[test]
fn save_header_finds_expected_jm_markers_in_empty_save() {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    assert_eq!(find_jm_markers(&bytes), vec![903, 947]);
}

#[test]
fn save_header_recalculates_checksum_for_empty_save() {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    let stored = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let recalculated = recalculate_checksum(&bytes).expect("checksum should recalculate");
    assert_eq!(stored, recalculated);
}

#[test]
fn checksum_operations_preserve_fixture_bytes() {
    let original = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s");
    let mut bytes = original.clone();
    Checksum::fix(&mut bytes);
    assert_eq!(bytes, original);

    // Corrupt only metadata; finalization must reproduce the original save.
    bytes[8..16].fill(0xff);
    let before_recalculation = bytes.clone();
    recalculate_checksum(&bytes).expect("corrupt metadata can still be checksummed");
    assert_eq!(bytes, before_recalculation);

    finalize_save_bytes(&mut bytes, false).expect("finalization should repair metadata");
    assert_eq!(bytes, original);
    finalize_save_bytes(&mut bytes, true).expect("repeated finalization should be stable");
    assert_eq!(bytes, original);
}

#[test]
fn checksum_operations_preserve_short_input_contracts() {
    for len in 0..16 {
        let original = vec![0xa5; len];
        let mut bytes = original.clone();
        Checksum::fix(&mut bytes);
        assert_eq!(bytes, original, "fix must be a no-op for length {len}");

        let error = recalculate_checksum(&bytes).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "Save file is too small for checksum recalculation."
        );
        for force in [false, true] {
            let error = finalize_save_bytes(&mut bytes, force).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert_eq!(
                error.to_string(),
                "Save bytes must be at least 16 bytes to finalize."
            );
            assert_eq!(bytes, original);
        }
    }
}
