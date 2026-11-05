use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use std::env;
use std::fs;
use std::io::{self, Cursor};
use std::process;

fn main() -> io::Result<()> {
    let mut parser = ArgParser::new("d2item_bit_align").description(
        "Analyzes bit-level similarity between actual item bits and expected re-serialized bits",
    );

    parser.add_spec(ArgSpec::positional(
        "save_file",
        "path to the save file (.d2s)",
    ));
    parser.add_spec(
        ArgSpec::positional("item_index", "optional index of the item to align").optional(),
    );

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
    let target_idx = parsed
        .get("item_index")
        .and_then(|s| s.parse::<usize>().ok());
    let mut om = d2r_core::verify::OutputManager::try_new("d2item_bit_align", &parsed)
        .unwrap_or_else(|error| {
            eprintln!("Failed to initialize output destination: {error}");
            std::process::exit(1);
        });

    let bytes = fs::read(path).expect("failed to read save file");

    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];
    let items = Item::read_player_items(&bytes, &huffman, is_alpha).expect("failed to parse items");

    if om.is_json() {
        let payload = alignment_payload(&items, target_idx, &huffman);
        let status = payload.status();
        let report = Report::new(
            ReportMetadata::new("d2item_bit_align", path, env!("CARGO_PKG_VERSION")),
            status,
        )
        .with_results(payload);
        let json = serde_json::to_string_pretty(&report).map_err(io::Error::other)?;
        om.try_json(&json)?;
        om.try_finish()?;
        for item in &report.scan_results.as_ref().unwrap().items {
            om.try_summary(&item.summary())?;
        }
        return Ok(());
    }
    let mut emitted = false;
    if let Some(idx) = target_idx {
        emitted |= emit_align_report(&items, idx, &huffman, &mut om)?;
    } else {
        for i in 0..items.len() {
            emitted |= emit_align_report(&items, i, &huffman, &mut om)?;
        }
    }
    if !emitted {
        om.try_finish()?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct AlignmentRecord {
    item_index: usize,
    code: String,
    similarity_pct: f32,
}

impl AlignmentRecord {
    fn summary(&self) -> String {
        format!(
            "Item {} [{}]: Similarity {}%",
            self.item_index, self.code, self.similarity_pct
        )
    }
}

#[derive(serde::Serialize)]
struct AlignmentPayload {
    items: Vec<AlignmentRecord>,
    skipped_indices: Vec<usize>,
}

impl AlignmentPayload {
    fn status(&self) -> ReportStatus {
        if self.skipped_indices.is_empty() {
            ReportStatus::Ok
        } else {
            ReportStatus::Warn
        }
    }
}

fn alignment_payload(
    items: &[Item],
    target_idx: Option<usize>,
    huffman: &HuffmanTree,
) -> AlignmentPayload {
    let mut payload = AlignmentPayload {
        items: Vec::new(),
        skipped_indices: Vec::new(),
    };
    let mut add = |index| match run_align_report(items, index, huffman) {
        Ok(record) => payload.items.push(record),
        Err(_) => payload.skipped_indices.push(index),
    };
    if let Some(index) = target_idx {
        add(index);
    } else {
        for index in 0..items.len() {
            add(index);
        }
    }
    payload
}

fn emit_align_report(
    items: &[Item],
    item_index: usize,
    huffman: &HuffmanTree,
    om: &mut d2r_core::verify::OutputManager,
) -> io::Result<bool> {
    match run_align_report(items, item_index, huffman) {
        Ok(record) => {
            om.try_summary(&record.summary())?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

fn run_align_report(
    items: &[Item],
    item_index: usize,
    huffman: &HuffmanTree,
) -> io::Result<AlignmentRecord> {
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
    let expected_encoded_bytes = expected_item.to_bytes(0, &huffman, false)?;

    let mut reader = BitReader::endian(Cursor::new(&expected_encoded_bytes), LittleEndian);
    let mut cursor = BitCursor::new(reader);
    let _ = Item::from_reader_with_context(&mut cursor, huffman, None, false, 0, None, None).ok();
    let expected: Vec<bool> = cursor.recorded_bits().iter().map(|rb| rb.bit).collect();

    let aligner = BitAligner::new(2, -1, -3, -1);
    let result = aligner.align(&actual, &expected);

    Ok(AlignmentRecord {
        item_index,
        code: item.code.trim().to_string(),
        similarity_pct: result.similarity_pct(),
    })
}

struct BitAligner {
    match_score: i32,
    mismatch_score: i32,
    gap_open: i32,
    gap_extend: i32,
}

impl BitAligner {
    fn new(m: i32, mis: i32, go: i32, ge: i32) -> Self {
        BitAligner {
            match_score: m,
            mismatch_score: mis,
            gap_open: go,
            gap_extend: ge,
        }
    }

    fn align(&self, a: &[bool], b: &[bool]) -> AlignResult {
        // Simple mock aligner for now
        let mut matches = 0;
        for i in 0..a.len().min(b.len()) {
            if a[i] == b[i] {
                matches += 1;
            }
        }
        AlignResult {
            matches,
            a_len: a.len(),
            b_len: b.len(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_item_bits_remain_an_ignored_analysis_error() {
        let parsed = ArgParser::new("alignment-test").parse(Vec::new()).unwrap();
        let mut output =
            d2r_core::verify::OutputManager::try_new("alignment-test", &parsed).unwrap();
        let emitted = emit_align_report(&[Item::default()], 0, &HuffmanTree::new(), &mut output)
            .expect("analysis errors remain non-fatal");
        assert!(!emitted);
    }

    #[test]
    fn json_reports_skipped_analysis_as_warning() {
        let payload = alignment_payload(&[Item::default()], Some(0), &HuffmanTree::new());
        assert!(payload.items.is_empty());
        assert_eq!(payload.skipped_indices, vec![0]);
        assert_eq!(payload.status(), ReportStatus::Warn);
        let report = Report::new(
            ReportMetadata::new("d2item_bit_align", "fixture", "test"),
            payload.status(),
        )
        .with_results(payload);
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["status"], "warn");
        assert_eq!(
            json["scan_results"]["skipped_indices"],
            serde_json::json!([0])
        );
        assert_eq!(json["scan_results"]["items"], serde_json::json!([]));
    }
}
