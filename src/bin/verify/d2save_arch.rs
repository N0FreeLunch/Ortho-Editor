use anyhow::{Context};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::{env, fs, process, ffi::OsString};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let subcommand = &args[1];
    match subcommand.as_str() {
        "dump" => handle_dump(&args[2..])?,
        "diff" => handle_diff(&args[2..])?,
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("Unknown subcommand: {}", subcommand);
            print_usage();
            process::exit(1);
        }
    }

    Ok(())
}

fn print_usage() {
    println!("d2save_arch - Forensic Bitstream Tool");
    println!("\nUsage:");
    println!("  d2save_arch <subcommand> [options]");
    println!("\nSubcommands:");
    println!("  dump    Dump bitstream from a file");
    println!("  diff    Compare bitstreams of two files");
    println!("\nOptions for 'dump':");
    println!("  -i, --file <path>        Input file");
    println!("  -s, --start-byte <int>   Start byte offset (default: 0)");
    println!("  -b, --bit-offset <int>   Bit offset from start byte (default: 0)");
    println!("  -l, --length <int>       Number of bits to dump (default: 100)");
    println!("\nOptions for 'diff':");
    println!("  -a, --file-a <path>      First file");
    println!("  -b, --file-b <path>      Second file");
    println!("  -o, --bit-offset <int>   Target bit offset for comparison");
    println!("  -w, --window <int>       Window size around offset (default: 64)");
}

fn handle_dump(args: &[String]) -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_arch dump");
    parser.add_spec(ArgSpec::option("file", Some('i'), Some("file"), "Input file").value_count(1));
    parser.add_spec(ArgSpec::option("start-byte", Some('s'), Some("start-byte"), "Start byte offset").value_count(1));
    parser.add_spec(ArgSpec::option("bit-offset", Some('b'), Some("bit-offset"), "Bit offset from start byte").value_count(1));
    parser.add_spec(ArgSpec::option("length", Some('l'), Some("length"), "Number of bits to dump").value_count(1));

    let parsed = match parser.parse(args.iter().map(|s| OsString::from(s)).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let path = parsed.get("file").context("Input file is required (use --file or -i)")?;
    let start_byte: u64 = parsed.get("start-byte").map(|s| s.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let bit_offset: u64 = parsed.get("bit-offset").map(|s| s.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let length: u64 = parsed.get("length").map(|s| s.as_str()).unwrap_or("100").parse().unwrap_or(100);

    let bytes = fs::read(path).with_context(|| format!("Failed to read file: {}", path))?;
    let total_start_bit = (start_byte * 8) + bit_offset;

    println!("Bits @ {} (File: {}):", total_start_bit, path);
    let mut bits = String::new();
    for i in 0..length {
        let current_bit = total_start_bit + i;
        let byte_idx = (current_bit / 8) as usize;
        let bit_idx = (current_bit % 8) as u8;

        if byte_idx < bytes.len() {
            let bit = if (bytes[byte_idx] & (1 << bit_idx)) != 0 { '1' } else { '0' };
            bits.push(bit);
        } else {
            break;
        }
    }
    println!("{}", bits);

    Ok(())
}

fn handle_diff(args: &[String]) -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_arch diff");
    parser.add_spec(ArgSpec::option("file-a", Some('a'), Some("file-a"), "First file").value_count(1));
    parser.add_spec(ArgSpec::option("file-b", Some('b'), Some("file-b"), "Second file").value_count(1));
    parser.add_spec(ArgSpec::option("bit-offset", Some('o'), Some("bit-offset"), "Target bit offset").value_count(1));
    parser.add_spec(ArgSpec::option("window", Some('w'), Some("window"), "Window size").value_count(1));

    let parsed = match parser.parse(args.iter().map(|s| OsString::from(s)).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let path_a = parsed.get("file-a").context("File A is required (use --file-a or -a)")?;
    let path_b = parsed.get("file-b").context("File B is required (use --file-b or -b)")?;
    let target_offset: i64 = parsed.get("bit-offset").context("Bit offset is required (use --bit-offset or -o)")?.parse()?;
    let window_size: i64 = parsed.get("window").map(|s| s.as_str()).unwrap_or("64").parse().unwrap_or(64);

    let bytes_a = fs::read(path_a).with_context(|| format!("Failed to read file A: {}", path_a))?;
    let bytes_b = fs::read(path_b).with_context(|| format!("Failed to read file B: {}", path_b))?;

    let start_bit = std::cmp::max(0, target_offset - (window_size / 2));
    let end_bit = std::cmp::min(
        std::cmp::min(bytes_a.len() as i64 * 8, bytes_b.len() as i64 * 8),
        start_bit + window_size
    );

    println!("Forensic Drill-down (BitWindow {})", window_size);
    println!("  File A: {}", path_a);
    println!("  File B: {}", path_b);
    println!("  Target Bit: {}", target_offset);
    println!();
    println!(" Offset | A | B | Diff");
    println!("--------|---|---|------");

    for i in start_bit..end_bit {
        let byte_idx = (i / 8) as usize;
        let bit_idx = (i % 8) as u8;

        let bit_a = if byte_idx < bytes_a.len() {
            if (bytes_a[byte_idx] & (1 << bit_idx)) != 0 { "1" } else { "0" }
        } else {
            "-"
        };

        let bit_b = if byte_idx < bytes_b.len() {
            if (bytes_b[byte_idx] & (1 << bit_idx)) != 0 { "1" } else { "0" }
        } else {
            "-"
        };

        let marker = if bit_a != bit_b { "  ***" } else { "" };
        let pointer = if i == target_offset { " <--" } else { "" };

        println!("{:>7} | {} | {} |{}{}", i, bit_a, bit_b, marker, pointer);
    }

    Ok(())
}
