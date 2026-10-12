//! Keep the explicit-v105 stored armor field separate from its display projection.
//! These checks prove encoding/layout, not an item's unknown creation history.

use crate::data::gems::GEMS;
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::set_items::SET_ITEMS;
use crate::domain::stats::{stat_save_add, stat_save_bits};
use crate::engine::validation::{ValidationWarning, WarningConfidenceTier, get_all_item_types};
use crate::item::{Item, ItemProperty, ItemQuality};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefenseContextError {
    StoredField,
    UnqualifiedModifier,
    WorldTime,
    SetMembers,
    SetLayout,
    SocketHost,
    SocketChild,
    OpaqueModifier,
    Arithmetic,
}

impl DefenseContextError {
    pub fn message(self) -> &'static str {
        match self {
            Self::StoredField => {
                "The stored defense field needs repair before calculating defense."
            }
            Self::UnqualifiedModifier => {
                "This defense modifier needs additional calculation support."
            }
            Self::WorldTime => "World-time context is required to calculate effective defense.",
            Self::SetMembers => "Specific equipped set members are required to calculate defense.",
            Self::SetLayout => "The conditional set properties cannot be activated reliably.",
            Self::SocketHost => "The socket effects cannot be applied to this armor type.",
            Self::SocketChild => "A socketed item's defense effects are not fully available.",
            Self::OpaqueModifier => {
                "An unresolved modifier prevents calculating effective defense."
            }
            Self::Arithmetic => "The effective defense cannot be represented reliably.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefenseDisplay {
    Final(u32),
    BaseOnly {
        value: u32,
        reason: DefenseContextError,
    },
}

pub(crate) fn is_explicit_v105(item: &Item) -> bool {
    !item.header.save_is_alpha
        && item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .is_some_and(|a| a.save_version == 105)
}

pub(crate) fn stored_defense_warnings(item: &Item) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    let Some(template) = ITEM_TEMPLATES.iter().find(|t| t.code == item.code.trim()) else {
        return warnings;
    };
    let mut warn = |category, message| {
        warnings.push(ValidationWarning::new(
            message,
            WarningConfidenceTier::HardInvariant,
            category,
        ));
    };
    if item.body.defense != item.defense {
        warn(
            "defense_mirror",
            "Stored defense disagrees with its legacy mirror.".to_string(),
        );
    }
    if !template.is_armor {
        if item.body.defense.is_some() {
            warn(
                "defense_layout",
                "A non-armor item contains a stored armor defense field.".to_string(),
            );
        }
        return warnings;
    }
    let Some(base) = item.body.defense else {
        warn(
            "defense_missing",
            "The armor's stored defense field is missing.".to_string(),
        );
        return warnings;
    };
    let representable = stat_save_bits(31)
        .zip(stat_save_add(31))
        .is_some_and(|(bits, add)| {
            if !(1..=32).contains(&bits) {
                return false;
            }
            let encoded = i64::from(base) + i64::from(add);
            encoded >= 0 && encoded < (1_i64 << bits)
        });
    if !representable {
        warn(
            "defense_encoding",
            format!("Stored defense {base} does not fit its save-field encoding."),
        );
    }
    warnings
}

#[derive(Default)]
struct Modifiers {
    percent: i64,
    flat: i64,
    per_level: i64,
}

impl Modifiers {
    fn add(&mut self, id: u32, param: u32, value: i32) -> Result<(), DefenseContextError> {
        if !matches!(id, 16 | 31 | 214 | 215 | 268 | 269) {
            return Ok(());
        }
        if param != 0 {
            return Err(DefenseContextError::UnqualifiedModifier);
        }
        if value == 0 {
            return Ok(());
        }
        let target = match id {
            16 => &mut self.percent,
            31 => &mut self.flat,
            214 => &mut self.per_level,
            268 | 269 => return Err(DefenseContextError::WorldTime),
            _ => return Err(DefenseContextError::UnqualifiedModifier),
        };
        *target = target
            .checked_add(i64::from(value))
            .ok_or(DefenseContextError::Arithmetic)?;
        Ok(())
    }

    fn list(&mut self, list: &[ItemProperty]) -> Result<(), DefenseContextError> {
        for prop in list {
            if prop.is_opaque || prop.opaque_bits.is_some() {
                return Err(DefenseContextError::OpaqueModifier);
            }
            self.add(prop.stat_id, prop.param, prop.value)?;
        }
        Ok(())
    }

    fn active_set(&mut self, item: &Item, active: usize) -> Result<(), DefenseContextError> {
        if active <= 1
            || !item
                .set_attributes
                .iter()
                .flatten()
                .any(|p| p.is_opaque || matches!(p.stat_id, 16 | 31 | 214 | 215 | 268 | 269))
        {
            return Ok(());
        }
        let set = item
            .unique_id
            .filter(|_| item.quality == Some(ItemQuality::Set))
            .and_then(|id| SET_ITEMS.iter().find(|s| s.id == u32::from(id)))
            .ok_or(DefenseContextError::SetLayout)?;
        if set.add_func != 2 {
            return Err(DefenseContextError::SetMembers);
        }
        let flags = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .and_then(|a| a.set_property_flags)
            .ok_or(DefenseContextError::SetLayout)?;
        if flags.iter().filter(|&&present| present).count() != item.set_attributes.len() {
            return Err(DefenseContextError::SetLayout);
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

    fn sockets(&mut self, item: &Item) -> Result<(), DefenseContextError> {
        let filled = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .map_or(0, |a| a.filled_sockets);
        if usize::from(filled) != item.socketed_items.len() {
            return Err(DefenseContextError::SocketChild);
        }
        if item.socketed_items.is_empty() {
            return Ok(());
        }
        let types = get_all_item_types(&item.code);
        let shield = types.contains(&"shld");
        if !shield && !types.contains(&"helm") && !types.contains(&"tors") {
            return Err(DefenseContextError::SocketHost);
        }
        for child in &item.socketed_items {
            if !child.socketed_items.is_empty() {
                return Err(DefenseContextError::SocketChild);
            }
            if let Some(gem) = GEMS.iter().find(|g| g.code == child.code.trim()) {
                // Compact gems/runes use host-specific source effects, not a second typed copy.
                let effects = if shield {
                    gem.shield_stats
                } else {
                    gem.helm_stats
                };
                for effect in effects {
                    if matches!(effect.stat_id, 16 | 31 | 214 | 215 | 268 | 269) {
                        if effect.min != effect.max {
                            return Err(DefenseContextError::SocketChild);
                        }
                        self.add(effect.stat_id, effect.param, effect.min)?;
                    }
                }
            } else if child.code.trim() == "jew" && child.is_identified {
                self.list(&child.properties)?;
            } else {
                return Err(DefenseContextError::SocketChild);
            }
        }
        Ok(())
    }

    fn apply(self, base: u32, level: u8) -> Result<u32, DefenseContextError> {
        let base = i64::from(base);
        // op13 uses the stored base; op4 merges coefficients before its arithmetic shift.
        let percent = base
            .checked_mul(self.percent)
            .ok_or(DefenseContextError::Arithmetic)?
            / 100;
        let per_level = i64::from(level)
            .checked_mul(self.per_level)
            .ok_or(DefenseContextError::Arithmetic)?
            >> 3;
        let total = base
            .checked_add(percent)
            .and_then(|v| v.checked_add(self.flat))
            .and_then(|v| v.checked_add(per_level))
            .ok_or(DefenseContextError::Arithmetic)?;
        u32::try_from(total).map_err(|_| DefenseContextError::Arithmetic)
    }
}

pub fn displayed_defense(
    item: &Item,
    active_set_count: usize,
    char_level: u8,
) -> Option<DefenseDisplay> {
    let base = item.defense()?;
    if !is_explicit_v105(item) || !item.is_identified {
        return Some(DefenseDisplay::Final(base));
    }
    let calculate = || {
        if !stored_defense_warnings(item).is_empty() {
            return Err(DefenseContextError::StoredField);
        }
        let mut modifiers = Modifiers::default();
        modifiers.list(&item.properties)?;
        if item.is_runeword {
            modifiers.list(&item.runeword_attributes)?;
        }
        modifiers.active_set(item, active_set_count)?;
        modifiers.sockets(item)?;
        modifiers.apply(base, char_level)
    };
    Some(match calculate() {
        Ok(value) => DefenseDisplay::Final(value),
        Err(reason) => DefenseDisplay::BaseOnly {
            value: base,
            reason,
        },
    })
}

/// The caller supplies only items in the authoritative equipment container.
pub fn equipped_set_piece_counts<'a>(
    items: impl IntoIterator<Item = &'a Item>,
) -> HashMap<&'static str, usize> {
    let mut groups: HashMap<&'static str, HashSet<u16>> = HashMap::new();
    for item in items {
        if item.is_identified
            && item.quality == Some(ItemQuality::Set)
            && let Some((id, set)) = item.unique_id.and_then(|id| {
                SET_ITEMS
                    .iter()
                    .find(|s| s.id == u32::from(id))
                    .map(|set| (id, set))
            })
        {
            groups.entry(set.set_id).or_default().insert(id);
        }
    }
    groups
        .into_iter()
        .map(|(group, ids)| (group, ids.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::entity::D2rV3AuxiliaryData;
    use crate::domain::item::stream::PlayerItemStream;
    use crate::domain::stats::ItemStats;
    use crate::engine::formatter::format_item;
    use crate::item::{HuffmanTree, ItemBitRange};
    use d2r_macros::settled_contract_test;

    fn armor(code: &str, base: u32) -> Item {
        let mut item = Item::default();
        item.code = code.into();
        item.body.code = code.into();
        item.body.defense = Some(base);
        item.defense = Some(base);
        item.is_identified = true;
        item.body.d2r_v3_auxiliary = Some(D2rV3AuxiliaryData {
            save_version: 105,
            ..Default::default()
        });
        item
    }

    fn prop(id: u32, value: i32) -> ItemProperty {
        ItemProperty::new(id, String::new(), 0, value, value, ItemBitRange::default())
    }

    fn socket(item: &mut Item, child: Item) {
        item.socketed_items.push(child);
        item.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets =
            item.socketed_items.len() as u8;
    }

    #[settled_contract_test]
    fn actual_cure_uses_runeword_percent_without_reapplying_ethereal() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Cure_xrn.d2s"
        );
        let stream = PlayerItemStream::read(bytes, &HuffmanTree::new()).unwrap();
        assert!(stream.diagnostics.is_empty());
        assert_eq!(stream.items.len(), 1);
        let item = &stream.items[0];
        let original = item.clone();
        assert_eq!(item.defense(), Some(162));
        assert!(item.is_ethereal);
        assert_eq!(
            displayed_defense(item, 0, 1),
            Some(DefenseDisplay::Final(286))
        );
        assert!(stored_defense_warnings(item).is_empty());
        assert_eq!(
            format_item(item, "en", 0, 1).base_attributes[0],
            "Defense: 286"
        );
        assert_eq!(*item, original);
    }

    #[settled_contract_test]
    fn root_runeword_jewel_and_rune_effects_merge_once() {
        let mut item = armor("cap", 101);
        item.is_ethereal = true;
        item.is_runeword = true;
        item.properties = vec![prop(16, 10), prop(31, 5)];
        item.runeword_attributes = vec![prop(16, 15)];
        let mut jewel = Item::default();
        jewel.code = "jew".into();
        jewel.is_identified = true;
        jewel.properties = vec![prop(31, 7)];
        socket(&mut item, jewel);
        let mut pul = Item::default();
        pul.code = "r21".into();
        socket(&mut item, pul);
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(168))
        );
        assert_eq!(item.defense(), Some(101));
    }

    #[settled_contract_test]
    fn gem_effects_follow_shield_or_helm_host_and_reject_nonsocketable_armor() {
        let mut item = armor("buc", 10);
        let mut gem = Item::default();
        gem.code = "gcv".into();
        socket(&mut item, gem);
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(18))
        );
        item.code = "cap".into();
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(10))
        );
        item.code = "hbl".into();
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::BaseOnly {
                value: 10,
                reason: DefenseContextError::SocketHost
            })
        );
    }

    #[settled_contract_test]
    fn sparse_set_mask_uses_flag_ordinal_and_level_coefficients_merge_before_shift() {
        let mut item = armor("buc", 4);
        item.quality = Some(ItemQuality::Set);
        item.unique_id = Some(4);
        item.body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .set_property_flags = Some([false, true, false, false, false]);
        item.set_attributes = vec![vec![prop(214, 20)]];
        assert_eq!(
            displayed_defense(&item, 2, 26),
            Some(DefenseDisplay::Final(4))
        );
        assert_eq!(
            displayed_defense(&item, 3, 26),
            Some(DefenseDisplay::Final(69))
        );
        item.properties = vec![prop(214, 5), prop(214, 5)];
        assert_eq!(
            displayed_defense(&item, 0, 1),
            Some(DefenseDisplay::Final(5))
        );
        item.body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .set_property_flags = Some([true, true, false, false, false]);
        assert_eq!(
            displayed_defense(&item, 3, 26),
            Some(DefenseDisplay::BaseOnly {
                value: 4,
                reason: DefenseContextError::SetLayout
            })
        );
    }

    #[settled_contract_test]
    fn unidentified_concealed_and_legacy_defense_visibility_remains_unchanged() {
        let mut item = armor("msk", 42);
        item.is_identified = false;
        let mut hidden = ItemStats::default();
        hidden.properties = vec![prop(16, 29)];
        item.body.d2r_v3_auxiliary.as_mut().unwrap().concealed_stats = Some(Box::new(hidden));
        let original = item.clone();
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(42))
        );
        assert_eq!(
            format_item(&item, "en", 0, 26).base_attributes[0],
            "Defense: 42"
        );
        assert_eq!(item, original);
        item.is_identified = true;
        item.body.d2r_v3_auxiliary = None;
        item.properties = vec![prop(16, 29)];
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(42))
        );
    }

    #[settled_contract_test]
    fn stored_validation_checks_encoding_mirror_presence_and_layout_not_effective_value() {
        let mut item = armor("utp", 787);
        item.is_ethereal = true;
        item.quality = Some(ItemQuality::High);
        item.properties = vec![prop(16, 15)];
        assert!(stored_defense_warnings(&item).is_empty());
        assert_eq!(
            displayed_defense(&item, 0, 26),
            Some(DefenseDisplay::Final(905))
        );
        item.body.defense = Some(2037);
        item.defense = Some(2037);
        assert!(stored_defense_warnings(&item).is_empty());
        item.body.defense = Some(2038);
        item.defense = Some(2038);
        assert!(
            stored_defense_warnings(&item)
                .iter()
                .any(|w| w.category == "defense_encoding")
        );
        item.defense = Some(1);
        assert!(
            stored_defense_warnings(&item)
                .iter()
                .any(|w| w.category == "defense_mirror")
        );
        item.body.defense = None;
        assert!(
            stored_defense_warnings(&item)
                .iter()
                .any(|w| w.category == "defense_missing")
        );
        item.body.defense = Some(1);
        item.code = "rin".into();
        assert!(
            stored_defense_warnings(&item)
                .iter()
                .any(|w| w.category == "defense_layout")
        );
    }

    #[settled_contract_test]
    fn unqualified_context_never_masquerades_as_final_defense() {
        let mut item = armor("cap", 10);
        for (stat, reason) in [
            (215, DefenseContextError::UnqualifiedModifier),
            (268, DefenseContextError::WorldTime),
            (269, DefenseContextError::WorldTime),
        ] {
            item.properties = vec![prop(stat, 1)];
            assert_eq!(
                displayed_defense(&item, 0, 26),
                Some(DefenseDisplay::BaseOnly { value: 10, reason })
            );
            let formatted = format_item(&item, "en", 0, 26);
            assert_eq!(formatted.base_attributes[0], "Base Defense: 10");
            assert!(
                formatted
                    .detailed_warnings
                    .iter()
                    .any(|w| w.category == "defense_context")
            );
        }
        item.properties.clear();
        item.quality = Some(ItemQuality::Set);
        item.unique_id = Some(0);
        item.set_attributes = vec![vec![prop(31, 5)]];
        item.body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .set_property_flags = Some([true, false, false, false, false]);
        assert_eq!(
            displayed_defense(&item, 2, 26),
            Some(DefenseDisplay::BaseOnly {
                value: 10,
                reason: DefenseContextError::SetMembers
            })
        );
    }

    #[settled_contract_test]
    fn equipped_counts_use_distinct_identified_source_piece_ids() {
        let mut shield = armor("buc", 4);
        shield.quality = Some(ItemQuality::Set);
        shield.unique_id = Some(4);
        let duplicate = shield.clone();
        let mut belt = shield.clone();
        belt.unique_id = Some(3);
        let mut hidden = shield.clone();
        hidden.unique_id = Some(5);
        hidden.is_identified = false;
        let group = SET_ITEMS.iter().find(|s| s.id == 4).unwrap().set_id;
        let counts = equipped_set_piece_counts([&shield, &duplicate, &belt, &hidden]);
        assert_eq!(counts.get(group), Some(&2));
    }
}
