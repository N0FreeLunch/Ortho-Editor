//! Entry policies for the explicitly non-Alpha save path.
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    RETAIL_PROPERTY_RHYTHM_LAYOUT, UNKNOWN_PROPERTY_VALUE_BITS,
};
use crate::domain::stats::entity::StatMaturityTier;
use crate::domain::stats::property_entry::{
    PropertyEntryPolicy, PropertyEntryRecovery, PropertyRhythm, legacy_entry_value_width,
    trace_property_entry,
};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(super) struct RetailEntryPolicy<'a> {
    code: &'a str,
    consume_paired_stats: bool,
}

impl<'a> RetailEntryPolicy<'a> {
    pub(super) fn new(code: &'a str, consume_paired_stats: bool) -> Self {
        Self {
            code,
            consume_paired_stats,
        }
    }
}

impl PropertyEntryPolicy for RetailEntryPolicy<'_> {
    fn trace_entry(&self, entry_start: u64, stat_id: u32) {
        trace_property_entry(entry_start, stat_id, self.code);
    }
    fn rhythm(&self, _: u32) -> PropertyRhythm {
        PropertyRhythm::from_layout(&RETAIL_PROPERTY_RHYTHM_LAYOUT)
    }
    fn terminal_is_authority(&self) -> bool {
        false
    }
    fn tvs_padding_bits(&self) -> Option<u32> {
        None
    }
    fn map_id(&self, stat_id: u32) -> u32 {
        stat_id
    }
    fn omit_parameter(&self) -> bool {
        false
    }
    fn effective_width(
        &self,
        stat_id: u32,
        stat_save_bits: Option<u32>,
        rhythm: PropertyRhythm,
    ) -> u32 {
        let selected = match stat_save_bits {
            Some(bits) => rhythm.value_bits.unwrap_or(bits),
            None => u32::from(UNKNOWN_PROPERTY_VALUE_BITS),
        };
        legacy_entry_value_width(self.code, stat_id, selected, || stat_id)
    }
    fn consume_paired_stats(&self) -> bool {
        self.consume_paired_stats
    }
    fn maturity(&self, _: u32, known: bool) -> StatMaturityTier {
        if known {
            StatMaturityTier::Verified
        } else {
            StatMaturityTier::Speculative
        }
    }
}

pub(super) struct NoChildRecovery;

impl<R: BitRead, T> PropertyEntryRecovery<R, T> for NoChildRecovery {
    fn recover(&mut self, _: &mut BitCursor<R>, _: u32, _: &mut Vec<T>) -> ParsingResult<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use crate::data::bit_cursor::BitCursor;
    use crate::domain::item::{HuffmanTree, ItemQuality};
    use crate::domain::stats::StatsAxiom;
    use crate::domain::stats::parser::{
        PropertyReaderContext, parse_single_property, read_property_list,
    };
    use bitstream_io::{BitReader, LittleEndian};
    use d2r_macros::hypothesis_test;
    use std::io::Cursor;

    #[hypothesis_test]
    fn public_retail_entry_and_list_use_no_registry_in_a_fresh_process() {
        const CHILD: &str = "ORTHO_RETAIL_ENTRY_PROOF_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let missing = std::env::temp_dir().join(format!(
                "ortho-retail-absent-{}-{nonce}",
                std::process::id()
            ));
            assert!(!missing.exists());
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("domain::stats::retail_entry::tests::public_retail_entry_and_list_use_no_registry_in_a_fresh_process")
                .args(["--exact", "--test-threads=1", "--nocapture"])
                .env(CHILD, "1").env("D2R_DATA_PATH", &missing).output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(
                text.contains("1 passed; 0 failed;") && text.contains("@@RETAIL_NO_REGISTRY@@3")
            );
            assert!(!missing.exists());
            return;
        }
        let root = std::env::var_os("D2R_DATA_PATH").unwrap();
        assert!(!std::path::Path::new(&root).exists());
        let huffman = HuffmanTree::new();
        let mut axiom = StatsAxiom::new(255, ItemQuality::Normal, false);
        axiom.code = "plain".into();
        let bytes = (509u32 | (7u32 << 9)).to_le_bytes();
        let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
        cursor.set_limit(18);
        let (property, complete, _, children) = parse_single_property(
            &mut cursor,
            0,
            &huffman,
            true,
            &axiom,
            PropertyReaderContext {
                bytes: &bytes,
                item_start_bit: 0,
            },
            |_, _, _, _, _| panic!("retail entry called child recovery"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (property.stat_id, property.raw_value, cursor.pos()),
            (509, 7, 18)
        );
        assert!(property.is_opaque && !complete && children.is_empty());
        let bytes = (511u16 | (1u16 << 9)).to_le_bytes();
        let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
        cursor.set_limit(18);
        let (properties, complete, terminal, children) = read_property_list(
            &mut cursor,
            "plain",
            0,
            PropertyReaderContext {
                bytes: &bytes,
                item_start_bit: 0,
            },
            &huffman,
            true,
            true,
            &axiom,
            None,
            None,
            |_, _, _, _, _| panic!("retail list called child recovery"),
        )
        .unwrap();
        assert_eq!(
            (properties.len(), properties[0].stat_id, cursor.pos()),
            (1, 511, 16)
        );
        assert!(complete && terminal && children.is_empty());
        let mut mixed = StatsAxiom::new(3, ItemQuality::Normal, true);
        mixed.code = "plain".into();
        assert!(!mixed.is_alpha());
        let bytes = (509u32 | (255u32 << 9)).to_le_bytes();
        let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
        cursor.set_limit(17);
        let (property, _, _, _) = parse_single_property(
            &mut cursor,
            0,
            &huffman,
            false,
            &mixed,
            PropertyReaderContext {
                bytes: &bytes,
                item_start_bit: 0,
            },
            |_, _, _, _, _| panic!("unadmitted Alpha entry called child recovery"),
        )
        .unwrap()
        .unwrap();
        assert_eq!((property.raw_value, cursor.pos()), (255, 17));
        println!("@@RETAIL_NO_REGISTRY@@3");
    }
}
