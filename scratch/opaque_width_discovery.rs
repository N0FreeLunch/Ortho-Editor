use std::env;
use std::fs;
use d2r_core::save::find_jm_markers;
use d2r_core::data::bit_cursor::BitCursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: opaque_width_discovery <fixture_path>");
        return Ok(());
    }

    let fixture_path = &args[1];
    let bytes = fs::read(fixture_path)?;
    let jm_markers = find_jm_markers(&bytes);

    println!("Found {} JM markers in {}", jm_markers.len(), fixture_path);

    // Analyze gaps between markers to find fixed-width candidates
    for i in 0..jm_markers.len() - 1 {
        let current_jm = jm_markers[i];
        let next_jm = jm_markers[i+1];
        let diff_bits = (next_jm - current_jm);
        
        println!("Marker {} -> {}: diff={} bits", i, i+1, diff_bits);
    }

    Ok(())
}
