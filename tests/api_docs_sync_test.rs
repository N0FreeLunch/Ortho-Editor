//! Contract test to enforce that API documentation in `docs/api/` is never out of sync with code.
//!
//! If this test fails, someone modified a wire DTO without running `cargo run --bin gen_api_docs`.

use std::process::Command;

#[test]
fn test_api_documentation_and_schemas_in_sync() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let status = Command::new("cargo")
        .args(["run", "--bin", "gen_api_docs", "--", "--check"])
        .current_dir(manifest_dir)
        .status()
        .expect("failed to execute gen_api_docs --check");

    assert!(
        status.success(),
        "\n===================================================================\n\
         [API DOCUMENTATION OUT OF SYNC]\n\
         The on-disk API documentation and JSON schemas in `docs/api/` do not\n\
         match the latest Rust domain contracts.\n\n\
         Please run the following command to regenerate the documentation:\n\
           cargo run --bin gen_api_docs\n\
         ===================================================================\n"
    );
}
