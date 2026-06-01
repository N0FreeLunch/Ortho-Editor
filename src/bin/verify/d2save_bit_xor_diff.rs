use d2r_core::save::{gf_payload_range, map_core_sections};
use std::env;
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "Usage: cargo run --bin d2save_bit_xor_diff -- <golden.d2s> <target.d2s>"
        );
        return Ok(());
    }

    let file1_path = &args[1];
    let file2_path = &args[2];

    let bytes1 = fs::read(file1_path)?;
    let bytes2 = fs::read(file2_path)?;

    println!("Comparing Bits: {} vs {}", file1_path, file2_path);

    let map1 = map_core_sections(&bytes1).ok();
    let _map2 = map_core_sections(&bytes2).ok();

    let max_len = bytes1.len().max(bytes2.len());

    println!("Offset      | File A Bits | File B Bits | XOR (Delta) | Section / Context");
    println!("------------|-------------|-------------|-------------|------------------");

    for i in 0..max_len {
        let b1 = bytes1.get(i).cloned();
        let b2 = bytes2.get(i).cloned();

        if b1 != b2 {
            let bits1 = b1
                .map(|b| format!("{:08b}", b))
                .unwrap_or_else(|| "--------".to_string());
            let bits2 = b2
                .map(|b| format!("{:08b}", b))
                .unwrap_or_else(|| "--------".to_string());

            let xor_bits = match (b1, b2) {
                (Some(v1), Some(v2)) => format!("{:08b}", v1 ^ v2),
                _ => "--------".to_string(),
            };

            let context = get_context(i, map1.as_ref());

            println!(
                "0x{:04X} ({:>4}) | {} | {} | {} | {}",
                i, i, bits1, bits2, xor_bits, context
            );
        }
    }

    Ok(())
}

fn get_context(offset: usize, map: Option<&d2r_core::save::SaveSectionMap>) -> String {
    if offset < 16 {
        return "Header (Magic/Version/Size/Checksum)".to_string();
    }

    if let Some(m) = map {
        if offset == m.gf_pos || offset == m.gf_pos + 1 {
            return "Marker 'gf'".to_string();
        }

        let gf_range = gf_payload_range(m);
        if gf_range.contains(&offset) {
            return format!("Attributes Section (GF) +{}", offset - gf_range.start);
        }

        if offset == m.if_pos || offset == m.if_pos + 1 {
            return "Marker 'if'".to_string();
        }

        let skill_start = m.if_pos + 2;
        let skill_end = skill_start + 30; // SKILL_SECTION_LEN is 30
        if (skill_start..skill_end).contains(&offset) {
            return format!("Skills Section (IF) +{}", offset - skill_start);
        }

        for (idx, &jm_pos) in m.jm_positions.iter().enumerate() {
            if offset == jm_pos || offset == jm_pos + 1 {
                return format!("Marker 'JM' (Item Section {})", idx);
            }
        }
    }

    if offset >= 0x0100 && offset < 0x0200 {
        return "Likely Header / Quests Area".to_string();
    }

    "Unknown".to_string()
}
