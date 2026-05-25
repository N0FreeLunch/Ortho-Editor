use bitstream_io::BitRead;
use crate::data::bit_cursor::BitCursor;
use crate::domain::item::{Item, ItemQuality};
use crate::domain::stats::entity::ItemProperty;
use crate::domain::stats::parser::read_item_stats;
use crate::domain::stats::axiom::StatsAxiom;
use crate::domain::item::subdomains::property::{PropertyNormalizer, AlphaPropertyCombinator};
use crate::item::{HuffmanTree, ParsingResult};

pub struct StatsCombinator;

impl StatsCombinator {
    pub fn read_stats<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
        code: &str,
        version: u8,
        ctx: Option<(&[u8], u64)>,
        huffman: &HuffmanTree,
        alpha_mode: bool,
        quality: Option<ItemQuality>,
        is_runeword: bool,
        is_v105_shadow: bool,
        is_personalized: bool,
        is_compact: bool,
        is_socketed: bool,
    ) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>, Vec<Item>)> {
        let (mut props, complete, term, v5_extra, unused_bits, shadow_bits, nested_items) = read_item_stats(
            cursor,
            code,
            version,
            ctx,
            huffman,
            alpha_mode,
            quality,
            is_runeword,
            is_v105_shadow,
            is_personalized,
            is_compact,
            is_socketed,
        )?;

        let quality_val = quality.unwrap_or(ItemQuality::Normal);
        let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
            .with_compact(is_compact)
            .with_code(code);
        
        AlphaPropertyCombinator.normalize(&mut props, code, &axiom);

        Ok((props, complete, term, v5_extra, unused_bits, shadow_bits, nested_items))
    }
}
