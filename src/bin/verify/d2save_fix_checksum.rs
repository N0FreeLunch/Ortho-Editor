use anyhow::{Context, bail};
use d2r_core::engine::checksum::Checksum;
use std::env;
use std::fs;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: d2save_fix_checksum <file1> [file2...]");
        return Ok(());
    }

    for path in &args[1..] {
        let mut bytes = fs::read(path).with_context(|| format!("Failed to read {}", path))?;
        if bytes.len() < 16 {
            eprintln!(
                "[WARN] {} is too small for a D2S file (min 16 bytes), skipping.",
                path
            );
            continue;
        }

        let old_checksum = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        Checksum::fix(&mut bytes);
        let new_checksum = u32::from_le_bytes(bytes[12..16].try_into().unwrap());

        if old_checksum == new_checksum {
            println!(
                "[OK] {} checksum is already correct (0x{:08X}).",
                path, new_checksum
            );
        } else {
            fs::write(path, &bytes).with_context(|| format!("Failed to write {}", path))?;
            println!(
                "[FIXED] {} checksum updated: 0x{:08X} -> 0x{:08X}",
                path, old_checksum, new_checksum
            );
        }
    }

    Ok(())
}
