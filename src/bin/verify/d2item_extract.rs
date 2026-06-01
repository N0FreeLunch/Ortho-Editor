use bitstream_io::{BitWrite, BitWriter, LittleEndian};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_extract").description(
        "Extracts specific item bits from a D2R save file into a standalone .d2i file",
    );

    parser.add_spec(ArgSpec::positional(
        "input_save",
        "path to the source save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "item_index",
        "zero-based index of the item to extract",
    ));
    parser.add_spec(ArgSpec::positional(
        "output_file",
        "path to save the extracted item bits (.d2i)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let input_path = parsed.get("input_save").unwrap();
    let target_index: usize = parsed
        .get("item_index")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("item_index must be a non-negative integer"))?;
    let output_path = parsed.get("output_file").unwrap();

    println!("=== d2item_extract ===");
    println!("  Input:  {}", input_path);
    println!("  Index:  {}", target_index);
    println!("  Output: {}", output_path);
    println!();

    // Load the save file
    let bytes =
        fs::read(input_path).map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", input_path, e))?;

    // Find first JM
    let jm_pos =
        (0..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M');

    let jm = match jm_pos {
        Some(p) => p,
        None => anyhow::bail!("No JM marker found in '{}'", input_path),
    };

    let item_count = u16::from_le_bytes([bytes[jm + 2], bytes[jm + 3]]);
    if target_index >= item_count as usize {
        anyhow::bail!(
            "item_index {} is out of range. File has {} items.",
            target_index,
            item_count
        );
    }

    let huffman = HuffmanTree::new();
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items = Item::read_player_items(&bytes, &huffman, version == 6 || version == 105)
        .map_err(|e| anyhow::anyhow!("Failed to parse item section: {}", e))?;
    let item = items.get(target_index).ok_or_else(|| {
        anyhow::anyhow!("Item at index {} not found during parsing", target_index)
    })?;

    println!("  Found item: '{}' ({} bits)", item.code, item.bits.len());
    let bits = item.bits.clone();

    // Convert bits to bytes for .d2i storage.
    let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    for bit in bits {
        writer.write_bit(bit.bit)?;
    }
    writer.byte_align()?; // Force byte alignment

    let result_bytes = writer.into_writer();

    fs::write(output_path, &result_bytes)
        .map_err(|e| anyhow::anyhow!("Cannot write to '{}': {}", output_path, e))?;

    println!();
    println!(
        "[OK] Extracted item to {}. Final size: {} bytes ({} bits)",
        output_path,
        result_bytes.len(),
        result_bytes.len() * 8
    );

    Ok(())
}
