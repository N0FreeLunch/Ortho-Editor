use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::item::serialization::HuffmanTree;
use d2r_core::error::ParsingError;
use d2r_macros::{invariant_test, settled_contract_test};
use std::io::{self, Cursor};

const ALPHABET: &str = " 0123456789abcdefghijklmnopqrstuvwxyz";
// Frozen before moving the live codec or wiring its new data owner.
const LEGACY_WIRE: &str = concat!(
    "101111101111111000011001101101111110100001011011011110111100",
    "010001110111100101010001100011100000100111101000011111111000",
    "010111001001011101011010011011111111110011101100111100001001",
    "1000000111011100000000111000101011011000",
);

fn pack(bits: &[bool]) -> Vec<u8> {
    let mut bytes = vec![0; bits.len().div_ceil(8)];
    for (index, &bit) in bits.iter().enumerate() {
        if bit {
            bytes[index / 8] |= 1 << (index % 8);
        }
    }
    bytes
}

fn wire(bits: &[bool]) -> String {
    bits.iter()
        .map(|&bit| if bit { '1' } else { '0' })
        .collect()
}

fn public_alias(tree: &d2r_core::domain::item::HuffmanTree) -> &d2r_core::item::HuffmanTree {
    tree
}

#[invariant_test]
fn legacy_alphabet_wire_and_public_aliases_are_preserved() {
    let tree = HuffmanTree::new();
    assert!(std::ptr::eq(public_alias(&tree), &tree));
    assert_eq!(wire(&tree.encode(ALPHABET).unwrap()), LEGACY_WIRE);
}

#[invariant_test]
fn all_symbol_pairs_preserve_decoding_visualization_and_bit_positions() {
    let tree = HuffmanTree::new();
    for left in ALPHABET.chars() {
        for right in ALPHABET.chars() {
            let text = format!("{left}{right}");
            let encoded = tree.encode(&text).unwrap();
            let mut framed = vec![true, false, true];
            framed.extend_from_slice(&encoded);
            framed.extend([true, false]);
            let bytes = pack(&framed);

            let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
            assert_eq!(reader.read::<3, u8>().unwrap(), 5);
            assert_eq!(tree.decode(&mut reader).unwrap(), left);
            assert_eq!(tree.decode(&mut reader).unwrap(), right);
            assert_eq!(reader.position_in_bits().unwrap(), 3 + encoded.len() as u64);
            assert_eq!(reader.read::<2, u8>().unwrap(), 1);

            let mut recorded = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            recorded.read_bits::<u8>(3).unwrap();
            assert_eq!(tree.decode_recorded(&mut recorded).unwrap(), left);
            assert_eq!(tree.decode_recorded(&mut recorded).unwrap(), right);
            assert_eq!(recorded.pos(), 3 + encoded.len() as u64);
            assert_eq!(recorded.read_bits::<u8>(2).unwrap(), 1);

            let mut visualized =
                BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            visualized.read_bits::<u8>(3).unwrap();
            for ch in [left, right] {
                let (decoded, bits) = tree.visualize_decode(&mut visualized).unwrap();
                assert_eq!(decoded, ch);
                assert_eq!(bits, tree.encode(&ch.to_string()).unwrap());
            }
            assert_eq!(visualized.pos(), 3 + encoded.len() as u64);
            assert_eq!(visualized.read_bits::<u8>(2).unwrap(), 1);
        }
    }
}

#[settled_contract_test]
fn truncated_prefixes_preserve_error_positions_context_and_hints() {
    let tree = HuffmanTree::new();
    for ch in ALPHABET.chars() {
        let encoded = tree.encode(&ch.to_string()).unwrap();
        let mut framed = vec![true, false, true];
        framed.extend_from_slice(&encoded);
        let bytes = pack(&framed);
        for cut in 0..encoded.len() {
            for visualization in [false, true] {
                let mut cursor =
                    BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                cursor.read_bits::<u8>(3).unwrap();
                cursor.push_context("HuffmanContract");
                cursor.set_limit(3 + cut as u64);
                let failure = if visualization {
                    tree.visualize_decode(&mut cursor).unwrap_err()
                } else {
                    tree.decode_recorded(&mut cursor).unwrap_err()
                };
                assert!(
                    matches!(failure.error, ParsingError::InvalidHuffmanBit { bit_offset } if bit_offset == 3 + cut as u64)
                );
                assert_eq!(cursor.pos(), 3 + cut as u64);
                assert_eq!(failure.bit_offset, cursor.pos());
                assert_eq!(failure.context_relative_offset, cut as u64);
                assert_eq!(failure.context_stack, ["HuffmanContract"]);
                let hint = if visualization {
                    "Unexpected EOF during Huffman visualization"
                } else {
                    "Huffman bit pattern does not match Alpha v105 table. Possible bit drift or misalignment."
                };
                assert_eq!(failure.hint.as_deref(), Some(hint));
            }
        }
    }
}

#[settled_contract_test]
fn empty_input_and_unsupported_symbols_preserve_io_contracts() {
    let tree = HuffmanTree::new();
    assert!(tree.encode("").unwrap().is_empty());
    for ch in ['#', 'A', char::from_u32(0xe9).unwrap()] {
        let error = tree.encode(&ch.to_string()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            format!("Char '{ch}' not in Huffman table")
        );
    }
    let mut reader = BitReader::endian(Cursor::new(&[] as &[u8]), LittleEndian);
    assert_eq!(
        tree.decode(&mut reader).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}
