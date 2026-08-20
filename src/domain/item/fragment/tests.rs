use super::header::HeaderFragment;
use super::option::OptionFragment;
use super::{BitFragment, FragmentContext};
use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use std::io::Cursor;

#[test]
fn test_option_fragment_faster_hit_recovery_99() {
    // Stat 99: item_fastergethitrate (FHR)
    let frag = OptionFragment::new(99, 0, 24);
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        frag.encode_to_bits(&mut writer).expect("encode failed");
        writer.byte_align().unwrap();
    }

    let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
    let ctx = FragmentContext::default();
    let decoded = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("decode failed");

    assert_eq!(decoded.stat_id, 99);
    assert_eq!(decoded.value, 24);
    assert_eq!(frag.bit_len(), 9 + frag.bit_width as usize);
}

#[test]
fn test_option_fragment_enhanced_defense_31() {
    // Stat 31: item_defense_percent (ED)
    let frag = OptionFragment::new(31, 0, 200);
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        frag.encode_to_bits(&mut writer).expect("encode failed");
        writer.byte_align().unwrap();
    }

    let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
    let ctx = FragmentContext::default();
    let decoded = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("decode failed");

    assert_eq!(decoded.stat_id, 31);
    assert_eq!(decoded.value, 200);
}

#[test]
fn test_option_fragment_durability_72_73() {
    // Stat 72: item_durability
    let frag_dur = OptionFragment::new(72, 0, 45);
    // Stat 73: item_maxdurability
    let frag_max = OptionFragment::new(73, 0, 45);

    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        frag_dur.encode_to_bits(&mut writer).expect("encode dur failed");
        frag_max.encode_to_bits(&mut writer).expect("encode max failed");
        writer.byte_align().unwrap();
    }

    let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
    let ctx = FragmentContext::default();
    let decoded_dur = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("decode dur failed");
    let decoded_max = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("decode max failed");

    assert_eq!(decoded_dur.stat_id, 72);
    assert_eq!(decoded_dur.value, 45);
    assert_eq!(decoded_max.stat_id, 73);
    assert_eq!(decoded_max.value, 45);
}

#[test]
fn test_option_fragment_all_skills_127() {
    // Stat 127: item_allskills
    let frag = OptionFragment::new(127, 0, 2);
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        frag.encode_to_bits(&mut writer).expect("encode failed");
        writer.byte_align().unwrap();
    }

    let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
    let ctx = FragmentContext::default();
    let decoded = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("decode failed");

    assert_eq!(decoded.stat_id, 127);
    assert_eq!(decoded.value, 2);
}

#[test]
fn test_option_fragment_with_param() {
    // Stat with parameter bits (e.g. single skill or class skills)
    let frag = OptionFragment::new(107, 3, 3).with_param(54, 9);
    assert_eq!(frag.bit_len(), 9 + 9 + 3);

    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        frag.encode_to_bits(&mut writer).expect("encode failed");
        writer.byte_align().unwrap();
    }

    assert_eq!(buffer.len(), 3); // 21 bits fits in 3 bytes
}

#[test]
fn test_header_fragment_roundtrip() {
    let mut code = [0u8; 4];
    code.copy_from_slice(b"amu ");
    let flags = 0x0000_0010; // identified

    let header = HeaderFragment::new(5, false, true, false, 4, code, flags);
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        header.encode_to_bits(&mut writer).expect("header encode failed");
        writer.byte_align().unwrap();
    }

    let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
    let ctx = FragmentContext {
        version: 5,
        is_alpha: false,
        code: Some("amu ".to_string()),
    };
    let decoded = HeaderFragment::decode_from_bits(&mut reader, &ctx).expect("header decode failed");

    assert_eq!(decoded.code, *b"amu ");
    assert_eq!(decoded.identified, true);
    assert_eq!(decoded.flags, flags);
}

#[test]
fn test_item_assembler_multi_option_assembly() {
    use super::assembler::ItemAssembler;

    let mut code = [0u8; 4];
    code.copy_from_slice(b"arm ");
    let header = HeaderFragment::new(5, false, true, false, 4, code, 0x0000_0010);

    let opt_fhr = OptionFragment::new(99, 0, 24);
    let opt_ed = OptionFragment::new(31, 0, 150);
    let opt_skills = OptionFragment::new(127, 0, 2);

    let assembler = ItemAssembler::new()
        .with_header(header)
        .add_option(opt_fhr)
        .add_option(opt_ed)
        .add_option(opt_skills)
        .with_terminator(true)
        .with_byte_align(true);

    let bytes = assembler.assemble_to_bytes().expect("assembly failed");
    assert!(!bytes.is_empty());

    // Read back and verify sequentially
    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    let ctx = FragmentContext {
        version: 5,
        is_alpha: false,
        code: Some("arm ".to_string()),
    };

    let decoded_header = HeaderFragment::decode_from_bits(&mut reader, &ctx).expect("header decode failed");
    assert_eq!(decoded_header.code, *b"arm ");

    let dec_fhr = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("fhr decode failed");
    assert_eq!(dec_fhr.stat_id, 99);
    assert_eq!(dec_fhr.value, 24);

    let dec_ed = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("ed decode failed");
    assert_eq!(dec_ed.stat_id, 31);
    assert_eq!(dec_ed.value, 150);

    let dec_skills = OptionFragment::decode_from_bits(&mut reader, &ctx).expect("skills decode failed");
    assert_eq!(dec_skills.stat_id, 127);
    assert_eq!(dec_skills.value, 2);

    // Verify terminator sentinel
    let sentinel: u16 = reader.read::<9, u16>().expect("sentinel read failed");
    assert_eq!(sentinel, 0x1FF);
}
