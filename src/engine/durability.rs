//! Project effective durability without changing the stored base or current fields.

use crate::data::gems::GEMS;
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::set_items::SET_ITEMS;
use crate::data::stat_costs::STAT_COSTS;
use crate::engine::validation::get_all_item_types;
use crate::item::{Item, ItemProperty, ItemQuality};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurabilityContextError {
    Unidentified,
    StoredField,
    Incomplete,
    Modifier,
    SetContext,
    SocketContext,
    Arithmetic,
}

impl DurabilityContextError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Unidentified => "Identify the item to determine its maximum durability.",
            Self::StoredField => "The stored durability fields cannot be interpreted reliably.",
            Self::Incomplete => "Unresolved item data prevents calculating maximum durability.",
            Self::Modifier => "A durability modifier cannot be interpreted reliably.",
            Self::SetContext => {
                "The active set effects are needed to determine maximum durability."
            }
            Self::SocketContext => "A socketed item's durability effects are not fully available.",
            Self::Arithmetic => "The effective maximum durability cannot be represented reliably.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurabilityDisplay {
    Final {
        current: u32,
        maximum: u32,
    },
    BaseOnly {
        current: u32,
        base: u32,
        reason: DurabilityContextError,
    },
}

fn explicit_v105(item: &Item) -> bool {
    !item.header.save_is_alpha
        && item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .is_some_and(|a| a.save_version == 105)
}

fn stored_scalar(id: u32, value: u32) -> bool {
    STAT_COSTS.iter().find(|s| s.id == id).is_some_and(|s| {
        let encoded = i64::from(value) + i64::from(s.save_add);
        (1..=32).contains(&s.save_bits) && encoded >= 0 && encoded < (1_i64 << s.save_bits)
    })
}

#[derive(Default)]
struct Modifiers {
    flat: i64,
    percent: i64,
}

impl Modifiers {
    fn add(&mut self, id: u32, param: u32, value: i32) -> Result<(), DurabilityContextError> {
        let target = match id {
            73 => &mut self.flat,
            75 => &mut self.percent,
            _ => return Ok(()),
        };
        if param != 0 {
            return Err(DurabilityContextError::Modifier);
        }
        *target = target
            .checked_add(i64::from(value))
            .ok_or(DurabilityContextError::Arithmetic)?;
        Ok(())
    }

    fn list(&mut self, list: &[ItemProperty]) -> Result<(), DurabilityContextError> {
        let mut seen = HashSet::new();
        for p in list {
            if p.is_opaque || p.opaque_bits.is_some() {
                return Err(DurabilityContextError::Incomplete);
            }
            if !matches!(p.stat_id, 73 | 75) {
                continue;
            }
            let valid = p.is_verified()
                && p.param == 0
                && seen.insert(p.stat_id)
                && STAT_COSTS
                    .iter()
                    .find(|s| s.id == p.stat_id)
                    .is_some_and(|s| {
                        (1..=31).contains(&s.save_bits)
                            && p.raw_value >= 0
                            && i64::from(p.raw_value) < (1_i64 << s.save_bits)
                            && p.raw_value.checked_sub(s.save_add) == Some(p.value)
                    });
            if !valid {
                return Err(DurabilityContextError::Modifier);
            }
            self.add(p.stat_id, p.param, p.value)?;
        }
        Ok(())
    }

    fn active_set(&mut self, item: &Item, active: usize) -> Result<(), DurabilityContextError> {
        if active <= 1
            || !item
                .set_attributes
                .iter()
                .flatten()
                .any(|p| p.is_opaque || p.opaque_bits.is_some() || matches!(p.stat_id, 73 | 75))
        {
            return Ok(());
        }
        let set = item
            .unique_id
            .filter(|_| item.quality == Some(ItemQuality::Set))
            .and_then(|id| SET_ITEMS.iter().find(|s| s.id == u32::from(id)))
            .ok_or(DurabilityContextError::SetContext)?;
        // Count-based activation is supported; membership-based activation needs the actual members.
        if set.add_func != 2 {
            return Err(DurabilityContextError::SetContext);
        }
        let flags = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .and_then(|a| a.set_property_flags)
            .ok_or(DurabilityContextError::SetContext)?;
        if flags.iter().filter(|&&x| x).count() != item.set_attributes.len() {
            return Err(DurabilityContextError::SetContext);
        }
        let mut packed = 0;
        for (ordinal, present) in flags.into_iter().enumerate() {
            if present {
                if active >= ordinal + 2 {
                    self.list(&item.set_attributes[packed])?;
                }
                packed += 1;
            }
        }
        Ok(())
    }

    fn sockets(&mut self, item: &Item) -> Result<(), DurabilityContextError> {
        let filled = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .map_or(0, |a| a.filled_sockets);
        if usize::from(filled) != item.socketed_items.len() {
            return Err(DurabilityContextError::SocketContext);
        }
        if item.socketed_items.is_empty() {
            return Ok(());
        }
        let template = ITEM_TEMPLATES
            .iter()
            .find(|t| t.code == item.code.trim())
            .ok_or(DurabilityContextError::SocketContext)?;
        let types = get_all_item_types(&item.code);
        let shield = types.contains(&"shld");
        if !template.is_weapon
            && (!template.is_armor
                || (!shield && !types.contains(&"helm") && !types.contains(&"tors")))
        {
            return Err(DurabilityContextError::SocketContext);
        }
        for child in &item.socketed_items {
            if child.header.save_is_alpha
                || !child.is_identified
                || !child.header.is_identified
                || child.is_runeword
                || !child.socketed_items.is_empty()
                || !child.runeword_attributes.is_empty()
                || child.set_attributes.iter().any(|x| !x.is_empty())
                || !child.is_completely_parsed()
            {
                return Err(DurabilityContextError::SocketContext);
            }
            if let Some(gem) = GEMS.iter().find(|g| g.code == child.code.trim()) {
                if !child.header.is_compact
                    || child
                        .properties
                        .iter()
                        .any(|p| p.is_opaque || p.opaque_bits.is_some() || p.stat_id != 511)
                {
                    return Err(DurabilityContextError::SocketContext);
                }
                let effects = if template.is_weapon {
                    gem.weapon_stats
                } else if shield {
                    gem.shield_stats
                } else {
                    gem.helm_stats
                };
                for p in effects.iter().filter(|p| matches!(p.stat_id, 73 | 75)) {
                    if p.min != p.max {
                        return Err(DurabilityContextError::SocketContext);
                    }
                    self.add(p.stat_id, p.param, p.min)?;
                }
            } else if child.code.trim() == "jew" && !child.header.is_compact && explicit_v105(child)
            {
                self.list(&child.properties)?;
            } else {
                return Err(DurabilityContextError::SocketContext);
            }
        }
        Ok(())
    }

    fn apply(self, base: u32) -> Result<u32, DurabilityContextError> {
        let base = i64::from(base);
        // op13 applies the merged coefficient to the stored base, then truncates once.
        let percent = base
            .checked_mul(self.percent)
            .ok_or(DurabilityContextError::Arithmetic)?
            / 100;
        let maximum = base
            .checked_add(self.flat)
            .and_then(|n| n.checked_add(percent))
            .ok_or(DurabilityContextError::Arithmetic)?;
        u32::try_from(maximum).map_err(|_| DurabilityContextError::Arithmetic)
    }
}

pub(crate) fn displayed_durability(
    item: &Item,
    active_set_count: usize,
) -> Option<DurabilityDisplay> {
    let (current, base) = item.current_durability().zip(item.max_durability())?;
    if base == 0 {
        return None;
    }
    if !explicit_v105(item) {
        return Some(DurabilityDisplay::Final {
            current,
            maximum: base,
        });
    }
    let calculate = || {
        if item.body.max_durability != item.max_durability
            || item.body.current_durability != item.current_durability
            || !stored_scalar(73, base)
            || !stored_scalar(72, current)
            || item.header.is_identified != item.is_identified
            || !ITEM_TEMPLATES
                .iter()
                .any(|t| t.code == item.code.trim() && (t.is_weapon || t.is_armor))
        {
            return Err(DurabilityContextError::StoredField);
        }
        if !item.is_identified {
            return Err(DurabilityContextError::Unidentified);
        }
        if !item.is_completely_parsed() {
            return Err(DurabilityContextError::Incomplete);
        }
        let mut modifiers = Modifiers::default();
        modifiers.list(&item.properties)?;
        if item.is_runeword {
            modifiers.list(&item.runeword_attributes)?;
        } else if item
            .runeword_attributes
            .iter()
            .any(|p| p.is_opaque || p.opaque_bits.is_some() || matches!(p.stat_id, 73 | 75))
        {
            return Err(DurabilityContextError::Modifier);
        }
        modifiers.active_set(item, active_set_count)?;
        modifiers.sockets(item)?;
        modifiers.apply(base)
    };
    Some(match calculate() {
        Ok(maximum) => DurabilityDisplay::Final { current, maximum },
        Err(reason) => DurabilityDisplay::BaseOnly {
            current,
            base,
            reason,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::entity::D2rV3AuxiliaryData;
    use crate::engine::formatter::format_item;
    use crate::item::{HuffmanTree, ItemBitRange};
    use d2r_macros::settled_contract_test;

    fn item(base: u32, current: u32) -> Item {
        let mut item = Item::empty_for_tests();
        item.code = "ob1 ".into();
        item.body.code = item.code.clone();
        item.is_identified = true;
        item.header.is_identified = true;
        item.header.save_is_alpha = false;
        item.is_compact = false;
        item.header.is_compact = false;
        item.properties_complete = true;
        item.body.max_durability = Some(base);
        item.max_durability = Some(base);
        item.body.current_durability = Some(current);
        item.current_durability = Some(current);
        item.body.d2r_v3_auxiliary = Some(D2rV3AuxiliaryData {
            save_version: 105,
            ..Default::default()
        });
        item
    }

    fn prop(id: u32, value: i32) -> ItemProperty {
        ItemProperty::new(
            id,
            String::new(),
            0,
            value + if id == 75 { 20 } else { 0 },
            value,
            ItemBitRange::default(),
        )
    }

    fn maximum(item: &Item, active: usize, expected: u32) {
        assert_eq!(
            displayed_durability(item, active),
            Some(DurabilityDisplay::Final {
                current: item.current_durability().unwrap(),
                maximum: expected,
            })
        );
    }

    fn fallback(item: &Item, reason: DurabilityContextError) {
        assert!(
            matches!(displayed_durability(item, 2), Some(DurabilityDisplay::BaseOnly { reason: r, .. }) if r == reason)
        );
    }

    #[settled_contract_test]
    fn actual_orb_uses_raw_base_and_effective_maximum_without_mutation() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act1/TESTDRUID_Quest1_DenOfEvil_Done_PreAkara.d2s"
        );
        let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
        let before = items.clone();
        let orb = &items[19];
        let raw = |start: usize, width: usize| {
            (0..width)
                .map(|n| u32::from((bytes[(start + n) / 8] >> ((start + n) % 8)) & 1) << n)
                .sum::<u32>()
        };
        assert_eq!(raw(9808, 8), 20);
        assert_eq!(raw(9816, 9), 22);
        assert_eq!(raw(9860, 9), 75);
        assert_eq!(raw(9869, 7), 31);
        assert_eq!(
            (orb.max_durability(), orb.current_durability()),
            (Some(20), Some(22))
        );
        maximum(orb, 0, 22);
        assert!(
            format_item(orb, "en", 0, 1)
                .base_attributes
                .contains(&"Durability: 22 of 22".into())
        );
        assert!(
            format_item(orb, "ko", 0, 1)
                .base_attributes
                .contains(&"내구력: 22 / 22".into())
        );
        assert_eq!(items, before);
    }

    #[settled_contract_test]
    fn percentages_merge_before_truncation_and_use_base_before_flat() {
        let mut x = item(19, 22);
        x.properties = vec![prop(75, 3)];
        x.is_runeword = true;
        x.runeword_attributes = vec![prop(75, 3)];
        maximum(&x, 0, 20);
        x.body.max_durability = Some(20);
        x.max_durability = Some(20);
        x.properties = vec![prop(73, 20), prop(75, 3)];
        maximum(&x, 0, 41);
    }

    #[settled_contract_test]
    fn negative_percentage_truncates_toward_zero_without_current_clamp_or_ethereal_reduction() {
        let mut x = item(19, 22);
        x.properties = vec![prop(75, -3)];
        maximum(&x, 0, 19);
        x.is_ethereal = true;
        maximum(&x, 0, 19);
        assert_eq!(x.current_durability(), Some(22));
    }

    #[settled_contract_test]
    fn count_qualified_packed_set_lists_activate_at_their_source_ordinal() {
        let mut x = item(20, 20);
        x.quality = Some(ItemQuality::Set);
        x.unique_id = Some(SET_ITEMS.iter().find(|s| s.add_func == 2).unwrap().id as u16);
        x.set_attributes = vec![vec![prop(75, 5)], vec![prop(75, 10)]];
        x.body.d2r_v3_auxiliary.as_mut().unwrap().set_property_flags =
            Some([true, false, true, false, false]);
        maximum(&x, 1, 20);
        maximum(&x, 2, 21);
        maximum(&x, 3, 21);
        maximum(&x, 4, 23);
        x.body.d2r_v3_auxiliary.as_mut().unwrap().set_property_flags =
            Some([true, false, false, false, false]);
        fallback(&x, DurabilityContextError::SetContext);
        x.unique_id = Some(SET_ITEMS.iter().find(|s| s.add_func != 2).unwrap().id as u16);
        fallback(&x, DurabilityContextError::SetContext);
    }

    #[settled_contract_test]
    fn jewel_contributes_once_and_unavailable_children_remain_unknown() {
        let mut x = item(19, 19);
        x.properties = vec![prop(75, 3)];
        x.is_runeword = true;
        x.runeword_attributes = vec![prop(75, 3)];
        let mut jewel = item(0, 0);
        jewel.code = "jew ".into();
        jewel.body.code = jewel.code.clone();
        jewel.properties = vec![prop(75, 3), prop(73, 5)];
        x.socketed_items.push(jewel);
        x.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 1;
        let snapshot = x.clone();
        maximum(&x, 0, 25);
        assert_eq!(x, snapshot);
        x.socketed_items[0].is_identified = false;
        fallback(&x, DurabilityContextError::SocketContext);
        x = snapshot;
        x.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 2;
        fallback(&x, DurabilityContextError::SocketContext);
    }

    #[settled_contract_test]
    fn compact_source_gem_is_not_double_counted_as_typed_child_properties() {
        let mut x = item(20, 20);
        x.properties = vec![prop(75, 11)];
        let mut gem = item(0, 0);
        gem.code = GEMS[0].code.into();
        gem.body.code = gem.code.clone();
        gem.is_compact = true;
        gem.header.is_compact = true;
        x.socketed_items.push(gem);
        x.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 1;
        maximum(&x, 0, 22);
        x.socketed_items[0].properties.push(prop(75, 11));
        fallback(&x, DurabilityContextError::SocketContext);
        x.socketed_items[0].properties.clear();
        x.socketed_items[0].code = "unknown".into();
        fallback(&x, DurabilityContextError::SocketContext);
    }

    #[settled_contract_test]
    fn incomplete_unidentified_and_invalid_scalar_contexts_never_claim_a_final_maximum() {
        let root = item(20, 22);
        let mut x = root.clone();
        x.is_identified = false;
        x.header.is_identified = false;
        fallback(&x, DurabilityContextError::Unidentified);
        let formatted = format_item(&x, "en", 0, 1);
        assert!(
            formatted
                .base_attributes
                .contains(&"Durability: 22 of ?".into())
        );
        assert!(
            formatted
                .base_attributes
                .contains(&"Base Maximum Durability: 20".into())
        );
        assert!(
            formatted
                .detailed_warnings
                .iter()
                .any(|w| w.category == "durability_context")
        );
        x = root.clone();
        x.properties_complete = false;
        fallback(&x, DurabilityContextError::Incomplete);
        x = root.clone();
        x.properties.push(ItemProperty::new_opaque(
            0,
            vec![true],
            ItemBitRange::default(),
        ));
        fallback(&x, DurabilityContextError::Incomplete);
        x = root.clone();
        x.properties = vec![prop(75, 11)];
        x.properties[0].raw_value += 1;
        fallback(&x, DurabilityContextError::Modifier);
        x.properties[0] = prop(75, 11);
        x.properties[0].param = 1;
        fallback(&x, DurabilityContextError::Modifier);
        x.properties = vec![prop(75, 11), prop(75, 11)];
        fallback(&x, DurabilityContextError::Modifier);
        x = root.clone();
        x.runeword_attributes = vec![prop(75, 11)];
        fallback(&x, DurabilityContextError::Modifier);
        x = root.clone();
        x.max_durability = Some(21);
        fallback(&x, DurabilityContextError::StoredField);
        x = root.clone();
        x.body.current_durability = Some(512);
        x.current_durability = Some(512);
        fallback(&x, DurabilityContextError::StoredField);
        x = root;
        x.body.max_durability = Some(256);
        x.max_durability = Some(256);
        fallback(&x, DurabilityContextError::StoredField);
    }

    #[settled_contract_test]
    fn legacy_alpha_and_missing_explicit_context_preserve_raw_display_and_omission() {
        let mut x = item(20, 22);
        x.properties = vec![prop(75, 11)];
        x.body.d2r_v3_auxiliary = None;
        maximum(&x, 0, 20);
        x.body.d2r_v3_auxiliary = Some(D2rV3AuxiliaryData {
            save_version: 104,
            ..Default::default()
        });
        maximum(&x, 0, 20);
        x.body.d2r_v3_auxiliary.as_mut().unwrap().save_version = 105;
        x.header.save_is_alpha = true;
        maximum(&x, 0, 20);
        x.body.max_durability = Some(0);
        x.max_durability = Some(0);
        assert_eq!(displayed_durability(&x, 0), None);
        x.body.max_durability = None;
        x.max_durability = None;
        assert_eq!(displayed_durability(&x, 0), None);
    }
}
