//! Item-code Huffman construction and decoding over the legacy data codebook.

use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingError, ParsingResult};
use bitstream_io::BitRead;
use std::io;

pub struct HuffmanTree {
    root: Box<HuffmanNode>,
    encoding_table: std::collections::HashMap<char, Vec<bool>>,
}

struct HuffmanNode {
    symbol: Option<char>,
    left: Option<Box<HuffmanNode>>,
    right: Option<Box<HuffmanNode>>,
}

impl HuffmanNode {
    fn new() -> Self {
        HuffmanNode {
            symbol: None,
            left: None,
            right: None,
        }
    }
}

impl HuffmanTree {
    pub fn new() -> Self {
        let mut root = Box::new(HuffmanNode::new());
        let mut encoding_table = std::collections::HashMap::new();
        for &(symbol, pattern) in crate::data::generated::layout_policy::ITEM_CODE_HUFFMAN_PATTERNS
        {
            let mut bits = Vec::new();
            let mut current = &mut root;
            for bit in pattern.chars() {
                if bit == '1' {
                    bits.push(true);
                    if current.right.is_none() {
                        current.right = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.right.as_mut().unwrap();
                } else {
                    bits.push(false);
                    if current.left.is_none() {
                        current.left = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.left.as_mut().unwrap();
                }
            }
            current.symbol = Some(symbol);
            encoding_table.insert(symbol, bits);
        }
        HuffmanTree {
            root,
            encoding_table,
        }
    }

    pub fn encode(&self, code: &str) -> io::Result<Vec<bool>> {
        let mut bits = Vec::new();
        for c in code.chars() {
            let pattern = self.encoding_table.get(&c).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Char '{}' not in Huffman table", c),
                )
            })?;
            bits.extend(pattern);
        }
        Ok(bits)
    }

    pub(in crate::domain) fn decode_internal<F: FnMut() -> io::Result<bool>>(
        &self,
        mut read_bit: F,
    ) -> io::Result<char> {
        let mut current = &self.root;
        loop {
            if let Some(symbol) = current.symbol {
                return Ok(symbol);
            }
            let bit = read_bit()?;
            current = if bit {
                current.right.as_ref()
            } else {
                current.left.as_ref()
            }
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid Huffman bit"))?;
        }
    }

    pub fn decode_recorded<R: BitRead>(&self, cursor: &mut BitCursor<R>) -> ParsingResult<char> {
        self.decode_internal(|| cursor.read_bit().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e))))
            .map_err(|_| {
                cursor.fail(ParsingError::InvalidHuffmanBit { bit_offset: cursor.pos() })
                    .with_hint("Huffman bit pattern does not match Alpha v105 table. Possible bit drift or misalignment.")
            })
    }

    pub fn visualize_decode<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
    ) -> ParsingResult<(char, Vec<bool>)> {
        let mut current = &self.root;
        let mut bits = Vec::new();
        loop {
            if let Some(symbol) = current.symbol {
                return Ok((symbol, bits));
            }
            let bit = cursor.read_bit().map_err(|_| {
                cursor
                    .fail(ParsingError::InvalidHuffmanBit {
                        bit_offset: cursor.pos(),
                    })
                    .with_hint("Unexpected EOF during Huffman visualization")
            })?;
            bits.push(bit);
            current = if bit {
                current.right.as_ref()
            } else {
                current.left.as_ref()
            }
            .ok_or_else(|| {
                cursor
                    .fail(ParsingError::InvalidHuffmanBit {
                        bit_offset: cursor.pos(),
                    })
                    .with_hint("Invalid bit path in Huffman tree during visualization")
            })?;
        }
    }

    pub fn decode<R: BitRead>(&self, reader: &mut R) -> io::Result<char> {
        self.decode_internal(|| reader.read_bit())
    }
}
