//! Property value consumption and materialization from a selected layout.
//! Recovery and layout selection stay with the caller.
use crate::data::bit_cursor::BitCursor;
use crate::data::stat_costs::StatCost;
use crate::domain::item::entity::ItemBitRange;
use crate::domain::stats::{ItemProperty, StatMaturityTier};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(super) struct PropertyValueLayout<'a> {
    pub entry_start: u64,
    pub stat_id: u32,
    pub param: u32,
    pub effective_width: u32,
    pub recovered: bool,
    pub stat_cost: Option<&'a StatCost>,
}

pub(super) fn read_property_values<R: BitRead>(
    recorder: &mut BitCursor<R>,
    layout: PropertyValueLayout<'_>,
    paired_stat_ids: &[(u32, &[u32])],
    consume_paired_stats: impl FnOnce() -> bool,
    mut maturity: impl FnMut(u32, bool) -> StatMaturityTier,
) -> ParsingResult<Vec<ItemProperty>> {
    let PropertyValueLayout {
        entry_start,
        stat_id,
        param,
        effective_width,
        recovered: handled,
        stat_cost,
    } = layout;
    let raw_value;

    if !handled {
        if effective_width > 32 {
            recorder.skip_and_record(effective_width)?;
            raw_value = 0;
        } else {
            raw_value = recorder.read_bits::<u32>(effective_width)? as i32;
        }
    } else {
        raw_value = 0;
    }

    recorder.push_context(&format!("Stat({})", stat_id));
    let entry_end = recorder.pos();
    recorder.pop_context();
    let is_unknown_stat = stat_cost.is_none();
    let logical_value = if handled || effective_width > 32 {
        0
    } else {
        stat_cost
            .map(|stat| raw_value.wrapping_sub(stat.save_add))
            .unwrap_or(raw_value)
    };

    let property = ItemProperty {
        stat_id,
        raw_value: raw_value as i32,
        param,
        name: if is_unknown_stat {
            "opaque_property".to_string()
        } else {
            stat_cost.map(|s| s.name.to_string()).unwrap_or_default()
        },
        value: logical_value,
        range: ItemBitRange {
            start: entry_start,
            end: entry_end,
        },
        is_opaque: is_unknown_stat,
        opaque_bits: None,
        maturity: maturity(stat_id, !is_unknown_stat),
    };
    let mut properties = vec![property];

    if consume_paired_stats() {
        let companion_stat_ids =
            paired_stat_ids
                .iter()
                .find_map(|(paired_stat_id, companion_stat_ids)| {
                    (*paired_stat_id == stat_id).then_some(*companion_stat_ids)
                });
        if let Some(companion_stat_ids) = companion_stat_ids {
            for companion_stat_id in companion_stat_ids.iter().copied() {
                if let Some(companion_stat) = crate::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|stat| stat.id == companion_stat_id)
                {
                    let companion_start = recorder.pos();
                    let companion_width = companion_stat.save_bits as u32;
                    let companion_raw_value = if companion_width > 32 {
                        recorder.skip_and_record(companion_width)?;
                        0
                    } else {
                        recorder.read_bits::<u32>(companion_width)? as i32
                    };
                    let companion_end = recorder.pos();
                    properties.push(ItemProperty {
                        stat_id: companion_stat_id,
                        raw_value: companion_raw_value,
                        param: 0,
                        name: companion_stat.name.to_string(),
                        value: companion_raw_value.wrapping_sub(companion_stat.save_add),
                        range: ItemBitRange {
                            start: companion_start,
                            end: companion_end,
                        },
                        is_opaque: false,
                        opaque_bits: None,
                        maturity: maturity(companion_stat_id, true),
                    });
                }
            }
        }
    }

    Ok(properties)
}
