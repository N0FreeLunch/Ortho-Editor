// d2r-core/src/bin/inspect/d2item_bit_visualizer.rs

use anyhow::Context;
use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use std::{env, fs, io::Cursor};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: d2item_bit_visualizer <item_file> [--alpha]");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let is_alpha_flag = args.iter().any(|arg| arg == "--alpha");
    let bytes = fs::read(file_path).context("Failed to read item file")?;

    let huffman = HuffmanTree::new();
    let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    let mut cursor = BitCursor::new(reader);
    cursor.set_trace(true);

    // Alpha mode detection
    let is_alpha = is_alpha_flag || file_path.contains("alpha") || file_path.contains("v105");

    println!("Visualizing item: {}", file_path);
    println!("File size: {} bytes ({} bits)", bytes.len(), bytes.len() * 8);

    // Ensure item trace is enabled via environment for core logic if needed,
    // though we already called set_trace(true) on the cursor.
    unsafe { env::set_var("D2R_ITEM_TRACE", "1"); }

    let res = Item::from_reader_with_context(
        &mut cursor,
        &huffman,
        None,
        is_alpha,
        0,
        None,
        None
    );

    match res {
        Ok(item) => {
            println!("\nParsing Successful!");
            print_visualizer_output(&cursor);
            if !item.socketed_items.is_empty() {
                println!("\nNested Items ({}):", item.socketed_items.len());
                for (i, child) in item.socketed_items.iter().enumerate() {
                    println!("  [{}] code={} bits={}", i, child.code, child.total_bits);
                }
            }
        }
        Err(e) => {
            println!("\nParsing Failed: {}", e);
            print_visualizer_output(&cursor);
            print_desync_context(&bytes, cursor.pos());
        }
    }

    Ok(())
}

fn print_visualizer_output<R: bitstream_io::BitRead>(cursor: &BitCursor<R>) {
    println!("\nSemantic Bitstream Mapping:");
    println!("{:<45} | {:<10} | {}", "AST Node / Segment", "Bits", "Bitstream");
    println!("{:-<45}-|-{:-<10}-|-{:-<60}", "", "", "");

    let segments = cursor.segments();
    let recorded_bits = cursor.recorded_bits();
    let base_offset = recorded_bits.first().map(|rb| rb.offset).unwrap_or(0);

    for segment in segments {
        let indent = "  ".repeat(segment.depth);
        let mut bits_str = String::new();
        
        let bit_len = segment.end - segment.start;
        for i in segment.start..segment.end {
            let idx = (i - base_offset) as usize;
            if let Some(rb) = recorded_bits.get(idx) {
                bits_str.push(if rb.bit { '1' } else { '0' });
            }
        }
        
        // Group bits into 8-bit chunks for readability
        let mut grouped_bits = String::new();
        for (i, c) in bits_str.chars().enumerate() {
            grouped_bits.push(c);
            if (i + 1) % 8 == 0 && (i + 1) < bits_str.len() {
                grouped_bits.push(' ');
            }
        }

        println!("{:<45} | {:>10} | {}", 
            format!("{}{}", indent, segment.label),
            bit_len,
            grouped_bits
        );
    }
}

fn print_desync_context(bytes: &[u8], error_pos: u64) {
    println!("\nDesync Context (around bit {}):", error_pos);
    
    let start_bit = error_pos.saturating_sub(64);
    let end_bit = (error_pos + 64).min((bytes.len() * 8) as u64);
    
    print!("{:>5}: ", start_bit);
    for bit_idx in start_bit..end_bit {
        let byte_idx = (bit_idx / 8) as usize;
        let bit_in_byte = (bit_idx % 8) as u8;
        
        if bit_idx == error_pos {
            print!("\x1b[91m[\x1b[0m");
        }
        
        if byte_idx < bytes.len() {
            let bit = (bytes[byte_idx] >> bit_in_byte) & 1 == 1;
            print!("{}", if bit { '1' } else { '0' });
        }
        
        if bit_idx == error_pos {
            print!("\x1b[91m]\x1b[0m");
        }
        
        if (bit_idx + 1) % 8 == 0 && (bit_idx + 1) < end_bit {
            print!(" ");
        }
        
        if (bit_idx + 1) % 64 == 0 && (bit_idx + 1) < end_bit {
            println!();
            print!("{:>5}: ", bit_idx + 1);
        }
    }
    println!();
}
