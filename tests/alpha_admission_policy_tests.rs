use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::header::HeaderAxiom;
use d2r_core::domain::item::serialization::{
    write_property_list, BitEmitter, PropertyReaderContext,
};
use d2r_core::domain::stats::{parse_single_property, ItemProperty, StatsAxiom};
use d2r_core::error::ParsingError;
use d2r_core::item::{HuffmanTree, ItemBitRange, ItemQuality};
use d2r_macros::{hypothesis_test, invariant_test};
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

fn legacy_stats(version: u8, flag: bool) -> bool {
    flag && matches!(version, 0 | 1 | 2 | 4 | 5 | 6 | 7)
}

fn legacy_header(version: u8, flag: bool) -> bool {
    flag && matches!(version, 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7)
}

#[hypothesis_test]
fn distinct_public_domains_preserve_all_versions_and_independent_flags() {
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for stats_flag in [false, true] {
            let mut stats = StatsAxiom::new(version, ItemQuality::Unique, stats_flag);
            stats.code = "not-an-admission-selector".into();
            stats.is_compact = version & 1 != 0;
            stats.is_socketed = version & 2 != 0;
            stats.is_personalized = version & 4 != 0;
            for header_flag in [false, true] {
                let header = HeaderAxiom::new(version, header_flag);
                assert_eq!(stats.is_alpha(), legacy_stats(version, stats_flag));
                assert_eq!(
                    stats.uses_alpha_mapping(),
                    legacy_stats(version, stats_flag)
                );
                assert_eq!(header.is_alpha(), legacy_header(version, header_flag));
                cases += 1;
            }
        }
    }
    let stats = StatsAxiom::new(3, ItemQuality::Normal, true);
    assert!(!stats.is_alpha() && !stats.uses_alpha_mapping());
    assert!(HeaderAxiom::new(3, true).is_alpha());
    assert_eq!(cases, 1_024);
}

#[hypothesis_test]
fn actual_mapping_preserves_admission_and_raw_fallback() {
    let registry = get_registry();
    let ids = [26, 312, 207, 380, 256, 496, 97, 509, u32::MAX];
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for flag in [false, true] {
            let stats = StatsAxiom::new(version, ItemQuality::Normal, flag);
            for raw in ids {
                let expected = if legacy_stats(version, flag) {
                    registry
                        .mappings
                        .get(&raw.to_string())
                        .map_or(raw, |row| row.effective_id)
                } else {
                    raw
                };
                assert_eq!(
                    stats.map_alpha_id(raw),
                    expected,
                    "version {version}, flag {flag}, raw {raw}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 4_608);
}

fn wire(text: &str) -> Vec<bool> {
    text.bytes().map(|byte| byte == b'1').collect()
}

fn packed(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .zip([1, 2, 4, 8, 16, 32, 64, 128])
                .filter_map(|(&bit, place)| bit.then_some(place))
                .sum()
        })
        .collect()
}

#[invariant_test]
fn stats_admission_preserves_actual_wire_ranges_error_and_guard() {
    let huffman = HuffmanTree::new();
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for flag in [false, true] {
            let mut stats = StatsAxiom::new(version, ItemQuality::Normal, flag);
            stats.code = "Opaque".into();
            let width = if !legacy_stats(version, flag) {
                6
            } else if matches!(version, 0 | 1) {
                8
            } else {
                9
            };
            // Stat 97, parameter 341, value 1; explicit version 255 excludes default/parameter rewrites.
            let mut field = wire("1000011001010101011");
            field.extend(std::iter::repeat_n(false, width - 1));
            let bits: Vec<bool> = wire("101")
                .into_iter()
                .chain(field.iter().copied())
                .chain(wire("0110"))
                .collect();
            let bytes = packed(&bits);
            let property = ItemProperty::new(
                97,
                "admission witness".into(),
                341,
                1,
                1,
                ItemBitRange { start: 0, end: 0 },
            );
            let mut emitter = BitEmitter::new();
            emitter.extend_bits(wire("101")).unwrap();
            write_property_list(
                &mut emitter,
                "Opaque",
                &[property],
                &[],
                &huffman,
                255,
                false,
                false,
                false,
                ItemQuality::Normal,
                false,
                &stats,
            )
            .unwrap();
            assert_eq!(emitter.written_bits(), 3 + field.len() as u64);
            emitter.extend_bits(wire("0110")).unwrap();
            assert_eq!(emitter.bits(), bits);
            assert_eq!(emitter.into_bytes(), bytes);
            let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            cursor.read_bits::<u8>(3).unwrap();
            let (parsed, terminal, terminal_bit, nested) = parse_single_property(
                &mut cursor,
                255,
                &huffman,
                false,
                &stats,
                PropertyReaderContext {
                    bytes: &bytes,
                    item_start_bit: 0,
                },
                |_, _, _, _, _| panic!("non-nested admission witness must not recover children"),
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                (parsed.stat_id, parsed.param, parsed.raw_value, parsed.value),
                (97, 341, 1, 1)
            );
            assert_eq!(parsed.range.start, 3);
            assert_eq!(parsed.range.end, 3 + field.len() as u64);
            assert_eq!(cursor.pos(), parsed.range.end);
            assert_eq!(cursor.read_bits::<u8>(4).unwrap(), 6);
            assert!(!terminal && !terminal_bit && nested.is_empty() && !parsed.is_opaque);
            assert_eq!(
                cursor
                    .recorded_bits()
                    .iter()
                    .map(|bit| bit.bit)
                    .collect::<Vec<_>>(),
                bits
            );
            let mut truncated =
                BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            truncated.read_bits::<u8>(3).unwrap();
            let limit = bits.len() as u64 - 5;
            truncated.set_limit(limit);
            let error = parse_single_property(
                &mut truncated,
                255,
                &huffman,
                false,
                &stats,
                PropertyReaderContext {
                    bytes: &bytes,
                    item_start_bit: 0,
                },
                |_, _, _, _, _| panic!("no nested recovery"),
            )
            .unwrap_err();
            assert_eq!(
                error.error,
                ParsingError::Io("Bit limit exceeded (possible section boundary violation)".into())
            );
            assert_eq!(error.bit_offset, limit);
            assert_eq!(
                error.hint.as_deref(),
                Some(
                    "The parser tried to read beyond the allocated bits for this item or section."
                )
            );
            assert_eq!(truncated.pos(), limit);
            assert_eq!(
                truncated
                    .recorded_bits()
                    .iter()
                    .map(|bit| bit.bit)
                    .collect::<Vec<_>>(),
                bits[..limit as usize]
            );
            truncated.set_limit(bits.len() as u64);
            assert_eq!(truncated.read_bits::<u8>(5).unwrap(), 12);
            cases += 1;
        }
    }
    assert_eq!(cases, 512);
}

#[hypothesis_test]
fn rejected_mapping_does_not_read_registry_in_fresh_process() {
    const CHILD: &str = "D2R_ALPHA_ADMISSION_REGISTRY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let data_root = PathBuf::from(std::env::var_os("D2R_DATA_PATH").unwrap());
        assert!(
            !data_root.exists(),
            "worker requires a nonexistent registry directory"
        );
        let mut rejected = 0;
        for version in u8::MIN..=u8::MAX {
            for flag in [false, true] {
                let stats = StatsAxiom::new(version, ItemQuality::Normal, flag);
                assert_eq!(stats.is_alpha(), legacy_stats(version, flag));
                assert_eq!(stats.uses_alpha_mapping(), legacy_stats(version, flag));
                for header_flag in [false, true] {
                    assert_eq!(
                        HeaderAxiom::new(version, header_flag).is_alpha(),
                        legacy_header(version, header_flag)
                    );
                }
                if !legacy_stats(version, flag) {
                    assert_eq!(stats.map_alpha_id(26), 26);
                    rejected += 1;
                }
            }
        }
        assert_eq!(rejected, 505);
        println!("NON_ADMITTED_CASES=505");
        return;
    }
    let core_root = std::env::var_os("D2R_CORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let absent = core_root.join("tmp").join(format!(
        "alpha-admission-no-registry-{}-{nonce}",
        std::process::id()
    ));
    assert!(!absent.exists());
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rejected_mapping_does_not_read_registry_in_fresh_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .env("D2R_DATA_PATH", &absent)
        .output()
        .expect("spawn isolated admission test worker");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success(),
        "worker failed: {stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(stdout.contains("NON_ADMITTED_CASES=505"));
    assert!(stdout.contains("test result: ok. 1 passed; 0 failed;"));
    assert!(
        !absent.exists(),
        "worker must not create a registry fixture"
    );
}
