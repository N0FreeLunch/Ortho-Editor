#[cfg(test)]
mod tests {
    use crate::domain::item::subdomains::gap::{GapCombinator, AlphaHeaderGap};
    use crate::domain::item::serialization::BitEmitter;
    use crate::data::bit_cursor::BitCursor;
    use bitstream_io::{BitReader, LittleEndian};
    use std::io::Cursor;

    #[test]
    fn test_alpha_header_gap_roundtrip() {
        let bits = vec![true, false, true, true];
        let gap = AlphaHeaderGap { bits: bits.clone() };
        
        let mut emitter = BitEmitter::new();
        gap.emit(&mut emitter).unwrap();
        
        let result = emitter.into_bits();
        assert_eq!(result, bits);
    }

    #[test]
    fn test_alpha_header_gap_zero() {
        let gap = AlphaHeaderGap { bits: vec![] };
        let mut emitter = BitEmitter::new();
        gap.emit(&mut emitter).unwrap();
        assert_eq!(emitter.into_bits().len(), 0);
    }

    #[test]
    fn test_alpha_header_gap_parse() {
        let data = vec![0b1010_1010u8];
        let mut reader = BitReader::endian(Cursor::new(&data), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        
        let gap = AlphaHeaderGap::parse(&mut cursor, 4).unwrap();
        // Little endian: first 4 bits of 0b1010_1010 (0xAA) are 0, 1, 0, 1
        assert_eq!(gap.bits, vec![false, true, false, true]);
    }
}
