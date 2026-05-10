use d2r_core::save::{map_core_sections, parse_skill_section, patch_skill_section, SkillSection};
use std::fs;
use std::io;

mod common;
use common::repo_path;

fn load_fixture(path: &str) -> io::Result<Vec<u8>> {
    fs::read(repo_path(path))
}

#[test]
fn status_skills_parse_patch_roundtrip() -> io::Result<()> {
    let fixtures = [
        "tests/fixtures/savegames/original/amazon_empty.d2s",
        "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
        "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
        "tests/fixtures/savegames/original/amazon_v105_andariel_killed_no_talk.d2s",
        "tests/fixtures/savegames/original/amazon_v105_re_probe_zigzag_all_diff.d2s",
        "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
        "tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
        "tests/fixtures/savegames/original/amazon_initial.d2s",
        "tests/fixtures/savegames/original/amazon_moved_diff_basis.d2s",
        "tests/fixtures/savegames/original/amazon_moved_manual.d2s",
    ];

    for fixture_path in fixtures {
        let bytes = load_fixture(fixture_path)?;
        let map = map_core_sections(&bytes)?;
        let skills = parse_skill_section(&bytes, &map)?;
        
        let patched = patch_skill_section(&bytes, &map, &skills)?;
        assert_eq!(patched, bytes, "Roundtrip failed for fixture: {}", fixture_path);
    }

    // Specific check for known value in progression fixture
    let prog_bytes = load_fixture("tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s")?;
    let map_prog = map_core_sections(&prog_bytes)?;
    let skills_prog = parse_skill_section(&prog_bytes, &map_prog)?;
    assert_eq!(skills_prog.as_slice()[3], 1);

    Ok(())
}

#[test]
fn skill_section_semantic_access() {
    let mut skills = SkillSection([0u8; 30]);

    // Amazon example (base_id: 6)
    // Skill ID 9 (Inner Sight) should be at index 3
    skills.set_level(6, 9, 5);
    assert_eq!(skills.as_slice()[3], 5);
    assert_eq!(skills.get_level(6, 9), 5);

    // Druid example (base_id: 221)
    // Skill ID 221 (Raven) should be at index 0
    skills.set_level(221, 221, 20);
    assert_eq!(skills.as_slice()[0], 20);
    assert_eq!(skills.get_level(221, 221), 20);

    // Underflow guard (skill_id < base_id)
    assert_eq!(skills.get_level(6, 5), 0);
    skills.set_level(6, 5, 10);
    // Should not have mutated any existing values (index 0 is Raven=20)
    assert_eq!(skills.as_slice()[0], 20);

    // Overflow guard (index >= 30)
    assert_eq!(skills.get_level(6, 36), 0);
    skills.set_level(6, 36, 10);
    // Should not panic or mutate
}

#[test]
fn skill_section_class_bridge() {
    use d2r_core::save::{class_skill_base_id, get_skill_level_by_class};
    
    // Test mapping
    assert_eq!(class_skill_base_id(0), Some(6));   // Amazon
    assert_eq!(class_skill_base_id(5), Some(221)); // Druid
    assert_eq!(class_skill_base_id(7), Some(373)); // Warlock

    let mut skills = SkillSection([0u8; 30]);
    // Skill ID 9 (Inner Sight) for Amazon (class_id 0)
    skills.set_level(6, 9, 7);
    assert_eq!(get_skill_level_by_class(&skills, 0, 9), 7);

    // Skill ID 221 (Raven) for Druid (class_id 5)
    skills.set_level(221, 221, 12);
    assert_eq!(get_skill_level_by_class(&skills, 5, 221), 12);

    // Unknown class
    assert_eq!(get_skill_level_by_class(&skills, 7, 9), 0);
}

#[test]
fn skill_section_iterator() {
    let mut skills = SkillSection([0u8; 30]);
    skills.set_level(6, 6, 1);  // Index 0
    skills.set_level(6, 10, 5); // Index 4
    skills.set_level(6, 35, 3); // Index 29

    let all_skills: Vec<_> = skills.iter_skills(6).collect();
    assert_eq!(all_skills.len(), 30);
    assert_eq!(all_skills[0].skill_id, 6);
    assert_eq!(all_skills[0].level, 1);
    assert_eq!(all_skills[4].skill_id, 10);
    assert_eq!(all_skills[4].level, 5);
    assert_eq!(all_skills[29].skill_id, 35);
    assert_eq!(all_skills[29].level, 3);
}
