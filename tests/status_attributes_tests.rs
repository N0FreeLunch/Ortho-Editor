use d2r_core::save::{AttributeSection, gf_payload_range, map_core_sections};
use std::fs;
use std::io;
use std::path::PathBuf;

mod common;
use common::repo_path;

fn load_fixture(path: &str) -> io::Result<Vec<u8>> {
    fs::read(repo_path(path))
}

#[test]
fn test_attribute_parse_progression_values() -> io::Result<()> {
    let bytes =
        load_fixture("tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s")?;
    let map = map_core_sections(&bytes)?;
    let section = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;

    assert_eq!(section.actual_value(12, true), Some(2), "Level should be 2");
    assert_eq!(
        section.actual_value(13, true),
        Some(1170),
        "Experience should be 1170"
    );
    // ID 3: Vitality (base 20 + 5 invested = 25 total). actual_value = 25-32 = -7
    assert_eq!(
        section.actual_value(3, true),
        Some(-7),
        "Vitality check failed"
    );
    assert_eq!(
        section.actual_value(15, true),
        Some(8061),
        "Stash Gold check failed"
    );
    Ok(())
}

#[test]
fn test_attribute_write_roundtrip_empty() -> io::Result<()> {
    let bytes = load_fixture("tests/fixtures/savegames/original/amazon_empty.d2s")?;
    let map = map_core_sections(&bytes)?;
    let section = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;

    let serialized = section.to_bytes(false)?;
    let original = &bytes[map.gf_pos..map.if_pos];
    assert_eq!(serialized, original);
    Ok(())
}

#[test]
fn test_attribute_write_roundtrip_progression() -> io::Result<()> {
    let bytes =
        load_fixture("tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s")?;
    let map = map_core_sections(&bytes)?;
    let section = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;

    let serialized = section.to_bytes(false)?;
    let original = &bytes[map.gf_pos..map.if_pos];
    assert_eq!(serialized, original);
    Ok(())
}
