use d2r_core::item::HuffmanTree;
use d2r_core::save::{AttributeSection, Save, map_core_sections};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: dump_gf <file.d2s>");
        return;
    }

    let bytes = fs::read(&args[1]).expect("read fail");
    let map = map_core_sections(&bytes).expect("map fail");
    let attr = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos).expect("parse fail");

    println!("=== GF Stats ===");
    for entry in attr.entries {
        if let Some(bits) = entry.opaque_bits {
            println!("ID {:>3}: [Opaque bits len {}]", entry.stat_id, bits.len());
        } else {
            println!("ID {:>3}: Value {}", entry.stat_id, entry.raw_value);
        }
    }
}
