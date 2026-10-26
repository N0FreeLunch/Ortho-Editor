use bitstream_io::BitRead;
use crate::data::bit_cursor::BitCursor;
use crate::domain::item::{Item, ItemQuality};
use crate::domain::stats::entity::ItemProperty;
use crate::domain::forensic::stats_recovery::{
    LegacyItemStatsReadContext, read_item_stats_normalized,
};
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
        flags: u32,
        is_runeword: bool,
        is_v105_shadow: bool,
        is_personalized: bool,
        is_compact: bool,
        is_socketed: bool,
    ) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>, Vec<Item>)> {
        read_item_stats_normalized(
            cursor,
            LegacyItemStatsReadContext {
                code,
                version,
                ctx,
                huffman,
                alpha_mode,
                quality,
                flags,
                is_runeword,
                is_v105_shadow,
                is_personalized,
                is_compact,
                is_socketed,
            },
        )
    }
}
