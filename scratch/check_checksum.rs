
fn calculate_alpha_v105_checksum(flags: u32, version: u8) -> u8 {
    let mut sum: u32 = 0;
    for i in 0..32 {
        if (flags & (1 << i)) != 0 {
            sum += 1;
        }
    }
    sum += version as u32;
    (sum & 0xFF) as u8
}

fn main() {
    let flags = 0x11008000;
    let version = 0;
    println!("Checksum for flags 0x{:X}, v={}: 0x{:X}", flags, version, calculate_alpha_v105_checksum(flags, version));
}
