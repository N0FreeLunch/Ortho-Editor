use d2r_core::save::Save;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let fixture_path = Path::new(repo_root)
        .join("tests/fixtures/savegames/original/amazon_v105_re_probe_zigzag_all_diff.d2s");

    println!("--- Alpha v105 Quest Semantic Verification ---");
    println!("Loading fixture: {:?}", fixture_path);

    let bytes = fs::read(fixture_path)?;
    let save = Save::from_bytes(&bytes)?;

    if let Some(quests) = save.header.quests {
        println!("\n[Normal Difficulty (Reality: 1,2,3,5,6 ON)]");
        let normal_q1 = "Den of Evil";
        let is_q1_done = quests.is_v105_completed_by_name(normal_q1, 0, 0x90);
        println!("{}: {}", normal_q1, is_q1_done);
        assert!(is_q1_done, "Normal Q1 must be completed");

        let normal_q2 = "Sisters' Burial Grounds";
        let is_q2_done = quests.is_v105_completed_by_name(normal_q2, 0, 0x90);
        println!("{}: {}", normal_q2, is_q2_done);
        assert!(is_q2_done, "Normal Q2 must be completed (Reality Check)");

        let normal_q3 = "The Search for Cain";
        let is_q3_done = quests.is_v105_completed_by_name(normal_q3, 0, 0x90);
        println!("{}: {}", normal_q3, is_q3_done);
        assert!(is_q3_done, "Normal Q3 must be completed");

        let normal_q4 = "The Forgotten Tower";
        let is_q4_done = quests.is_v105_completed_by_name(normal_q4, 0, 0x90);
        println!("{}: {}", normal_q4, is_q4_done);
        assert!(
            !is_q4_done,
            "Normal Q4 must NOT be completed (Skipped in probe)"
        );

        println!("\n[Nightmare Difficulty (Expected: Even ON)]");
        let nm_q1 = "NM Den of Evil";
        let is_nm_q1_done = quests.is_v105_completed_by_name(nm_q1, 0, 0x90);
        println!("{}: {}", nm_q1, is_nm_q1_done);
        assert!(
            !is_nm_q1_done,
            "NM Q1 must NOT be completed (Even ON pattern)"
        );

        let nm_q2 = "NM Sisters' Burial Grounds";
        let is_nm_q2_done = quests.is_v105_completed_by_name(nm_q2, 0, 0x90);
        println!("{}: {}", nm_q2, is_nm_q2_done);
        assert!(is_nm_q2_done, "NM Q2 must be completed (Even ON pattern)");

        println!("\n[Hell Difficulty (Expected: 5th ON)]");
        let hell_q5 = "Hell The Tools of the Trade"; // Act 1 Q5
        let is_hell_q5_done = quests.is_v105_completed_by_name(hell_q5, 0, 0x90);
        println!("{}: {}", hell_q5, is_hell_q5_done);
        assert!(
            is_hell_q5_done,
            "Hell Q5 must be completed (5th ON pattern)"
        );

        println!("\n✅ All semantic quest mappings verified against Alpha v105 Oracle!");
    } else {
        panic!("Quest section missing or version mismatch!");
    }

    Ok(())
}
