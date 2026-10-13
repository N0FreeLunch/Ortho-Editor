//! Transient socket totals for tooltips; never stored or serialized as item fields.
//! Poison duration is an item projection, not a character or skill damage aggregate.

use crate::data::gems::GEMS;
use crate::data::item_codes::ITEM_TEMPLATES;
use crate::data::item_specs::ItemStatRange;
use crate::data::stat_costs::STAT_COSTS;
use crate::domain::stats::entity::StatMaturityTier;
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
    DamageChannels,
    PoisonSources,
    Arithmetic,
}

impl SocketContextError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::ChildCount => "Socket effects require a consistent filled-socket count.",
            Self::ChildProperties => "A socketed item's effects cannot be composed reliably.",
            Self::OpaqueProperties => "Unresolved properties prevent composing socket effects.",
            Self::SourceRange => "Socket effects require fixed, unparameterized source values.",
            Self::DamageChannels => {
                "Socket damage channels do not identify one reliable source bonus."
            }
            Self::PoisonSources => {
                "Socket poison effects require complete, consistent source groups."
            }
            Self::Arithmetic => "The combined socket effects cannot be represented reliably.",
        }
    }
}

fn virtual_property(id: u32, value: i32) -> ItemProperty {
    let name = STAT_COSTS
        .iter()
        .find(|s| s.id == id)
        .map_or("", |s| s.name);
    ItemProperty::new(id, name.into(), 0, value, value, ItemBitRange::default())
}

fn saved_scalar(p: &ItemProperty) -> bool {
    !p.is_opaque
        && p.opaque_bits.is_none()
        && p.maturity == StatMaturityTier::Verified
        && p.param == 0
        && STAT_COSTS
            .iter()
            .find(|s| s.id == p.stat_id)
            .is_some_and(|s| {
                (1..=31).contains(&s.save_bits)
                    && p.raw_value >= 0
                    && i64::from(p.raw_value) < (1_i64 << s.save_bits)
                    && p.raw_value.checked_sub(s.save_add) == Some(p.value)
            })
}

// Each rolled bonus may be copied into multiple physical attack channels.
fn normalize_damage(
    list: &[ItemProperty],
    active: [bool; 2],
    jewel: bool,
) -> Result<Vec<ItemProperty>, SocketContextError> {
    let families = [[21, 23, 159], [22, 24, 160]];
    let mut skip = HashSet::new();
    let mut replacements = HashMap::new();
    for (family, enabled) in families.iter().zip(active) {
        if !enabled {
            continue;
        }
        let members: Vec<_> = list
            .iter()
            .enumerate()
            .filter(|(_, p)| family.contains(&p.stat_id))
            .collect();
        if members.is_empty() {
            continue;
        }
        let ids: HashSet<_> = members.iter().map(|(_, p)| p.stat_id).collect();
        let value = members[0].1.value;
        if members.len() > 3
            || ids.len() != members.len()
            || (jewel && members.len() != 3)
            || value < 0
            || members
                .iter()
                .any(|(_, p)| !saved_scalar(p) || p.raw_value != value || p.value != value)
        {
            return Err(SocketContextError::DamageChannels);
        }
        replacements.insert(members[0].0, virtual_property(family[0], value));
        skip.extend(members.iter().skip(1).map(|(i, _)| *i));
    }
    Ok(list
        .iter()
        .enumerate()
        .filter(|(i, _)| !skip.contains(i))
        .map(|(i, p)| replacements.get(&i).unwrap_or(p).clone())
        .collect())
}

fn poison_source(list: &[ItemProperty]) -> Result<bool, SocketContextError> {
    if list.iter().any(|p| p.stat_id == 326) {
        return Err(SocketContextError::PoisonSources);
    }
    let group: Vec<_> = list
        .iter()
        .enumerate()
        .filter(|(_, p)| (57..=59).contains(&p.stat_id))
        .collect();
    if group.is_empty() {
        return Ok(false);
    }
    if group.len() != 3
        || group.iter().enumerate().any(|(n, (i, p))| {
            p.stat_id != 57 + n as u32 || *i != group[0].0 + n || !saved_scalar(p)
        })
        || group[0].1.value < 0
        || group[1].1.value < group[0].1.value
        || group[1].1.value <= 0
        || group[2].1.value <= 0
    {
        return Err(SocketContextError::PoisonSources);
    }
    Ok(true)
}

pub(crate) fn project(item: &Item) -> Result<Option<Vec<ItemProperty>>, SocketContextError> {
    if !is_explicit_v105(item) || !item.is_identified || item.socketed_items.is_empty() {
        return Ok(None);
    }
    let Some(template) = ITEM_TEMPLATES.iter().find(|t| t.code == item.code.trim()) else {
        return Ok(None);
    };
    let types = get_all_item_types(&item.code);
    let shield = types.contains(&"shld");
    if !template.is_weapon
        && (!template.is_armor || (!shield && !types.contains(&"helm") && !types.contains(&"tors")))
    {
        return Ok(None);
    }
    if item.socketed_items.iter().any(|c| {
        !GEMS.iter().any(|g| g.code == c.code.trim())
            && (c.code.trim() != "jew" || c.header.is_compact)
    }) {
        return Ok(None);
    }
    let filled = item
        .body
        .d2r_v3_auxiliary
        .as_ref()
        .map_or(0, |a| a.filled_sockets);
    if usize::from(filled) != item.socketed_items.len() {
        return Err(SocketContextError::ChildCount);
    }
    if !item.properties_complete {
        return Err(SocketContextError::ChildProperties);
    }
    if item
        .properties
        .iter()
        .chain(if item.is_runeword {
            item.runeword_attributes.iter()
        } else {
            [].iter()
        })
        .any(|p| p.is_opaque || p.opaque_bits.is_some())
    {
        return Err(SocketContextError::OpaqueProperties);
    }
    let mut contributors = Vec::new();
    for child in &item.socketed_items {
        if child.header.save_is_alpha
            || !child.is_identified
            || child.is_runeword
            || !child.socketed_items.is_empty()
            || !child.runeword_attributes.is_empty()
            || child.set_attributes.iter().any(|list| !list.is_empty())
        {
            return Err(SocketContextError::ChildProperties);
        }
        if let Some(gem) = GEMS.iter().find(|g| g.code == child.code.trim()) {
            if !child.header.is_compact
                || child
                    .properties
                    .iter()
                    .any(|p| p.is_opaque || p.opaque_bits.is_some() || p.stat_id != 511)
            {
                return Err(SocketContextError::ChildProperties);
            }
            let ranges = if template.is_weapon {
                gem.weapon_stats
            } else if shield {
                gem.shield_stats
            } else {
                gem.helm_stats
            };
            if ranges.iter().any(|r| r.min != r.max || r.param != 0) {
                return Err(SocketContextError::SourceRange);
            }
            contributors.push(
                ranges
                    .iter()
                    .map(|r| virtual_property(r.stat_id, r.min))
                    .collect::<Vec<_>>(),
            );
        } else {
            if !is_explicit_v105(child)
                || !child.properties_complete
                || child
                    .properties
                    .iter()
                    .any(|p| p.is_opaque || p.opaque_bits.is_some())
            {
                return Err(SocketContextError::ChildProperties);
            }
            let properties: Vec<_> = child
                .properties
                .iter()
                .filter(|p| p.stat_id != 511)
                .cloned()
                .collect();
            let ids: HashSet<_> = properties.iter().map(|p| (p.stat_id, p.param)).collect();
            if ids.len() != properties.len() || properties.iter().any(|p| !saved_scalar(p)) {
                return Err(SocketContextError::ChildProperties);
            }
            contributors.push(normalize_damage(&properties, [true, true], true)?);
        }
    }
    let poison = contributors
        .iter()
        .flatten()
        .any(|p| (57..=59).contains(&p.stat_id));
    let effects: Vec<_> = contributors
        .iter()
        .flatten()
        .filter(|p| p.value != 0 || (poison && (57..=59).contains(&p.stat_id)))
        .map(|p| ItemStatRange {
            stat_id: p.stat_id,
            param: p.param,
            min: p.value,
            max: p.value,
        })
        .collect();
    let active = [
        effects.iter().any(|p| p.stat_id == 21),
        effects.iter().any(|p| p.stat_id == 22),
    ];
    let root = normalize_damage(&item.properties, active, false)?;
    let runeword = if item.is_runeword {
        normalize_damage(&item.runeword_attributes, active, false)?
    } else {
        Vec::new()
    };
    let mut count = 0_i32;
    if poison {
        if poison_source(&item.properties)? {
            count += 1;
        }
        if item.is_runeword && poison_source(&item.runeword_attributes)? {
            count += 1;
        }
        for (source, child) in contributors.iter().zip(&item.socketed_items) {
            // Table values have already been converted to logical virtual fields.
            let has_poison = if child.header.is_compact {
                let restored: Vec<_> = source
                    .iter()
                    .map(|p| {
                        let mut p = p.clone();
                        if let Some(cost) = STAT_COSTS.iter().find(|s| s.id == p.stat_id) {
                            p.raw_value = p.value.checked_add(cost.save_add).unwrap_or(i32::MIN);
                        }
                        p
                    })
                    .collect();
                poison_source(&restored)?
            } else {
                poison_source(&child.properties)?
            };
            if has_poison {
                count = count.checked_add(1).ok_or(SocketContextError::Arithmetic)?;
            }
        }
    }
    let affected: HashSet<_> = effects.iter().map(|p| (p.stat_id, p.param)).collect();
    // Validate original encoded scalars before their provenance is replaced by a virtual total.
    for p in item.properties.iter().chain(if item.is_runeword {
        item.runeword_attributes.iter()
    } else {
        [].iter()
    }) {
        if affected.contains(&(p.stat_id, p.param)) && !saved_scalar(p) {
            return Err(SocketContextError::SourceRange);
        }
    }
    let mut original = root;
    original.extend(runeword);
    let mut output = compose(&original, &effects)?;
    if poison {
        if count <= 0 {
            return Err(SocketContextError::PoisonSources);
        }
        let length = output
            .iter_mut()
            .find(|p| p.stat_id == 59 && p.param == 0)
            .ok_or(SocketContextError::PoisonSources)?;
        length.value /= count;
        length.raw_value = length.value;
    }
    Ok(Some(output))
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
    output
        .retain(|p| !(affected.contains(&(p.stat_id, p.param)) && p.value == 0 && p.stat_id != 57));
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
    fn host_selection_includes_weapon_palette() {
        for (code, expected) in [("cap", 30), ("brs", 30), ("buc", 35)] {
            assert_eq!(
                value(&project(&tal_host(code)).unwrap().unwrap(), 45),
                expected
            );
        }
        assert_eq!(value(&project(&tal_host("ssd")).unwrap().unwrap(), 57), 154);
    }

    #[d2r_macros::settled_contract_test]
    fn repeated_children_root_runeword_and_distinct_params() {
        let mut item = tal_host("cap");
        item.socketed_items.push(item.socketed_items[0].clone());
        item.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 2;
        let distinct = property(45, 1, 7);
        item.properties = vec![source_property(45, 2), distinct.clone(), property(80, 0, 5)];
        item.is_runeword = true;
        item.runeword_attributes = vec![source_property(45, 3)];
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
    fn cold_items() -> &'static Vec<Item> {
        static ITEMS: std::sync::OnceLock<Vec<Item>> = std::sync::OnceLock::new();
        ITEMS.get_or_init(|| Item::read_player_items_d2r_v3(include_bytes!("../../tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s"), &HuffmanTree::new()).unwrap())
    }

    fn cold_parent(code: &str, children: &[&str]) -> Item {
        cold_items()
            .iter()
            .find(|i| {
                i.code.trim() == code
                    && i.socketed_items
                        .iter()
                        .map(|c| c.code.trim())
                        .collect::<Vec<_>>()
                        == children
            })
            .unwrap()
            .clone()
    }

    fn source_property(id: u32, value: i32) -> ItemProperty {
        let mut p = property(id, 0, value);
        p.raw_value = value + STAT_COSTS.iter().find(|s| s.id == id).unwrap().save_add;
        p
    }

    fn host_with(children: Vec<Item>, code: &str) -> Item {
        let mut i = tal_host(code);
        i.socketed_items = children;
        i.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = i.socketed_items.len() as u8;
        i
    }

    fn gem(code: &str) -> Item {
        let mut i = tal_host("cap").socketed_items.remove(0);
        i.code = code.into();
        i
    }

    #[d2r_macros::settled_contract_test]
    fn actual_hustle_uses_logical_runeword_and_weapon_sources() {
        let i = fixture(
            include_bytes!(
                "../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Hustle_72h.d2s"
            ),
            "72h",
        );
        let before = i.clone();
        let rw = i
            .runeword_attributes
            .iter()
            .find(|p| p.stat_id == 93)
            .unwrap();
        assert_eq!((rw.raw_value, rw.value), (30, 10));
        let p = project(&i).unwrap().unwrap();
        for (id, v) in [(93, 30), (2, 10), (124, 50), (122, 75)] {
            assert_eq!(value(&p, id), v);
        }
        assert!(
            format_item(&i, "en", 0, 99)
                .properties
                .contains(&"+30% Increased Attack Speed".into())
        );
        assert_eq!(i, before);
    }

    #[d2r_macros::settled_contract_test]
    fn actual_weapon_damage_and_compact_palette() {
        let i = cold_parent("ces", &["r06", "r06"]);
        let before = i.clone();
        assert_eq!(value(&project(&i).unwrap().unwrap(), 22), 18);
        assert_eq!(
            format_item(&i, "en", 0, 99).properties,
            vec!["+18 to Maximum Damage"]
        );
        assert_eq!(i, before);
        let cold = host_with(vec![gem("gfb")], "ktr");
        let p = project(&cold).unwrap().unwrap();
        for (id, v) in [(54, 3), (55, 5), (56, 35)] {
            assert_eq!(value(&p, id), v);
        }
        assert_eq!(
            format_item(&cold, "en", 0, 99).properties,
            vec!["Adds 3-5 cold damage (1 sec duration)"]
        );
        let strength = host_with(vec![gem("r11"), gem("r03")], "ces");
        let p = project(&strength).unwrap().unwrap();
        assert_eq!(value(&p, 60), 7);
        assert_eq!(value(&p, 138), 2);
    }

    #[d2r_macros::settled_contract_test]
    fn actual_jewel_sources_transfer_once_without_mutation() {
        let i = cold_parent("cap", &["jew", "jew"]);
        let before = i.clone();
        let p = project(&i).unwrap().unwrap();
        for (id, v) in [(21, 3), (45, 13), (57, 103), (58, 103), (59, 50)] {
            assert_eq!(value(&p, id), v);
        }
        assert!(!p.iter().any(|p| matches!(p.stat_id, 23 | 159)));
        assert_eq!(
            format_item(&i, "en", 0, 99).properties,
            vec![
                "+3 to Minimum Damage",
                "Poison Resist +13%",
                "+20 poison damage over 2 seconds"
            ]
        );
        assert_eq!(i, before);
    }

    #[d2r_macros::settled_contract_test]
    fn actual_malice_cancels_only_affected_zero_light() {
        let mut i = cold_parent("spr", &["r06", "r01", "r05"]);
        let untouched = property(317, 0, 0);
        i.properties.insert(0, untouched.clone());
        let before = i.clone();
        let light = i
            .runeword_attributes
            .iter()
            .find(|p| p.stat_id == 89)
            .unwrap();
        assert_eq!((light.raw_value, light.value), (3, -1));
        let p = project(&i).unwrap().unwrap();
        assert!(!p.iter().any(|p| p.stat_id == 89));
        assert_eq!(p[0], untouched);
        assert_eq!(value(&p, 22), 9);
        assert_eq!(value(&p, 19), 50);
        assert_eq!(value(&p, 116), 25);
        assert_eq!(i, before);
    }

    #[d2r_macros::settled_contract_test]
    fn rolled_damage_is_canonical_per_source_and_rejects_ambiguity() {
        let mut jewel = cold_parent("cap", &["jew", "jew"]).socketed_items.remove(0);
        jewel.properties = [22, 24, 160].map(|id| source_property(id, 9)).to_vec();
        let mut i = host_with(vec![jewel, gem("r12")], "ces");
        i.properties = vec![source_property(23, 2), source_property(22, 1)];
        let before = i.clone();
        let p = project(&i).unwrap().unwrap();
        assert_eq!(value(&p, 21), 11);
        assert_eq!(value(&p, 22), 10);
        assert!(!p.iter().any(|p| matches!(p.stat_id, 23 | 24 | 159 | 160)));
        assert_eq!(i, before);
        for change in 0..4 {
            let mut bad = i.clone();
            match change {
                0 => {
                    bad.socketed_items[0].properties.pop();
                }
                1 => {
                    bad.socketed_items[0].properties[1].value = 8;
                    bad.socketed_items[0].properties[1].raw_value = 8;
                }
                2 => {
                    let p = bad.properties[1].clone();
                    bad.properties.push(p);
                }
                _ => {
                    bad.properties.push(source_property(159, 3));
                }
            }
            assert_eq!(project(&bad), Err(SocketContextError::DamageChannels));
        }
    }

    #[d2r_macros::settled_contract_test]
    fn unsafe_jewel_and_affected_parent_sources_use_advisory_fallback() {
        let i = cold_parent("cap", &["jew", "jew"]);
        for change in 0..8 {
            let mut bad = i.clone();
            match change {
                0 => bad.socketed_items[0].properties_complete = false,
                1 => bad.socketed_items[0].properties[0].is_opaque = true,
                2 => bad.socketed_items[0].properties[0].param = 1,
                3 => bad.socketed_items[0].properties[0].maturity = StatMaturityTier::Inferred,
                4 => bad.socketed_items[0].properties[0].raw_value += 1,
                5 => bad.socketed_items[0].is_identified = false,
                6 => bad.properties_complete = false,
                _ => {
                    let mut p = source_property(45, 4);
                    p.raw_value += 1;
                    bad.properties.push(p);
                }
            }
            let before = bad.clone();
            assert!(project(&bad).is_err());
            let f = format_item(&bad, "en", 0, 99);
            assert_eq!(
                f.detailed_warnings
                    .iter()
                    .filter(|w| w.category == "socket_context")
                    .count(),
                1
            );
            assert_eq!(bad, before);
        }
    }

    #[d2r_macros::settled_contract_test]
    fn poison_sources_average_duration_and_keep_zero_minimum_anchor() {
        let envy = cold_parent("cap", &["jew", "jew"]).socketed_items.remove(1);
        let equal = host_with(vec![envy.clone(), envy.clone()], "ces");
        let p = project(&equal).unwrap().unwrap();
        assert_eq!(value(&p, 57), 206);
        assert_eq!(value(&p, 59), 50);
        assert_eq!(
            format_item(&equal, "en", 0, 99).properties,
            vec!["+40 poison damage over 2 seconds"]
        );
        let mixed = host_with(vec![envy.clone(), gem("gcg")], "ces");
        let p = project(&mixed).unwrap().unwrap();
        assert_eq!(value(&p, 57), 137);
        assert_eq!(value(&p, 59), 62);
        assert_eq!(
            format_item(&mixed, "en", 0, 99).properties,
            vec!["+33 poison damage over 2 seconds"]
        );
        let mut zero = host_with(vec![envy.clone()], "ces");
        zero.socketed_items[0].properties[0] = source_property(57, 0);
        assert_eq!(value(&project(&zero).unwrap().unwrap(), 57), 0);
        for change in 0..7 {
            let mut bad = host_with(vec![envy.clone()], "ces");
            let props = &mut bad.socketed_items[0].properties;
            match change {
                0 => {
                    props.remove(1);
                }
                1 => props.swap(0, 1),
                2 => props[0] = source_property(57, 104),
                3 => props[1] = source_property(58, 0),
                4 => props[2] = source_property(59, 0),
                5 => {
                    let p = props[0].clone();
                    props.insert(1, p);
                }
                _ => {
                    bad.properties = vec![property(326, 0, 1)];
                }
            }
            assert!(project(&bad).is_err());
        }
        let mut merged = host_with(vec![envy.clone()], "ces");
        merged.properties = [(57, 20), (58, 20), (59, 100)]
            .map(|(id, v)| source_property(id, v))
            .to_vec();
        merged.is_runeword = true;
        merged.runeword_attributes = [(57, 30), (58, 30), (59, 25)]
            .map(|(id, v)| source_property(id, v))
            .to_vec();
        let snapshot = merged.clone();
        let p = project(&merged).unwrap().unwrap();
        assert_eq!(value(&p, 57), 153);
        assert_eq!(value(&p, 59), 58);
        assert_eq!(merged, snapshot);
        let minimum = cold_parent("cap", &["jew", "jew"]).socketed_items.remove(0);
        let mut gap = host_with(vec![envy, minimum], "ces");
        gap.properties = [(21, 2), (57, 1), (23, 2), (58, 2), (59, 50), (159, 2)]
            .map(|(id, v)| source_property(id, v))
            .to_vec();
        assert_eq!(project(&gap), Err(SocketContextError::PoisonSources));
    }

    #[d2r_macros::settled_contract_test]
    fn mixed_contexts_preserve_bases_and_unaffected_source_fields() {
        let jewel = cold_parent("cap", &["jew", "jew"]).socketed_items.remove(0);
        let mut i = host_with(vec![jewel, gem("r12")], "ces");
        i.properties = vec![source_property(21, 2), property(317, 0, 8)];
        i.is_runeword = true;
        i.runeword_attributes = vec![source_property(21, 4)];
        let before = i.clone();
        assert_eq!(value(&project(&i).unwrap().unwrap(), 21), 18);
        let mut plain = i.clone();
        plain.socketed_items.clear();
        plain.body.d2r_v3_auxiliary.as_mut().unwrap().filled_sockets = 0;
        for lang in ["en", "ko"] {
            assert_eq!(
                format_item(&i, lang, 0, 99).base_attributes,
                format_item(&plain, lang, 0, 99).base_attributes
            );
        }
        assert_eq!(i, before);
        let mut inactive = i.clone();
        inactive.is_runeword = false;
        inactive.runeword_attributes[0].is_opaque = true;
        assert_eq!(value(&project(&inactive).unwrap().unwrap(), 21), 14);
        let mut old_child = i.clone();
        old_child.socketed_items[0]
            .body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .save_version = 104;
        assert_eq!(
            project(&old_child),
            Err(SocketContextError::ChildProperties)
        );
        i.header.save_is_alpha = true;
        assert_eq!(project(&i), Ok(None));
        i = before.clone();
        i.is_identified = false;
        assert_eq!(project(&i), Ok(None));
        i = before.clone();
        i.body.d2r_v3_auxiliary.as_mut().unwrap().save_version = 104;
        assert_eq!(project(&i), Ok(None));
        i = before;
        i.socketed_items[0].code = "unknown".into();
        assert_eq!(project(&i), Ok(None));
    }
}
