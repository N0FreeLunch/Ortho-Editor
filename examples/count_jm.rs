use std::fs;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: count_jm <save_file>");
        return;
    }
    let fixture_path = &args[1];
    let bytes = fs::read(fixture_path).expect("Fixture not found");
    
    let mut jm_positions = Vec::new();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'J' && bytes[i+1] == b'M' {
            jm_positions.push(i);
        }
    }
    
    println!("Found {} JM markers", jm_positions.len());
    for (i, pos) in jm_positions.iter().enumerate() {
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        println!("Marker {}: offset 0x{:X} ({}), count {}", i, pos, pos, count);
    }
}
