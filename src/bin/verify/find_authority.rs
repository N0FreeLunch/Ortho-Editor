use d2r_core::domain::item::serialization::HuffmanTree;
use d2r_core::domain::item::Item;
use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_authority_runeword.d2s";
    let bytes = fs::read(fixture_path).expect("Failed to read fixture file");
    let huffman = HuffmanTree::new();

    let items = Item::read_player_items(&bytes, &huffman, true)
        .expect("Failed to read player items from Authority fixture");

    println!("Authority Runeword Fixture Verification:");
    println!("Total recovered items: {}", items.len());

    let authority_opt = items.iter().find(|it| it.code.trim() == "xrs");
    if let Some(authority) = authority_opt {
        println!(
            "Found Authority Cuirass: code='{}', is_runeword={}, sockets={}, socket_children={:?}",
            authority.code.trim(),
            authority.is_runeword,
            authority.num_socketed_items,
            authority
                .socketed_items
                .iter()
                .map(|s| s.code.trim())
                .collect::<Vec<_>>()
        );
        for p in &authority.properties {
            println!(
                "  Property: stat_id={} name='{}' val={}",
                p.stat_id, p.name, p.value
            );
        }
    } else {
        eprintln!("ERROR: Could not locate 'xrs' Authority runeword in parsed items!");
        std::process::exit(1);
    }
}
