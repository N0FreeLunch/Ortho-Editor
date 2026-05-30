use d2r_core::inventory::InventoryGrid;
use d2r_core::item::HuffmanTree;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2save_grid")
        .description("Displays a visual grid map of the inventory for a D2R save file");

    parser.add_spec(ArgSpec::positional(
        "save_file",
        "path to the save file (.d2s)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let path = parsed.get("save_file").unwrap();
    let bytes = fs::read(path).unwrap_or_else(|e| {
        eprintln!("[ERROR] Cannot read '{}': {}", path, e);
        process::exit(1);
    });

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let huffman = HuffmanTree::new();
    let grid = InventoryGrid::from_save_bytes(&bytes, &huffman);

    println!("=== Inventory Grid Map: {} ===", path);
    grid.debug_print();
    println!();

    // Also list details
    println!("[ITEM LIST]");
    let jm_pos =
        (0..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M');

    if let Some(jm) = jm_pos {
        let item_count = u16::from_le_bytes([bytes[jm + 2], bytes[jm + 3]]);
        println!("  Total items: {}", item_count);
        let mut reader = bitstream_io::BitReader::endian(
            std::io::Cursor::new(&bytes[jm + 4..]),
            bitstream_io::LittleEndian,
        );

        for i in 0..item_count {
            let _ = bitstream_io::BitRead::byte_align(&mut reader);
            if let Ok(item) =
                d2r_core::item::Item::from_reader(&mut reader, &huffman, version == 105)
            {
                let (w, h) = d2r_core::inventory::get_item_size(&item.code);
                println!(
                    "  [{:>2}] {:<4} | Size: {}x{} | Pos: ({}, {}) | Loc: {}",
                    i, item.code, w, h, item.x, item.y, item.location
                );
            }
        }
    }
}
