#[cfg(test)]
mod tests {
    use crate::domain::item::subdomains::affix::{AffixCombinator, MagicAffixSegment, RareAffixSegment, UniqueAffixSegment};
    use crate::domain::item::serialization::BitEmitter;
    use crate::data::bit_cursor::BitCursor;
    use bitstream_io::{BitReader, LittleEndian};
    use std::io::Cursor;

    #[test]
    fn test_magic_affix_roundtrip() {
        let magic = MagicAffixSegment {
            prefix: Some(123),
            suffix: Some(456),
        };
        
        let mut emitter = BitEmitter::new();
        magic.emit(&mut emitter).unwrap();
        
        let bits = emitter.into_bits();
        assert_eq!(bits.len(), 22);

        let bytes = bits_to_bytes(bits);
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let parsed = MagicAffixSegment::parse(&mut cursor).unwrap();
        
        assert_eq!(parsed.prefix, Some(123));
        assert_eq!(parsed.suffix, Some(456));
    }

    #[test]
    fn test_unique_affix_roundtrip() {
        let unique = UniqueAffixSegment { unique_id: Some(2048) };
        let mut emitter = BitEmitter::new();
        unique.emit(&mut emitter).unwrap();
        
        let bits = emitter.into_bits();
        assert_eq!(bits.len(), 12);

        let bytes = bits_to_bytes(bits);
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let parsed = UniqueAffixSegment::parse(&mut cursor).unwrap();
        
        assert_eq!(parsed.unique_id, Some(2048));
    }

    #[test]
    fn test_rare_affix_roundtrip() {
        let mut rare = RareAffixSegment::default();
        rare.names = [Some(10), Some(20)];
        rare.affixes[0] = Some(100);
        rare.affixes[2] = Some(300);
        
        let mut emitter = BitEmitter::new();
        rare.emit(&mut emitter).unwrap();
        
        let bits = emitter.into_bits();
        // 8+8 (names) + 6 bits (presence) + 2*11 (values) = 16 + 6 + 22 = 44 bits
        assert_eq!(bits.len(), 44);

        let bytes = bits_to_bytes(bits);
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let parsed = RareAffixSegment::parse(&mut cursor).unwrap();
        
        assert_eq!(parsed.names, [Some(10), Some(20)]);
        assert_eq!(parsed.affixes[0], Some(100));
        assert_eq!(parsed.affixes[1], None);
        assert_eq!(parsed.affixes[2], Some(300));
    }

    fn bits_to_bytes(bits: Vec<bool>) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit { byte |= 1 << i; }
            }
            bytes.push(byte);
        }
        bytes
    }
}
