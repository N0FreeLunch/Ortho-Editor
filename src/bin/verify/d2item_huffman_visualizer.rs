use anyhow::Result;
use bitstream_io::{BitReader, LittleEndian};
use colored::*;
use d2r_core::data::BitCursor;
use d2r_core::domain::item::serialization::HuffmanTree;
use std::io::Cursor;

// Using a manual argument parser since clap was causing issues
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        println!(
            "Usage: d2item_huffman_visualizer --fixture <path> --start-bit <offset> [--length <bits>]"
        );
        return Ok(());
    }

    let mut fixture = String::new();
    let mut start_bit = 0;
    let mut length = 19;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--fixture" => {
                fixture = args[i + 1].clone();
                i += 2;
            }
            "--start-bit" => {
                start_bit = args[i + 1].parse()?;
                i += 2;
            }
            "--length" => {
                length = args[i + 1].parse()?;
                i += 2;
            }
            _ => i += 1,
        }
    }

    let bytes = std::fs::read(&fixture)?;

    let huffman = HuffmanTree::new();
    let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
    cursor.skip(start_bit)?;

    println!("{}", "--- Huffman Decoding Trace ---".cyan().bold());
    println!("Fixture: {}", fixture);
    println!("Start Bit: {}", start_bit);
    println!("Target Length: {} bits", length);
    println!();

    let mut bits_consumed = 0;
    let mut decoded_string = String::new();

    while bits_consumed < length {
        let bit_before = cursor.pos();
        match huffman.visualize_decode(&mut cursor) {
            Ok((ch, bits)) => {
                let bit_str: String = bits.iter().map(|&b| if b { '1' } else { '0' }).collect();
                let consumed = bits.len() as u64;
                bits_consumed += consumed;
                decoded_string.push(ch);

                println!(
                    "[{:4}] Char: '{}' | Bits: {:10} | Consumed: {:2} | Total: {}/{}",
                    bit_before,
                    ch.to_string().green().bold(),
                    bit_str.yellow(),
                    consumed,
                    bits_consumed,
                    length
                );
            }
            Err(e) => {
                println!(
                    "{}",
                    format!("Decoding Failed at bit {}: {}", cursor.pos(), e)
                        .red()
                        .bold()
                );
                break;
            }
        }
    }

    println!();
    println!("Final Decoded String: \"{}\"", decoded_string.cyan().bold());
    println!("Total Bits Consumed: {}", bits_consumed);

    Ok(())
}
