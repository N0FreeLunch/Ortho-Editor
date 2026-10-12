//! Transient armor socket totals for tooltips; never stored in the parsed item.

use crate::data::gems::GEMS;
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::item_specs::ItemStatRange;
use crate::data::stat_costs::STAT_COSTS;
use crate::engine::defense::is_explicit_v105;
use crate::engine::validation::get_all_item_types;
use crate::item::{Item, ItemBitRange, ItemProperty};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SocketContextError {
    ChildCount,
    ChildProperties,
    OpaqueProperties,
    SourceRange,
    Arithmetic,
}

impl SocketContextError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::ChildCount => "Socket effects require a consistent filled-socket count.",
            Self::ChildProperties => "A socketed item's effects cannot be composed reliably.",
            Self::OpaqueProperties => "Unresolved properties prevent composing socket effects.",
            Self::SourceRange => "Socket effects require fixed, unparameterized source values.",
            Self::Arithmetic => "The combined socket effects cannot be represented reliably.",
        }
    }
}

pub(crate) fn project(item: &Item) -> Result<Option<Vec<ItemProperty>>, SocketContextError> {
    if !is_explicit_v105(item) || !item.is_identified || item.socketed_items.is_empty() {
        return Ok(None);
    }
    if !ITEM_TEMPLATES
        .iter()
        .any(|t| t.code == item.code.trim() && t.is_armor)
    {
        return Ok(None);
    }
    let types = get_all_item_types(&item.code);
    let shield = types.contains(&"shld");
    if !shield && !types.contains(&"helm") && !types.contains(&"tors") {
        return Ok(None);
    }
    // Jewel and mixed families need their own qualified composition policy.
    let Some(gems) = item
        .socketed_items
        .iter()
        .map(|child| GEMS.iter().find(|gem| gem.code == child.code.trim()))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let filled = item
        .body
        .d2r_v3_auxiliary
        .as_ref()
        .map_or(0, |a| a.filled_sockets);
    if usize::from(filled) != item.socketed_items.len() {
        return Err(SocketContextError::ChildCount);
    }
    for child in &item.socketed_items {
        if !child.header.is_compact
            || !child.is_identified
            || !child.socketed_items.is_empty()
            || !child.runeword_attributes.is_empty()
            || child.set_attributes.iter().any(|list| !list.is_empty())
            || child
                .properties
                .iter()
                .any(|p| p.is_opaque || p.opaque_bits.is_some() || p.stat_id != 511)
        {
            return Err(SocketContextError::ChildProperties);
        }
    }
    let effects: Vec<_> = gems
        .into_iter()
        .flat_map(|gem| {
            if shield {
                gem.shield_stats
            } else {
                gem.helm_stats
            }
        })
        .copied()
        .collect();
    let mut original = item.properties.clone();
    if item.is_runeword {
        original.extend_from_slice(&item.runeword_attributes);
    }
    compose(&original, &effects).map(Some)
}

fn compose(
    original: &[ItemProperty],
    effects: &[ItemStatRange],
) -> Result<Vec<ItemProperty>, SocketContextError> {
    if original
        .iter()
        .any(|p| p.is_opaque || p.opaque_bits.is_some())
    {
        return Err(SocketContextError::OpaqueProperties);
    }
    if effects.iter().any(|s| s.min != s.max || s.param != 0) {
        return Err(SocketContextError::SourceRange);
    }
    let affected: HashSet<_> = effects.iter().map(|s| (s.stat_id, s.param)).collect();
    let mut positions = HashMap::new();
    let mut output: Vec<ItemProperty> = Vec::new();
    for property in original {
        let key = (property.stat_id, property.param);
        if !affected.contains(&key) {
            output.push(property.clone());
        } else if let Some(&index) = positions.get(&key) {
            let total: &mut ItemProperty = &mut output[index];
            total.value = total
                .value
                .checked_add(property.value)
                .ok_or(SocketContextError::Arithmetic)?;
            total.raw_value = total.value;
        } else {
            let mut total = property.clone();
            total.range = ItemBitRange::default();
            total.raw_value = total.value;
            positions.insert(key, output.len());
            output.push(total);
        }
    }
    for effect in effects {
        let key = (effect.stat_id, effect.param);
        if let Some(&index) = positions.get(&key) {
            let total = &mut output[index];
            total.value = total
                .value
                .checked_add(effect.min)
                .ok_or(SocketContextError::Arithmetic)?;
            total.raw_value = total.value;
        } else {
            positions.insert(key, output.len());
            let name = STAT_COSTS
                .iter()
                .find(|s| s.id == effect.stat_id)
                .map_or("", |s| s.name);
            output.push(ItemProperty::new(
                effect.stat_id,
                name.to_string(),
                effect.param,
                effect.min,
                effect.min,
                ItemBitRange::default(),
            ));
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::formatter::{format_item, format_properties_list};
    use crate::item::HuffmanTree;

    fn fixture(bytes: &[u8], code: &str) -> Item {
        let mut items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
        let index = items
            .iter()
            .position(|i| i.code.trim() == code && !i.socketed_items.is_empty())
            .unwrap();
        items.remove(index)
    }

    fn cure() -> Item {
        fixture(
            include_bytes!(
                "../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Cure_xrn.d2s"
            ),
            "xrn",
        )
    }

    fn value(properties: &[ItemProperty], id: u32) -> i32 {
        let matches: Vec<_> = properties
            .iter()
            .filter(|p| p.stat_id == id && p.param == 0)
            .collect();
        assert_eq!(matches.len(), 1);
        matches[0].value
    }

    fn property(id: u32, param: u32, value: i32) -> ItemProperty {
        ItemProperty::new(
            id,
            "control".into(),
            param,
            value,
            value,
            ItemBitRange { start: 8, end: 20 },
        )
    }

    fn tal_host(code: &str) -> Item {
        let mut item = cure();
        let tal = item
            .socketed_items
            .iter()
            .find(|c| c.code.trim() == "r07")
            .unwrap()
            .clone();
        item.code = code.into();
        item.socketed_items = vec![tal];
        item.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 1;
        item.properties.clear();
        item.runeword_attributes.clear();
        item.is_runeword = false;
        item
    }

    #[d2r_macros::settled_contract_test]
    fn actual_cure_totals_and_immutability() {
        let item = cure();
        let before = item.clone();
        assert_eq!(value(&item.runeword_attributes, 45), 15);
        let projected = project(&item).unwrap().unwrap();
        for (id, expected) in [(45, 45), (99, 20), (3, 10)] {
            assert_eq!(value(&projected, id), expected);
            assert_eq!(
                projected.iter().find(|p| p.stat_id == id).unwrap().range,
                ItemBitRange::default()
            );
        }
        for language in ["en", "ko"] {
            let displayed = format_item(&item, language, 0, 99);
            assert_eq!(
                displayed.properties,
                format_properties_list(&projected, 99, language)
            );
            assert!(
                !displayed
                    .detailed_warnings
                    .iter()
                    .any(|w| w.category == "socket_context")
            );
        }
        assert_eq!(item, before);
    }

    #[d2r_macros::settled_contract_test]
    fn actual_treachery_totals() {
        let item = fixture(
            include_bytes!(
                "../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Treachery_uea.d2s"
            ),
            "uea",
        );
        let before = item.clone();
        let projected = project(&item).unwrap().unwrap();
        assert_eq!(value(&projected, 99), 20);
        assert_eq!(value(&projected, 43), 30);
        assert_eq!(value(&projected, 79), 50);
        assert_eq!(item, before);
    }

    #[d2r_macros::settled_contract_test]
    fn actual_sapphire_mana_and_base() {
        let item = fixture(
            include_bytes!(
                "../../tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest2_BarbariansRescued.d2s"
            ),
            "brs",
        );
        let before = item.clone();
        let projected = project(&item).unwrap().unwrap();
        assert_eq!(value(&projected, 9), 38);
        let mut unsocketed = item.clone();
        unsocketed.socketed_items.clear();
        unsocketed
            .body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .filled_sockets = 0;
        assert_eq!(
            format_item(&item, "en", 0, 99).base_attributes,
            format_item(&unsocketed, "en", 0, 99).base_attributes
        );
        assert_eq!(item, before);
    }

    #[d2r_macros::settled_contract_test]
    fn host_selection_and_weapon_exclusion() {
        for (code, expected) in [("cap", 30), ("brs", 30), ("buc", 35)] {
            assert_eq!(
                value(&project(&tal_host(code)).unwrap().unwrap(), 45),
                expected
            );
        }
        assert_eq!(project(&tal_host("ssd")), Ok(None));
    }

    #[d2r_macros::settled_contract_test]
    fn repeated_children_root_runeword_and_distinct_params() {
        let mut item = tal_host("cap");
        item.socketed_items.push(item.socketed_items[0].clone());
        item.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 2;
        let distinct = property(45, 1, 7);
        item.properties = vec![property(45, 0, 2), distinct.clone(), property(80, 0, 5)];
        item.is_runeword = true;
        item.runeword_attributes = vec![property(45, 0, 3)];
        let projected = project(&item).unwrap().unwrap();
        assert_eq!(value(&projected, 45), 65);
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[1], distinct);
        assert_eq!(projected[2], item.properties[2]);
    }

    #[d2r_macros::settled_contract_test]
    fn malformed_context_and_advisory_fallback() {
        let mut bad = cure();
        bad.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 1;
        assert_eq!(project(&bad), Err(SocketContextError::ChildCount));
        let mut original = format_properties_list(&bad.properties, 99, "en");
        original.extend(format_properties_list(&bad.runeword_attributes, 99, "en"));
        let display = format_item(&bad, "en", 0, 99);
        assert_eq!(display.properties, original);
        let warnings: Vec<_> = display
            .detailed_warnings
            .iter()
            .filter(|w| w.category == "socket_context")
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].tier,
            crate::engine::validation::WarningConfidenceTier::Advisory
        );
        let mut bad = cure();
        bad.socketed_items[0].header.is_compact = false;
        assert_eq!(project(&bad), Err(SocketContextError::ChildProperties));
        let mut bad = cure();
        let nested = bad.socketed_items[1].clone();
        bad.socketed_items[0].socketed_items.push(nested);
        assert_eq!(project(&bad), Err(SocketContextError::ChildProperties));
        let mut bad = cure();
        bad.socketed_items[0].properties.push(property(99, 0, 20));
        assert_eq!(project(&bad), Err(SocketContextError::ChildProperties));
        let mut bad = cure();
        bad.runeword_attributes[0].is_opaque = true;
        assert_eq!(project(&bad), Err(SocketContextError::OpaqueProperties));
    }

    #[d2r_macros::settled_contract_test]
    fn checked_arithmetic_and_fixed_source_contract() {
        let effect = ItemStatRange {
            stat_id: 45,
            param: 0,
            min: 30,
            max: 30,
        };
        assert_eq!(
            compose(&[property(45, 0, i32::MAX)], &[effect]),
            Err(SocketContextError::Arithmetic)
        );
        assert_eq!(
            compose(&[], &[ItemStatRange { max: 31, ..effect }]),
            Err(SocketContextError::SourceRange)
        );
        assert_eq!(
            compose(&[], &[ItemStatRange { param: 1, ..effect }]),
            Err(SocketContextError::SourceRange)
        );
        assert_eq!(
            compose(&[property(45, 0, i32::MAX), property(45, 0, 1)], &[effect]),
            Err(SocketContextError::Arithmetic)
        );
    }

    #[d2r_macros::settled_contract_test]
    fn unqualified_families_are_unchanged() {
        let mut item = cure();
        item.header.save_is_alpha = true;
        assert_eq!(project(&item), Ok(None));
        item = cure();
        item.body.d2r_v3_auxiliary.as_mut().unwrap().save_version = 104;
        assert_eq!(project(&item), Ok(None));
        item = cure();
        item.is_identified = false;
        assert_eq!(project(&item), Ok(None));
        item = cure();
        item.socketed_items[0].code = "jew".into();
        assert_eq!(project(&item), Ok(None));
        item.socketed_items.clear();
        assert_eq!(project(&item), Ok(None));
    }

    #[d2r_macros::settled_contract_test]
    fn defense_real_317_and_unaffected_fields_preserved() {
        let mut item = cure();
        let real = property(317, 0, 8);
        item.properties.insert(0, real.clone());
        let before = item.clone();
        let defense = crate::engine::defense::displayed_defense(&item, 0, 99);
        let projected = project(&item).unwrap().unwrap();
        assert_eq!(projected[0], real);
        let affected = [45, 99, 3];
        let original: Vec<_> = item
            .properties
            .iter()
            .chain(&item.runeword_attributes)
            .filter(|p| !affected.contains(&p.stat_id))
            .cloned()
            .collect();
        assert_eq!(
            projected
                .iter()
                .filter(|p| !affected.contains(&p.stat_id))
                .cloned()
                .collect::<Vec<_>>(),
            original
        );
        for language in ["en", "ko"] {
            let display = format_item(&item, language, 0, 99);
            assert!(
                display
                    .properties
                    .contains(&format_properties_list(&[real.clone()], 99, language)[0])
            );
        }
        assert_eq!(
            crate::engine::defense::displayed_defense(&item, 0, 99),
            defense
        );
        assert_eq!(item, before);
    }
}
