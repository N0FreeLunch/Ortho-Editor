use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load Root .env (from the d2r-spec workspace root)
    // In our structure, we are in d2r-core/src/bin, so root is ../../
    dotenvy::from_path("../.env").ok();
    println!("[v105_lab] Root .env loaded.");

    // 2. Build and Run Extractor in d2r-data/extractor
    println!("[v105_lab] Step 1: Running Extractor...");
    let mut extractor = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--bin",
            "d2r-data-extractor",
            "--",
            "extract",
        ])
        .current_dir("../d2r-data/extractor")
        .spawn()?;

    let status = extractor.wait()?;
    if !status.success() {
        return Err("Extractor failed to run".into());
    }
    println!("[v105_lab] Step 1: Data extraction complete.");

    // 3. Build and Run Progression Dump
    let target_fixture =
        "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest3_BrainCainTalked.d2s";
    println!(
        "[v105_lab] Step 2: Running Progression Dump on {}...",
        target_fixture
    );

    let mut dump = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "v105_progression_dump",
            "--",
            target_fixture,
        ])
        .spawn()?;

    let status = dump.wait()?;
    if !status.success() {
        return Err("Progression dump failed to run".into());
    }

    println!("\n[v105_lab] Success! Alpha v105 lab cycle complete.");
    Ok(())
}
