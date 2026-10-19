use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::item::{BitEmitter, HuffmanTree, ItemBitRange, ItemQuality};
use d2r_core::domain::stats::{
    ItemProperty, PropertyReaderContext, StatsAxiom, parse_single_property, read_property_list,
    write_property_list,
};
use d2r_core::error::ParsingError;
use d2r_macros::{hypothesis_test, invariant_test};
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

const VERSIONS: [u8; 9] = [0, 1, 2, 4, 5, 6, 7, 3, 255];
const GUARD: [bool; 5] = [false, true, true, false, true];

// Freeze legacy observations independently of the candidate data record.
fn admitted(version: u8, alpha: bool) -> bool {
    alpha && matches!(version, 0 | 1 | 2 | 4 | 5 | 6 | 7)
}

fn tvs(version: u8, alpha: bool, rw: bool, compact: bool) -> bool {
    alpha && matches!(version, 0 | 1 | 2 | 4 | 6) && !rw && !compact
}

fn prefix(offset: usize) -> Vec<bool> {
    (0..offset).map(|i| i % 2 == 0).collect()
}

fn packed(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0, |byte, (i, &bit)| byte | (u8::from(bit) << i))
        })
        .collect()
}

fn axiom(version: u8, alpha: bool, compact: bool, code: &str) -> StatsAxiom {
    let mut a = StatsAxiom::new(version, ItemQuality::Normal, alpha);
    a.code = code.into();
    a.is_compact = compact;
    a
}

#[invariant_test]
fn writer_auto_terminator_preserves_explicit_version_and_inferred_runeword_authority() {
    let registry = get_registry();
    assert!(!registry.mappings.contains_key("511"));
    assert!(
        !d2r_core::data::stat_costs::STAT_COSTS
            .iter()
            .any(|stat| stat.id == 511)
    );
    assert!(
        !registry
            .forced_runeword_codes
            .as_ref()
            .is_some_and(|codes| codes.iter().any(|code| code == "plain"))
    );
    assert!(
        registry
            .item_overrides
            .as_ref()
            .and_then(|items| items.get("plain"))
            .and_then(|row| row.get("is_runeword"))
            .is_none_or(|&value| value == 0)
    );
    let tree = HuffmanTree::new();
    let terminator = ItemProperty::new(
        511,
        "stream terminator witness".into(),
        0,
        0,
        0,
        ItemBitRange { start: 0, end: 0 },
    );
    let mut cases = 0;
    for receiver_version in VERSIONS {
        for explicit_version in u8::MIN..=u8::MAX {
            for alpha in [false, true] {
                for compact in [false, true] {
                    for complete in [false, true] {
                        for existing_term in [false, true] {
                            for inferred_rw in [false, true] {
                                for argument_rw in [false, true] {
                                    for offset in [0, 3] {
                                        let code = if inferred_rw { " xrs " } else { "plain" };
                                        let a = axiom(receiver_version, alpha, compact, code);
                                        let alpha_format = admitted(receiver_version, alpha);
                                        let generated = complete
                                            && !existing_term
                                            && (!alpha_format
                                                || matches!(
                                                    explicit_version,
                                                    0 | 1 | 2 | 4 | 5 | 6 | 7
                                                ))
                                            && !inferred_rw
                                            && !compact;
                                        let mut wire = prefix(offset);
                                        if existing_term || generated {
                                            wire.extend([true; 9]);
                                        }
                                        let mut capture = wire.clone();
                                        if complete && !alpha_format {
                                            wire.push(true);
                                            capture.push(true);
                                            while wire.len() % 8 != 0 {
                                                wire.push(false);
                                            }
                                        }
                                        if complete
                                            && tvs(receiver_version, alpha, argument_rw, compact)
                                        {
                                            wire.extend([false; 9]);
                                            capture.extend([false; 9]);
                                        }
                                        let mut emitter = BitEmitter::new();
                                        emitter.extend_bits(prefix(offset)).unwrap();
                                        let props = if existing_term {
                                            std::slice::from_ref(&terminator)
                                        } else {
                                            &[]
                                        };
                                        // Explicit code intentionally differs from the inferred receiver code.
                                        write_property_list(
                                            &mut emitter,
                                            "plain",
                                            props,
                                            &[],
                                            &tree,
                                            explicit_version,
                                            argument_rw,
                                            true,
                                            complete,
                                            ItemQuality::Normal,
                                            false,
                                            &a,
                                        )
                                        .unwrap();
                                        assert_eq!(emitter.written_bits(), wire.len() as u64);
                                        emitter.extend_bits(GUARD).unwrap();
                                        capture.extend(GUARD);
                                        wire.extend(GUARD);
                                        assert_eq!(
                                            emitter.bits(),
                                            capture,
                                            "receiver {receiver_version}, explicit {explicit_version}, alpha {alpha}, compact {compact}, complete {complete}, term {existing_term}, inferred {inferred_rw}, argument {argument_rw}, offset {offset}"
                                        );
                                        assert_eq!(emitter.into_bytes(), packed(&wire));
                                        cases += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 294_912);
}

#[hypothesis_test]
fn partial_id_prefix_never_accesses_registry_in_a_fresh_process() {
    const CHILD: &str = "D2R_PROPERTY_STREAM_PREFIX_CHILD";
    if std::env::var_os(CHILD).is_some() {
        assert!(!PathBuf::from(std::env::var_os("D2R_DATA_PATH").unwrap()).exists());
        let tree = HuffmanTree::new();
        let mut cases = 0;
        for version in u8::MIN..=u8::MAX {
            for alpha in [false, true] {
                for offset in [0, 3] {
                    for available in 0..9 {
                        let mut bits = prefix(offset);
                        bits.extend((0..9).map(|i| 509 & (1 << i) != 0));
                        bits.extend(GUARD);
                        let bytes = packed(&bits);
                        let mut cursor =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        for bit in prefix(offset) {
                            assert_eq!(cursor.read_bit().unwrap(), bit);
                        }
                        let limit = (offset + available) as u64;
                        cursor.set_limit(limit);
                        let error = parse_single_property(
                            &mut cursor,
                            255,
                            &tree,
                            false,
                            &axiom(version, alpha, false, "plain"),
                            PropertyReaderContext {
                                bytes: &bytes,
                                item_start_bit: 0,
                            },
                            |_, _, _, _, _| panic!("partial ID must not recover children"),
                        )
                        .unwrap_err();
                        assert_eq!(
                            error.error,
                            ParsingError::Io(
                                "Bit limit exceeded (possible section boundary violation)".into()
                            )
                        );
                        assert_eq!(error.bit_offset, limit);
                        assert_eq!(
                            error.hint.as_deref(),
                            Some(
                                "The parser tried to read beyond the allocated bits for this item or section."
                            )
                        );
                        assert_eq!(cursor.pos(), limit);
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits[..limit as usize]
                        );
                        cursor.set_limit(bits.len() as u64);
                        for bit in &bits[limit as usize..] {
                            assert_eq!(cursor.read_bit().unwrap(), *bit);
                        }
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert_eq!(cases, 9_216);
        println!("PARTIAL_PREFIX_CASES=9216");
        return;
    }
    let root = std::env::var_os("D2R_CORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let absent = root.join("tmp").join(format!(
        "property-prefix-no-registry-{}-{nonce}",
        std::process::id()
    ));
    assert!(!absent.exists());
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "partial_id_prefix_never_accesses_registry_in_a_fresh_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .env("D2R_DATA_PATH", &absent)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success(),
        "worker failed: {stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        stdout.contains("PARTIAL_PREFIX_CASES=9216")
            && stdout.contains("test result: ok. 1 passed; 0 failed;")
    );
    assert!(!absent.exists());
}

fn terminal_stream(
    offset: usize,
    receiver_version: u8,
    alpha: bool,
    argument_rw: bool,
) -> Vec<bool> {
    let mut bits = prefix(offset);
    bits.extend([true; 9]);
    if !admitted(receiver_version, alpha) {
        bits.push(true);
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
    }
    if tvs(receiver_version, alpha, argument_rw, false) {
        bits.extend([true, false, true, true, false, false, true, false, true]);
    }
    bits
}

#[invariant_test]
fn property_list_terminal_phase_preserves_ranges_limits_errors_and_remaining_tail() {
    let tree = HuffmanTree::new();
    let mut full_cases = 0;
    let mut partial_cases = 0;
    let mut early_cases = 0;
    let mut error_cases = 0;
    for receiver_version in VERSIONS {
        for alpha in [false, true] {
            for explicit_version in [0, 1, 3, 255] {
                for argument_rw in [false, true] {
                    for offset in 0..8 {
                        let a = axiom(receiver_version, alpha, false, "plain");
                        let mut bits =
                            terminal_stream(offset, receiver_version, alpha, argument_rw);
                        let end = bits.len() as u64;
                        bits.extend(GUARD);
                        let bytes = packed(&bits);
                        let mut cursor =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        for bit in prefix(offset) {
                            assert_eq!(cursor.read_bit().unwrap(), bit);
                        }
                        cursor.set_limit(bits.len() as u64);
                        let (props, complete, term_bit, nested) = read_property_list(
                            &mut cursor,
                            "plain",
                            explicit_version,
                            PropertyReaderContext {
                                bytes: &bytes,
                                item_start_bit: 0,
                            },
                            &tree,
                            argument_rw,
                            false,
                            &a,
                            None,
                            None,
                            |_, _, _, _, _| {
                                panic!("non-socketed terminal must not recover children")
                            },
                        )
                        .unwrap();
                        assert!(complete && nested.is_empty());
                        assert_eq!(term_bit, !admitted(receiver_version, alpha));
                        assert_eq!(props.len(), 1);
                        assert_eq!(
                            (props[0].stat_id, props[0].raw_value, props[0].param),
                            (511, 0, 0)
                        );
                        assert_eq!(
                            (props[0].range.start, props[0].range.end),
                            (offset as u64, end)
                        );
                        assert_eq!(cursor.pos(), end);
                        assert_eq!(cursor.limit(), Some(bits.len() as u64));
                        for bit in GUARD {
                            assert_eq!(cursor.read_bit().unwrap(), bit);
                        }
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits
                        );
                        full_cases += 1;
                        let term_end = offset as u64 + 9;
                        for available in 0..(end - term_end) {
                            let mut short = BitCursor::new(BitReader::endian(
                                Cursor::new(&bytes),
                                LittleEndian,
                            ));
                            for bit in prefix(offset) {
                                assert_eq!(short.read_bit().unwrap(), bit);
                            }
                            let limit = term_end + available;
                            short.set_limit(limit);
                            let result = read_property_list(
                                &mut short,
                                "plain",
                                explicit_version,
                                PropertyReaderContext {
                                    bytes: &bytes,
                                    item_start_bit: 0,
                                },
                                &tree,
                                argument_rw,
                                false,
                                &a,
                                None,
                                None,
                                |_, _, _, _, _| panic!("no nested recovery"),
                            );
                            // The list host stops before reading an ID when its exact bound is nine bits.
                            if available == 0 {
                                let (early_props, early_complete, early_term, early_nested) =
                                    result.unwrap();
                                assert!(
                                    early_props.is_empty()
                                        && early_nested.is_empty()
                                        && !early_term
                                );
                                assert_eq!(early_complete, admitted(receiver_version, alpha));
                                assert_eq!(short.pos(), offset as u64);
                                assert_eq!(short.limit(), Some(limit));
                                assert_eq!(
                                    short
                                        .recorded_bits()
                                        .iter()
                                        .map(|bit| bit.bit)
                                        .collect::<Vec<_>>(),
                                    prefix(offset)
                                );
                                short.set_limit(bits.len() as u64);
                                for bit in &bits[offset..] {
                                    assert_eq!(short.read_bit().unwrap(), *bit);
                                }
                                assert_eq!(
                                    short
                                        .recorded_bits()
                                        .iter()
                                        .map(|bit| bit.bit)
                                        .collect::<Vec<_>>(),
                                    bits
                                );
                                partial_cases += 1;
                                early_cases += 1;
                                continue;
                            }
                            let error = result.unwrap_err();
                            error_cases += 1;
                            assert_eq!(
                                error.error,
                                ParsingError::Io(
                                    "Bit limit exceeded (possible section boundary violation)"
                                        .into()
                                )
                            );
                            assert_eq!(error.bit_offset, limit);
                            assert_eq!(
                                error.hint.as_deref(),
                                Some(
                                    "The parser tried to read beyond the allocated bits for this item or section."
                                )
                            );
                            assert_eq!(short.pos(), limit);
                            assert_eq!(
                                short
                                    .recorded_bits()
                                    .iter()
                                    .map(|bit| bit.bit)
                                    .collect::<Vec<_>>(),
                                bits[..limit as usize]
                            );
                            short.set_limit(bits.len() as u64);
                            for bit in &bits[limit as usize..] {
                                assert_eq!(short.read_bit().unwrap(), *bit);
                            }
                            assert_eq!(
                                short
                                    .recorded_bits()
                                    .iter()
                                    .map(|bit| bit.bit)
                                    .collect::<Vec<_>>(),
                                bits
                            );
                            partial_cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(full_cases, 1_152);
    assert_eq!(partial_cases, 4_608);
    assert_eq!(early_cases, 864);
    assert_eq!(error_cases, 3_744);
}
