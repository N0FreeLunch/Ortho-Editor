use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, Item};
use std::env;
use std::fs;
use std::io::Cursor;

fn print_bits_window(bytes: &[u8], start_bit: usize, bit_count: usize) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(start_bit as u32);
    println!("Bits from {} ({} bits):", start_bit, bit_count);
    for i in 0..bit_count {
        let bit = reader.read_bit().unwrap_or(false);
        print!("{}", if bit { '1' } else { '0' });
        if (i + 1) % 8 == 0 {
            print!(" ");
        }
        if (i + 1) % 32 == 0 {
            println!("(bit {})", start_bit + i + 1);
        }
    }
    println!();
}

fn read_bits(reader: &mut BitReader<Cursor<&[u8]>, LittleEndian>, count: u32) -> u32 {
    reader.read_var(count).unwrap_or(0)
}

fn analyze_non_compact_item(bytes: &[u8], bit_start: usize, huffman: &HuffmanTree) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit_start as u32);
    let mut offset = bit_start;

    let flags = read_bits(&mut reader, 32);
    println!(
        "  flags           {:>5}-{:>5} = 0x{:08X}",
        offset,
        offset + 32,
        flags
    );
    offset += 32;

    let version = read_bits(&mut reader, 3);
    let mode = read_bits(&mut reader, 3);
    let location = read_bits(&mut reader, 4);
    let x = read_bits(&mut reader, 4);
    let y = read_bits(&mut reader, 4);
    let page = read_bits(&mut reader, 3);
    println!(
        "  version         {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        version
    );
    offset += 3;
    println!(
        "  mode            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        mode
    );
    offset += 3;
    println!(
        "  location        {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        location
    );
    offset += 4;
    println!("  x               {:>5}-{:>5} = {}", offset, offset + 4, x);
    offset += 4;
    println!("  y               {:>5}-{:>5} = {}", offset, offset + 4, y);
    offset += 4;
    println!(
        "  page            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        page
    );
    offset += 3;

    let code_start = offset;
    let mut code = String::new();
    for _ in 0..4 {
        code.push(huffman.decode(&mut reader).unwrap_or('?'));
    }
    let code_end = reader.position_in_bits().unwrap_or(0) as usize;
    println!(
        "  code            {:>5}-{:>5} = '{}'",
        code_start, code_end, code
    );
    offset = code_end;

    let socketed_count = read_bits(&mut reader, 3);
    println!(
        "  post-code bits  {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        socketed_count
    );
    offset += 3;

    let id = read_bits(&mut reader, 32);
    let level = read_bits(&mut reader, 7);
    let quality = read_bits(&mut reader, 4);
    let multi_graphics = read_bits(&mut reader, 1);
    println!(
        "  id              {:>5}-{:>5} = {}",
        offset,
        offset + 32,
        id
    );
    offset += 32;
    println!(
        "  level           {:>5}-{:>5} = {}",
        offset,
        offset + 7,
        level
    );
    offset += 7;
    println!(
        "  quality         {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        quality
    );
    offset += 4;
    println!(
        "  has graphics    {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        multi_graphics
    );
    offset += 1;
    if multi_graphics != 0 {
        let graphic_id = read_bits(&mut reader, 3);
        println!(
            "  graphic id      {:>5}-{:>5} = {}",
            offset,
            offset + 3,
            graphic_id
        );
        offset += 3;
    }

    let class_specific = read_bits(&mut reader, 1);
    println!(
        "  class specific  {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        class_specific
    );
    offset += 1;
    if class_specific != 0 {
        let class_bits = read_bits(&mut reader, 11);
        println!(
            "  class data      {:>5}-{:>5} = {}",
            offset,
            offset + 11,
            class_bits
        );
        offset += 11;
    }

    println!("  next 64 bits from {}", offset);
    print_bits_window(bytes, offset, 64);

    println!("  0x1FF candidates after {}", offset);
    for delta in 0..48 {
        let mut probe = BitReader::endian(Cursor::new(bytes), LittleEndian);
        let _ = probe.skip((offset + delta) as u32);
        if probe.read::<9, u32>().unwrap_or(0) == 0x1FF {
            println!("    offset {} -> bit {}", delta, offset + delta);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return;
    }
    let bytes = fs::read(&args[1]).unwrap();
    let huffman = HuffmanTree::new();

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    if let Ok(items) = Item::read_player_items(&bytes, &huffman, version == 105) {
        println!(
            "Library parse recovered {} top-level items from player section",
            items.len()
        );
        for (i, item) in items.iter().enumerate() {
            println!(
                "Item {:2}: '{:4}' mode={} loc={} flags=0x{:08X} name={:?} children={} range={}-{}", 
                i, item.code, item.header.mode, item.header.location, item.flags, item.personalized_player_name, item.socketed_items.len(), item.range.start, item.range.end
            );
            for prop in &item.properties {
                println!("  Prop: id={} value={} param={} bits {}-{}", 
                    prop.stat_id, prop.raw_value, prop.param, prop.range.start, prop.range.end);
            }



            for (socket_index, child) in item.socketed_items.iter().enumerate() {
                println!(
                    "  socket {:2}: '{}' mode={} loc={}",
                    socket_index, child.code, child.mode, child.location
                );
            }
        }
        return;
    }

    let jm_pos = (0..bytes.len() - 2)
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM not found");
    let item_count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    let next_jm = (jm_pos + 4..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .unwrap_or(bytes.len());
    let section_bytes = &bytes[jm_pos + 4..next_jm];
    let section_bits = (section_bytes.len() * 8) as u64;

    let mut reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    let mut visible_items: Vec<(usize, usize, Item)> = Vec::new();
    let mut raw_index = 0usize;

    while reader.position_in_bits().unwrap_or(section_bits) < section_bits {
        let _ = reader.byte_align();
        let pos = reader.position_in_bits().unwrap_or(0);
        if pos >= section_bits {
            break;
        }
        let bit_start = (jm_pos + 4) * 8 + pos as usize;

        match Item::from_reader(&mut reader, &huffman, version == 105) {
            Ok(item) => {
                let pos_end = reader.position_in_bits().unwrap_or(0);
                let bit_end = (jm_pos + 4) * 8 + pos_end as usize;
                if item.mode == 6 {
                    if let Some((_, _, parent)) = visible_items.last_mut() {
                        parent.socketed_items.push(item);
                    } else {
                        println!(
                            "Error at raw item {}: socketed item without a parent",
                            raw_index
                        );
                        break;
                    }
                } else {
                    visible_items.push((bit_start, bit_end, item));
                }
            }
            Err(e) => {
                if visible_items.len() >= item_count as usize {
                    println!(
                        "Stopped after {} visible items at raw item {}: {}",
                        visible_items.len(),
                        raw_index,
                        e
                    );
                    break;
                }
                println!("Error at raw item {}: {}", raw_index, e);
                analyze_non_compact_item(&bytes, bit_start, &huffman);
                break;
            }
        }

        raw_index += 1;
    }

    println!(
        "Parsed {} visible items from a section expecting {} top-level items",
        visible_items.len(),
        item_count
    );

    for (i, (bit_start, bit_end, item)) in visible_items.iter().enumerate() {
        println!(
            "Item {:2}: '{}' bits {}-{} loc={} socketed_children={}",
            i,
            item.code,
            bit_start,
            bit_end,
            item.location,
            item.socketed_items.len()
        );
        for (socket_index, child) in item.socketed_items.iter().enumerate() {
            println!(
                "  socket {:2}: '{}' loc={}",
                socket_index, child.code, child.location
            );
        }
    }
}
