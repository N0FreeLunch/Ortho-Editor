use std::fs;
use std::process::Command;

#[test]
fn test_recreate_10_scrolls_from_empty_base() {
    let base_file = "tests/fixtures/savegames/original/amazon_empty.d2s";
    let target_file = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let output_file = "tests/fixtures/savegames/modified/recreated_10_scrolls.d2s";

    fs::create_dir_all("tests/fixtures/savegames/modified").unwrap();

    // 1. Extract items 4-15 from amazon_10_scrolls
    // We'll extract them as individual .d2i files first for this test
    let mut extracted_paths = Vec::new();
    for i in 4..16 {
        let path = format!("tests/fixtures/savegames/modified/item_{}.d2i", i);
        let status = Command::new(env!("CARGO_BIN_EXE_d2item_extract"))
            .arg(target_file)
            .arg(i.to_string())
            .arg(&path)
            .status()
            .expect("Failed to execute d2item_extract");
        assert!(status.success(), "Failed to extract item {}", i);
        extracted_paths.push(path);
    }

    // 2. Inject them one by one into the base file?
    // Or we can modify d2save_inject to take multiple items.
    // Actually, d2save_inject currently takes ONE item and a count.
    // Let's modify d2save_inject to support multiple .d2i files if passed.
    // For now, let's just do it sequentially (incremental building).

    let mut current_base = base_file.to_string();
    for (idx, path) in extracted_paths.iter().enumerate() {
        let next_output = format!("tests/fixtures/savegames/modified/step_{}.d2s", idx);
        let status = Command::new(env!("CARGO_BIN_EXE_d2save_inject"))
            .arg(&current_base)
            .arg(path)
            .arg("1")
            .arg(&next_output)
            .arg("--no-align")
            .status()
            .expect("Failed to execute d2save_inject");
        assert!(status.success(), "Failed to inject item {}", idx);
        current_base = next_output;
    }

    // Final result
    fs::copy(&current_base, output_file).unwrap();

    // 3. Compare item section with the golden master
    let output = Command::new(env!("CARGO_BIN_EXE_d2save_item_diff"))
        .arg(output_file)
        .arg(target_file)
        .output()
        .expect("Failed to execute d2save_item_diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("[IDENTICAL]"),
        "Verification failed! Item sections are not identical.\n{}",
        stdout
    );

    println!("Successfully recreated amazon_10_scrolls.d2s bit-perfectly!");
}

#[test]
fn test_basic_scroll_injection_validity() {
    let base_file = "tests/fixtures/savegames/original/amazon_empty.d2s";
    let source_file = "tests/fixtures/savegames/original/amazon_initial.d2s";
    let item_file = "tests/fixtures/savegames/modified/tsc_extracted_for_test.d2i";
    let output_file = "tests/fixtures/savegames/modified/injected_scroll.d2s";

    fs::create_dir_all("tests/fixtures/savegames/modified").unwrap();

    // 1. Extract a fresh scroll from initial save
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2item_extract")
        .arg("--")
        .arg(source_file)
        .arg("0") // The first item is a scroll
        .arg(item_file)
        .status()
        .expect("Failed to extract item for test");
    assert!(status.success());

    // 2. Inject 1 real scroll
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2save_inject")
        .arg("--")
        .arg(base_file)
        .arg(item_file)
        .arg("1")
        .arg(output_file)
        .status()
        .expect("Failed to execute d2save_inject");
    assert!(status.success());

    // Verify file integrity (checksum, JM markers)
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2save_verify")
        .arg("--")
        .arg(output_file)
        .status()
        .expect("Failed to execute d2save_verify");
    assert!(
        status.success(),
        "Generated save file has invalid structure or checksum"
    );
}

#[test]
fn test_multiple_scroll_injection_alignment_drift() {
    let base_file = "tests/fixtures/savegames/original/amazon_empty.d2s";
    let source_file = "tests/fixtures/savegames/original/amazon_initial.d2s";
    let item_file = "tests/fixtures/savegames/modified/tsc_extracted_for_test_multi.d2i";
    let output_file = "tests/fixtures/savegames/modified/injected_10_scrolls.d2s";

    fs::create_dir_all("tests/fixtures/savegames/modified").unwrap();

    // 1. Extract
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2item_extract")
        .arg("--")
        .arg(source_file)
        .arg("0")
        .arg(item_file)
        .status()
        .expect("Failed to extract item for test");
    assert!(status.success());

    // 2. Inject 10 real scrolls
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2save_inject")
        .arg("--")
        .arg(base_file)
        .arg(item_file)
        .arg("10")
        .arg(output_file)
        .status()
        .expect("Failed to execute d2save_inject");
    assert!(status.success());

    // Even if it doesn't match a golden master EXACTLY (due to coordinates),
    // it MUST have a valid structure.
    let status = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("d2save_verify")
        .arg("--")
        .arg(output_file)
        .status()
        .expect("Failed to execute d2save_verify");
    assert!(status.success());
}
