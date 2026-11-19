//! Select existing set bonuses independently of localized rendering.

use crate::data::item_specs::ItemStatRange;
use crate::domain::item::specification::lookup_set_item;
use crate::data::sets::{get_set_group, get_set_items, get_set_total_pieces};
use crate::item::{Item, ItemBitRange, ItemProperty, ItemQuality};

pub(crate) enum PieceBonusMode {
    Requested,
    Fused,
}

pub(crate) struct PieceBonusDisplay<'a> {
    pub active: bool,
    pub required_count: u8,
    pub properties: &'a [ItemProperty],
    pub mode: PieceBonusMode,
}

pub(crate) struct GroupBonusDisplay {
    pub active: bool,
    pub required_count: u8,
    pub properties: Vec<ItemProperty>,
}

pub(crate) struct SetDisplay<'a> {
    pub name_key: &'static str,
    pub member_keys: Vec<&'static str>,
    pub pieces: Vec<PieceBonusDisplay<'a>>,
    pub groups: Vec<GroupBonusDisplay>,
}

fn group_property(stat: &ItemStatRange) -> ItemProperty {
    ItemProperty::new(
        stat.stat_id,
        String::new(),
        stat.param,
        stat.min,
        stat.min,
        ItemBitRange::default(),
    )
}

pub(crate) fn displayed_set_bonuses(
    item: &Item,
    active_set_count: usize,
) -> Option<SetDisplay<'_>> {
    if item.quality != Some(ItemQuality::Set) || !item.is_identified {
        return None;
    }
    let id = item.unique_id?;
    let set_item = lookup_set_item(id)?;
    let member_keys = get_set_items(set_item.set_id).map(|s| s.index).collect();
    let mut pieces = Vec::new();
    let flags = item
        .body
        .d2r_v3_auxiliary
        .as_ref()
        .and_then(|a| a.set_property_flags);

    if let Some(flags) = flags {
        let mut packed = 0;
        for (ordinal, present) in flags.into_iter().enumerate() {
            if present {
                if packed < item.set_attributes.len() {
                    let required_count = (ordinal + 2) as u8;
                    pieces.push(PieceBonusDisplay {
                        active: active_set_count >= required_count as usize,
                        required_count,
                        properties: &item.set_attributes[packed],
                        mode: PieceBonusMode::Requested,
                    });
                }
                packed += 1;
            }
        }
    } else {
        for (idx, list) in item.set_attributes.iter().enumerate() {
            let required_count = (idx + 2) as u8;
            pieces.push(PieceBonusDisplay {
                active: active_set_count >= required_count as usize,
                required_count,
                properties: list,
                mode: PieceBonusMode::Fused,
            });
        }
    }

    let mut groups = Vec::new();
    if let Some(group) = get_set_group(set_item.set_id) {
        let mut counts: Vec<u8> = group.partial.iter().map(|p| p.required_count).collect();
        counts.sort();
        counts.dedup();
        for count in counts {
            let properties = group
                .partial
                .iter()
                .filter(|p| p.required_count == count)
                .map(|bonus| group_property(&bonus.stat))
                .collect();
            groups.push(GroupBonusDisplay {
                active: active_set_count >= count as usize,
                required_count: count,
                properties,
            });
        }
        if !group.full.is_empty() {
            let total_pieces = get_set_total_pieces(set_item.set_id);
            groups.push(GroupBonusDisplay {
                active: active_set_count >= total_pieces,
                required_count: total_pieces as u8,
                properties: group.full.iter().map(group_property).collect(),
            });
        }
    }

    Some(SetDisplay {
        name_key: set_item.set_id,
        member_keys,
        pieces,
        groups,
    })
}
