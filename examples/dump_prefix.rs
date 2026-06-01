use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    println!("JM marker at 0x{:X}", jm_pos);
    println!("Bytes: {:02X?}", &bytes[jm_pos..jm_pos + 64]);
}
