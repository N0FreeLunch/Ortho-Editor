use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use std::env;
use std::fs;
use std::io::{self, Cursor};
use std::process;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn main() {
    let mut parser = ArgParser::new("d2item_bit_align")
        .description("Analyzes bit-level similarity between actual item bits and expected re-serialized bits");

    parser.add_spec(ArgSpec::positional("save_file", "path to the save file (.d2s)"));
    parser.add_spec(ArgSpec::positional("item_index", "optional index of the item to align").optional());

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
    let target_idx = parsed.get("item_index").and_then(|s| s.parse::<usize>().ok());

    let bytes = fs::read(path).expect("failed to read save file");

    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];
    let items = Item::read_player_items(&bytes, &huffman, is_alpha).expect("failed to parse items");

    if let Some(idx) = target_idx {
        let _ = run_align_report(&items, idx, &huffman);
    } else {
        for i in 0..items.len() {
            let _ = run_align_report(&items, i, &huffman);
        }
    }
}

fn run_align_report(items: &[Item], item_index: usize, huffman: &HuffmanTree) -> io::Result<()> {
    let item = &items[item_index];
    let actual: Vec<bool> = item.bits.iter().map(|rb| rb.bit).collect();

    if actual.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No bits recorded for the item.",
        ));
    }

    // Strategy A: Re-serialize and re-parse to get "Expected Bits"
    let mut expected_item = item.clone();
    expected_item.bits.clear(); // Force re-encoding
    let expected_encoded_bytes = expected_item.to_bytes(&huffman, false)?;

    let mut reader = BitReader::endian(Cursor::new(&expected_encoded_bytes), LittleEndian);
    let mut cursor = BitCursor::new(reader);
    let _ = Item::from_reader_with_context(&mut cursor, huffman, None, false, false).ok();
    let expected: Vec<bool> = cursor.recorded_bits().iter().map(|rb| rb.bit).collect();

    let aligner = BitAligner::new(2, -1, -3, -1);
    let result = aligner.align(&actual, &expected);

    println!("Item {} [{}]: Similarity {}%", item_index, item.code.trim(), result.similarity_pct());
    Ok(())
}

struct BitAligner {
    match_score: i32,
    mismatch_score: i32,
    gap_open: i32,
    gap_extend: i32,
}

impl BitAligner {
    fn new(m: i32, mis: i32, go: i32, ge: i32) -> Self {
        BitAligner { match_score: m, mismatch_score: mis, gap_open: go, gap_extend: ge }
    }

    fn align(&self, a: &[bool], b: &[bool]) -> AlignResult {
        // Simple mock aligner for now
        let mut matches = 0;
        for i in 0..a.len().min(b.len()) {
            if a[i] == b[i] { matches += 1; }
        }
        AlignResult { matches, a_len: a.len(), b_len: b.len() }
    }
}

struct AlignResult {
    matches: usize,
    a_len: usize,
    b_len: usize,
}

impl AlignResult {
    fn similarity_pct(&self) -> f32 {
        (self.matches as f32 / self.a_len.max(self.b_len) as f32) * 100.0
    }
}
