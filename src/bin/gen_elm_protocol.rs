//! Generates `frontend/src/Generated/Protocol.elm` from Rust DTOs and optionally dumps
//! a projected `SessionView` JSON fixture for Elm/Node headless harness tests.

use std::fs;
use std::path::{Path, PathBuf};

use d2r_core::projection::{EditorSession, generate_elm_protocol_module};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    if args.first().map(String::as_str) == Some("--dump-fixture") {
        let d2s_path = args
            .get(1)
            .ok_or("Usage: gen_elm_protocol --dump-fixture <save.d2s> <out.json>")?;
        let out_path = args
            .get(2)
            .ok_or("Usage: gen_elm_protocol --dump-fixture <save.d2s> <out.json>")?;
        let bytes = fs::read(d2s_path)?;
        let label = Path::new(d2s_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture.d2s");
        let session = EditorSession::from_bytes(&bytes, label)?;
        let view = session.project();
        let json = serde_json::to_string_pretty(&view)?;
        if let Some(parent) = Path::new(out_path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out_path, format!("{json}\n"))?;
        println!("Wrote SessionView fixture to {out_path}");
        return Ok(());
    }

    let protocol_path: PathBuf = manifest_dir.join("frontend/src/Generated/Protocol.elm");
    if let Some(parent) = protocol_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let source = generate_elm_protocol_module()?;
    fs::write(&protocol_path, source)?;
    println!("Generated {}", protocol_path.display());
    Ok(())
}
