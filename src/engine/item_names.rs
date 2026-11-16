//! Item naming policy and quality labels.

use crate::data::affixes::{AFFIX_NAMES, PREFIXES, SUFFIXES};
use crate::data::item_codes::{ITEM_NAME_SOURCES, ITEM_TEMPLATES};
use crate::data::localization::LOCALIZATIONS;
use crate::data::rare_names::{RARE_PREFIXES, RARE_SUFFIXES};
use crate::data::set_items::SET_ITEMS;
use crate::data::unique_items::UNIQUE_ITEMS;
use crate::domain::item::runeword::{resolve_runeword, runeword_display_name};
use crate::engine::is_explicit_v105;
use crate::item::{Item, ItemQuality};
use crate::localization::{get_loc, get_loc_optional};

fn rare_name_key(raw_id: u8) -> Option<&'static str> {
    let index = usize::from(raw_id).checked_sub(1)?;
    // Rare names share a one-based pool with all suffix rows before prefix rows.
    let key = if index < RARE_SUFFIXES.len() {
        RARE_SUFFIXES.get(index)
    } else {
        RARE_PREFIXES.get(index - RARE_SUFFIXES.len())
    }?;
    (!key.is_empty()).then_some(*key)
}

fn magic_affix_name(item: &Item, kind: &str, id: u16) -> Option<&'static str> {
    let primary = match kind {
        "prefix" => PREFIXES,
        "suffix" => SUFFIXES,
        _ => return None,
    };
    primary
        .iter()
        .find(|a| a.id == u32::from(id))
        .map(|a| a.name)
        .or_else(|| {
            if is_explicit_v105(item)
                && item.is_identified
                && item.header.is_identified
                && item.is_completely_parsed()
            {
                AFFIX_NAMES
                    .iter()
                    .find(|a| a.kind == kind && a.id == u32::from(id))
                    .map(|a| a.name_key)
            } else {
                None
            }
        })
}

pub fn resolve_item_name(item: &Item, language: &str) -> String {
    // 1. Runeword
    if item.is_runeword || item.header.is_runeword {
        if let Some(resolved) = resolve_runeword(item) {
            let source_name = runeword_display_name(resolved.template);
            let localized = LOCALIZATIONS
                .iter()
                .find(|entry| entry.key == resolved.template.name && entry.en == source_name);
            return localized
                .map(|entry| if language == "ko" { entry.ko } else { entry.en })
                .unwrap_or(source_name)
                .to_string();
        }
        if let Some(raw_id) = item.runeword_id {
            return format!("Runeword{raw_id}");
        }
    }

    // 2. Unique
    if item.quality == Some(ItemQuality::Unique) && item.is_identified {
        if let Some(id) = item.unique_id {
            if let Some(unique) = UNIQUE_ITEMS.iter().find(|u| u.id == id as u32) {
                return get_loc(unique.index, language).to_string();
            }
        }
    }

    // 3. Set
    if item.quality == Some(ItemQuality::Set) && item.is_identified {
        if let Some(id) = item.unique_id {
            if let Some(set_item) = SET_ITEMS.iter().find(|s| s.id == id as u32) {
                return get_loc(set_item.index, language).to_string();
            }
        }
    }

    // 4. Rare / Crafted
    if (item.quality == Some(ItemQuality::Rare) || item.quality == Some(ItemQuality::Crafted))
        && item.is_identified
    {
        if let (Some(p), Some(s)) = (
            item.rare_name_1.and_then(rare_name_key),
            item.rare_name_2.and_then(rare_name_key),
        ) {
            let p_loc = get_loc(p, language);
            let s_loc = get_loc(s, language);
            return if language == "ko" {
                format!("{p_loc}{s_loc}")
            } else {
                format!("{p_loc} {s_loc}")
            };
        }
    }

    // 5. Magic
    if item.quality == Some(ItemQuality::Magic) && item.is_identified {
        let mut name = String::new();
        if let Some(id) = item.magic_prefix.filter(|&id| id != 0) {
            if let Some(name_key) = magic_affix_name(item, "prefix", id) {
                name.push_str(get_loc(name_key, language));
            }
        }

        let base_name = base_item_name(item, language);

        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(base_name);

        if let Some(id) = item.magic_suffix.filter(|&id| id != 0) {
            if let Some(name_key) = magic_affix_name(item, "suffix", id) {
                let s_loc = get_loc(name_key, language);
                if language != "ko" {
                    name.push(' ');
                }
                name.push_str(s_loc);
            }
        }
        return name;
    }

    let base = base_item_name(item, language);

    // 6. Superior (High) Quality
    if item.quality == Some(ItemQuality::High) {
        let prefix = get_loc("Hiquality", language);
        return format!("{} {}", prefix, base);
    }

    // 7. Low Quality
    if item.quality == Some(ItemQuality::Low) {
        let sub_idx = item.low_high_graphic_bits.unwrap_or(3) as usize;
        let prefix = crate::data::quality_items::LOW_QUALITY_SUBTYPES
            .get(sub_idx)
            .or_else(|| crate::data::quality_items::LOW_QUALITY_SUBTYPES.last())
            .map(|(en, ko)| if language == "ko" { *ko } else { *en })
            .unwrap_or("Low Quality");
        return format!("{} {}", prefix, base);
    }

    // Default: Base Name
    base.to_string()
}

pub fn quality_display_name(quality: Option<ItemQuality>, language: &str) -> String {
    let q_idx = match quality {
        Some(ItemQuality::Low) => 1,
        Some(ItemQuality::Normal) | None => 2,
        Some(ItemQuality::High) => 3,
        Some(ItemQuality::Magic) => 4,
        Some(ItemQuality::Set) => 5,
        Some(ItemQuality::Rare) => 6,
        Some(ItemQuality::Unique) => 7,
        Some(ItemQuality::Crafted) => 8,
    };
    if let Some(&(_, en, ko)) = crate::data::quality_items::QUALITY_DISPLAY_NAMES
        .iter()
        .find(|(idx, _, _)| *idx == q_idx)
    {
        return if language == "ko" {
            ko.to_string()
        } else {
            en.to_string()
        };
    }
    "Normal".to_string()
}

fn base_item_name<'a>(item: &'a Item, language: &str) -> &'a str {
    let Some(template) = ITEM_TEMPLATES.iter().find(|t| t.code == item.code.trim()) else {
        return &item.code;
    };
    ITEM_NAME_SOURCES
        .iter()
        .find(|source| source.code == template.code)
        .and_then(|source| get_loc_optional(source.name_key, language))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| get_loc(template.name, language))
}

#[cfg(test)]
mod zero_affix_sentinel_tests {
    use super::*;
    use crate::engine::formatter::format_item;
    use crate::item::HuffmanTree;
    use d2r_macros::{invariant_test, settled_contract_test};

    #[settled_contract_test]
    fn zero_is_absent_without_erasing_real_nonzero_affixes() {
        let mut item = Item::default();
        item.code = "rin".into();
        item.quality = Some(ItemQuality::Magic);
        item.is_identified = true;
        item.magic_prefix = Some(0);
        item.magic_suffix = Some(0);
        let original = item.clone();
        assert!(item.prefixes().is_empty());
        assert!(item.suffixes().is_empty());
        assert_eq!(resolve_item_name(&item, "en"), "Ring");
        assert_eq!(item, original);
        item.magic_suffix = Some(115);
        assert_eq!(item.suffixes()[0].id, 115);
        assert_eq!(resolve_item_name(&item, "en"), "Ring of Health");
        item.magic_suffix = None;
        item.magic_prefix = None;
        assert_eq!(resolve_item_name(&item, "en"), "Ring");
        item.quality = Some(ItemQuality::Rare);
        item.rare_affixes = [Some(0), Some(0), Some(213), Some(115), None, None];
        let original = item.clone();
        assert_eq!(
            item.prefixes().iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![213]
        );
        assert_eq!(
            item.suffixes().iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![115]
        );
        assert_eq!(item, original);
    }

    #[invariant_test]
    fn actual_names_and_raw_zero_jewelry_bits_preserve_ownership() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act1/TESTDRUID_PostWarriv_Act1.d2s"
        );
        let huffman = HuffmanTree::new();
        let items = Item::read_player_items_d2r_v3(bytes, &huffman).unwrap();
        assert_eq!(items.len(), 41);
        for (index, prefix, suffix, expected) in [
            (10, 213, 0, "Rugged Belt"),
            (19, 409, 0, "Beryl Ring"),
            (24, 0, 115, "Amulet of Health"),
            (12, 0, 174, "Ring of the Apprentice"),
        ] {
            let item = &items[index];
            let original = item.clone();
            assert_eq!(item.magic_prefix, Some(prefix));
            assert_eq!(item.magic_suffix, Some(suffix));
            assert_eq!(resolve_item_name(item, "en"), expected);
            let formatted = format_item(item, "en", 0, 40);
            assert!(!formatted.warnings.iter().any(|w| w.contains("(id:0)")));
            if suffix == 0 {
                assert!(item.suffixes().is_empty());
            } else {
                assert_eq!(item.suffixes()[0].id, u32::from(suffix));
            }
            assert_eq!(*item, original);
            if index != 10 {
                let expected: Vec<_> = (item.range.start..item.range.end)
                    .map(|p| bytes[p as usize / 8] & (1 << (p % 8)) != 0)
                    .collect();
                let mut clean = item.clone();
                clean.bits.clear();
                clean.segments.clear();
                clean.range = Default::default();
                assert_eq!(clean.to_bits_d2r_v3_jewelry(&huffman).unwrap(), expected);
            }
        }
    }
}

#[cfg(test)]
mod rare_name_identity_tests {
    use super::*;
    use crate::item::HuffmanTree;
    use d2r_macros::{invariant_test, settled_contract_test};

    #[settled_contract_test]
    fn pooled_ids_are_one_based_suffix_first_with_checked_bounds() {
        assert_eq!(rare_name_key(0), None);
        assert_eq!(rare_name_key(1), Some(RARE_SUFFIXES[0]));
        let suffix_count = u8::try_from(RARE_SUFFIXES.len()).unwrap();
        let total = u8::try_from(RARE_SUFFIXES.len() + RARE_PREFIXES.len()).unwrap();
        assert_eq!(rare_name_key(suffix_count), RARE_SUFFIXES.last().copied());
        assert_eq!(rare_name_key(suffix_count + 1), Some(RARE_PREFIXES[0]));
        assert_eq!(rare_name_key(total), RARE_PREFIXES.last().copied());
        assert_eq!(rare_name_key(total + 1), None);
        assert_eq!(rare_name_key(u8::MAX), None);
    }

    #[settled_contract_test]
    fn crafted_and_rare_names_preserve_hidden_and_invalid_id_policy() {
        let mut item = Item::default();
        item.code = "rin".into();
        item.rare_name_1 = Some(169);
        item.rare_name_2 = Some(94);
        for quality in [ItemQuality::Rare, ItemQuality::Crafted] {
            item.quality = Some(quality);
            item.is_identified = true;
            let original = item.clone();
            assert_eq!(resolve_item_name(&item, "en"), "Storm Grip");
            assert_eq!(resolve_item_name(&item, "ko"), "스톰그립");
            assert_eq!(item, original);
            item.is_identified = false;
            assert_eq!(resolve_item_name(&item, "en"), "Ring");
            item.is_identified = true;
            for (first, second) in [
                (Some(0), Some(94)),
                (Some(169), Some(255)),
                (None, Some(94)),
            ] {
                item.rare_name_1 = first;
                item.rare_name_2 = second;
                assert_eq!(resolve_item_name(&item, "en"), "Ring");
            }
            item.rare_name_1 = Some(169);
            item.rare_name_2 = Some(94);
        }
    }

    #[invariant_test]
    fn actual_rare_names_match_original_wire_ids_without_mutating_items() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act4/TESTDRUID_Act4_Enter.d2s"
        );
        let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
        assert_eq!(items.len(), 39);
        for (index, first, second, first_bit, en, ko) in [
            (5, 187, 68, 8622, "Dire Visor", "다이어바이저"),
            (6, 168, 110, 8943, "Shadow Slippers", "섀도우슬리퍼"),
            (12, 169, 94, 10260, "Storm Grip", "스톰그립"),
            (14, 172, 124, 10710, "Stone Eye", "스톤아이"),
            (37, 170, 24, 15096, "Rune Bane", "룬베인"),
        ] {
            let read_byte = |start: usize| {
                (0..8).fold(0u8, |value, shift| {
                    value | (((bytes[(start + shift) / 8] >> ((start + shift) % 8)) & 1) << shift)
                })
            };
            assert_eq!(read_byte(first_bit), first);
            assert_eq!(read_byte(first_bit + 8), second);
            let item = &items[index];
            let original = item.clone();
            assert!(item.is_identified);
            assert_eq!(item.quality, Some(ItemQuality::Rare));
            assert_eq!(
                (item.rare_name_1, item.rare_name_2),
                (Some(first), Some(second))
            );
            assert_eq!(resolve_item_name(item, "en"), en);
            assert_eq!(resolve_item_name(item, "ko"), ko);
            assert_eq!(*item, original);
        }
    }
}

#[cfg(test)]
mod magic_affix_name_source_tests {
    use super::*;
    use crate::domain::item::entity::D2rV3AuxiliaryData;
    use crate::item::HuffmanTree;
    use d2r_macros::settled_contract_test;

    fn magic() -> Item {
        let mut item = Item::empty_for_tests();
        item.code = "ktr ".into();
        item.body.code = item.code.clone();
        item.quality = Some(ItemQuality::Magic);
        item.header.quality = item.quality;
        item.is_identified = true;
        item.header.is_identified = true;
        item.header.save_is_alpha = false;
        item.properties_complete = true;
        item.magic_prefix = Some(0);
        item.magic_suffix = Some(195);
        item.body.d2r_v3_auxiliary = Some(D2rV3AuxiliaryData {
            save_version: 105,
            ..Default::default()
        });
        item
    }

    #[settled_contract_test]
    fn actual_magic_affix_name_source_matches_saved_id_and_preserves_parsed_damage() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s"
        );
        let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
        let snapshot = items.clone();
        let katar = &items[32];
        assert_eq!(katar.code.trim(), "ktr");
        assert_eq!(katar.magic_suffix, Some(195));
        assert!(!SUFFIXES.iter().any(|s| s.id == 195));
        assert!(
            katar
                .properties
                .iter()
                .any(|p| p.stat_id == 22 && p.param == 0 && p.value == 1)
        );
        let namespaced_source = AFFIX_NAMES
            .iter()
            .find(|s| s.kind == "suffix" && s.id == 195)
            .unwrap();
        assert_eq!(namespaced_source.name_key, "of Craftsmanship");
        assert_eq!(resolve_item_name(katar, "en"), "Katar of Craftmanship");
        assert_eq!(
            resolve_item_name(katar, "ko"),
            format!(
                "{}{}",
                get_loc("ktr", "ko"),
                get_loc("of Craftsmanship", "ko")
            )
        );
        assert_eq!(items, snapshot);
    }

    #[settled_contract_test]
    fn source_name_fallback_is_bounded_by_identification_and_explicit_complete_v3_context() {
        let original = magic();
        assert_eq!(resolve_item_name(&original, "en"), "Katar of Craftmanship");
        let mut item = original.clone();
        item.header.save_is_alpha = true;
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        item = original.clone();
        item.body.d2r_v3_auxiliary = None;
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        item = original.clone();
        item.is_identified = false;
        item.header.is_identified = false;
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        item = original.clone();
        item.header.is_identified = false;
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        item = original.clone();
        item.properties_complete = false;
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        item = original.clone();
        item.magic_suffix = Some(2047);
        assert_eq!(resolve_item_name(&item, "en"), "Katar");
        assert_eq!(original.magic_suffix, Some(195));
    }

    #[settled_contract_test]
    fn existing_primary_labels_and_affix_namespaces_keep_their_original_policy() {
        let mut item = magic();
        item.magic_suffix = Some(740);
        let expected = format!("Katar {}", get_loc("of Energy", "en"));
        assert_eq!(resolve_item_name(&item, "en"), expected);
        item.header.save_is_alpha = true;
        assert_eq!(resolve_item_name(&item, "en"), expected);
        item.header.save_is_alpha = false;
        item.magic_suffix = Some(0);
        item.magic_prefix = Some(195);
        assert_eq!(magic_affix_name(&item, "prefix", 195), Some("Cinnabar"));
        assert_eq!(
            magic_affix_name(&item, "suffix", 195),
            Some("of Craftsmanship")
        );
        assert_eq!(magic_affix_name(&item, "unknown", 195), None);
    }
}

#[cfg(test)]
mod base_item_name_source_tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    fn base(code: &str) -> Item {
        let mut item = Item::empty_for_tests();
        item.code = code.into();
        item.body.code = item.code.clone();
        item
    }

    #[settled_contract_test]
    fn known_base_codes_use_source_localization_without_identification_or_dialect_gates() {
        for code in ["ktr", "fhl"] {
            let mut item = base(code);
            let expected = get_loc(code, "ko");
            assert_eq!(resolve_item_name(&item, "ko"), expected);
            item.header.save_is_alpha = true;
            item.properties_complete = false;
            assert_eq!(resolve_item_name(&item, "ko"), expected);
            item.quality = Some(ItemQuality::Magic);
            item.is_identified = false;
            assert_eq!(resolve_item_name(&item, "ko"), expected);
            assert_eq!(resolve_item_name(&item, "en"), get_loc(code, "en"));
        }
    }

    #[settled_contract_test]
    fn unavailable_source_localization_keeps_template_and_unknown_code_fallbacks() {
        let source = ITEM_NAME_SOURCES
            .iter()
            .find(|s| get_loc_optional(s.name_key, "ko").is_none())
            .unwrap();
        let item = base(source.code);
        let template = ITEM_TEMPLATES
            .iter()
            .find(|t| t.code == source.code)
            .unwrap();
        assert_eq!(resolve_item_name(&item, "ko"), get_loc(template.name, "ko"));
        let item = base("unknown-code ");
        assert_eq!(resolve_item_name(&item, "ko"), "unknown-code ");
    }

    #[settled_contract_test]
    fn source_base_keys_do_not_override_named_quality_precedence() {
        let mut item = base("sbw");
        item.quality = Some(ItemQuality::Unique);
        item.is_identified = true;
        item.unique_id = Some(59);
        assert_eq!(resolve_item_name(&item, "ko"), get_loc("Pluckeye", "ko"));
        item.is_identified = false;
        assert_eq!(resolve_item_name(&item, "ko"), get_loc("sbw", "ko"));
        item = base("kit");
        item.quality = Some(ItemQuality::Set);
        item.is_identified = true;
        item.unique_id = Some(21);
        assert_eq!(
            resolve_item_name(&item, "ko"),
            get_loc("Milabrega's Orb", "ko")
        );
    }

    #[settled_contract_test]
    fn generated_source_key_lookup_preserves_models_across_the_complete_metadata_domain() {
        assert_eq!(ITEM_NAME_SOURCES.len(), 692);
        for source in ITEM_NAME_SOURCES {
            let item = base(source.code);
            let before = item.clone();
            for language in ["en", "ko"] {
                if let Some(expected) =
                    get_loc_optional(source.name_key, language).filter(|s| !s.trim().is_empty())
                {
                    assert_eq!(
                        resolve_item_name(&item, language),
                        expected,
                        "{}",
                        source.code
                    );
                }
            }
            assert_eq!(item, before);
        }
    }
}
