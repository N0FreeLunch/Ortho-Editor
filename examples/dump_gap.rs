use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    // jm_pos + 4 is bit 0 of item section.
    // Gap starts at bit 720 (byte 90)
    // Gap ends at bit 1040 (byte 130)
    let start_byte = jm_pos + 4 + 90;
    let end_byte = jm_pos + 4 + 130;

    println!(
        "Gap between item 8 and jav: 0x{:X} to 0x{:X}",
        start_byte, end_byte
    );
    let data = &bytes[start_byte..end_byte];
    println!("Gap bytes: {:02X?}", data);
}
