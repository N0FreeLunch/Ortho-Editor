//! Item validation and compatible public lookup paths.
//!
//! ```
//! use d2r_core::engine::validation::get_all_item_types;
//!
//! let types: Vec<&'static str> = get_all_item_types(" buc ");
//! assert_eq!(types, ["shie", "shld", "armo", "seco"]);
//! assert!(get_all_item_types("unknown-code").is_empty());
//!
//! use d2r_core::data::item_specs::Runeword;
//! use d2r_core::data::runewords::RUNEWORDS;
//! use d2r_core::engine::validation::{
//!     resolve_runeword, runeword_display_name, ResolvedRuneword, RunewordMatchSource,
//! };
//! use d2r_core::item::Item;
//!
//! let resolver: fn(&Item) -> Option<ResolvedRuneword> = resolve_runeword;
//! let naming: fn(&'static Runeword) -> &'static str = runeword_display_name;
//! let sample = ResolvedRuneword {
//!     template: &RUNEWORDS[0],
//!     source: RunewordMatchSource::SavedId,
//! };
//! assert_eq!(sample.source, RunewordMatchSource::SavedId);
//! assert!(!naming(sample.template).is_empty());
//! let _ = RunewordMatchSource::OrderedSocketRecipe;
//! assert!(resolver(&Item::default()).is_none());
//! ```

use crate::item::{Item, ItemQuality};
#[cfg(test)]
use crate::item::{ItemBitRange, ItemProperty};
pub use crate::domain::item::type_hierarchy::get_all_item_types;
pub use crate::domain::item::runeword::{
    ResolvedRuneword, RunewordMatchSource, resolve_runeword, runeword_display_name,
};

mod base_item_policy;
pub use base_item_policy::{
    check_ethereal_legitimacy, check_item_code_legitimacy, check_socket_legitimacy,
};

mod affix_policy;
mod grading;
pub(super) mod model;
mod runeword_policy;

pub use affix_policy::{
    check_affix_group_legitimacy, check_alvl_legitimacy, check_alvl_legitimacy_detailed,
    check_staffmod_legitimacy, check_staffmod_legitimacy_detailed,
};

use grading::{consolidate_affix_stats, validate_item_properties};
pub use model::{
    StatValidation, StatValidationStatus, ValidationResult, ValidationWarning, WarningConfidenceTier,
};

pub use crate::engine::defense::validation_policy::{
    check_base_stat_legitimacy, check_base_stat_legitimacy_detailed,
};

pub use runeword_policy::{check_runeword_legitimacy, check_runeword_legitimacy_detailed};

pub use crate::domain::item::affix_catalog::{is_affix_eligible_for, lookup_prefix, lookup_suffix};

pub use crate::domain::item::specification::{ItemSpec, lookup_spec};


pub fn validate_item(item: &Item) -> Option<ValidationResult> {
    // Harness Invariant Gate: Only fully parsed items qualify for legitimacy checks.
    // Suppress warnings on partially parsed, opaque, or incomplete property bitstreams.
    if !item.is_completely_parsed() {
        return None;
    }

    let mut result = if let Some(spec) = lookup_spec(item) {
        match spec {
            ItemSpec::Unique(unique_spec) => Some(validate_item_properties(
                unique_spec.index,
                unique_spec.stats,
                &item.properties,
            )),
            ItemSpec::Set(set_spec) => Some(validate_item_properties(
                set_spec.index,
                set_spec.stats,
                &item.properties,
            )),
            ItemSpec::Runeword(rw_spec) => Some(validate_item_properties(
                runeword_display_name(rw_spec),
                rw_spec.stats,
                &item.runeword_attributes,
            )),
            ItemSpec::Affix(_) => None,
        }
    } else {
        match item.quality {
            Some(ItemQuality::Magic) | Some(ItemQuality::Rare) | Some(ItemQuality::Crafted) => {
                let prefixes = item.prefixes();
                let suffixes = item.suffixes();

                if prefixes.is_empty() && suffixes.is_empty() {
                    Some(ValidationResult {
                        spec_name: format!("{:?}", item.quality.unwrap_or(ItemQuality::Normal)),
                        is_perfect: true,
                        score: 1.0,
                        stats: Vec::new(),
                        warnings: Vec::new(),
                        detailed_warnings: Vec::new(),
                    })
                } else {
                    let item_types = get_all_item_types(&item.code);
                    let mut warnings = Vec::new();

                for affix in prefixes.iter().chain(suffixes.iter()) {
                    if !is_affix_eligible_for(affix, &item_types) {
                        warnings.push(format!(
                            "Affix '{}' (id:{}) is not eligible for this item type",
                            affix.name, affix.id
                        ));
                    }
                }

                let consolidated_stats = consolidate_affix_stats(&prefixes, &suffixes);
                let names: Vec<&str> = prefixes
                    .iter()
                    .chain(suffixes.iter())
                    .map(|a| a.name)
                    .collect();
                let spec_name = if names.is_empty() {
                    "Unknown Affix Item".to_string()
                } else {
                    names.join(" ")
                };

                let mut res = validate_item_properties(
                    &spec_name,
                    &consolidated_stats,
                    &item.properties,
                );
                res.warnings.extend(warnings);
                Some(res)
                }
            }
            _ => Some(ValidationResult {
                spec_name: format!("{:?}", item.quality.unwrap_or(ItemQuality::Normal)),
                is_perfect: true,
                score: 1.0,
                stats: Vec::new(),
                warnings: Vec::new(),
                detailed_warnings: Vec::new(),
            }),
        }
    };

    if let Some(ref mut res) = result {
        res.warnings.extend(check_socket_legitimacy(item));

        let alvl_warnings = check_alvl_legitimacy_detailed(item);
        res.extend_detailed_warnings(alvl_warnings);

        let staffmod_warnings = check_staffmod_legitimacy_detailed(item);
        res.extend_detailed_warnings(staffmod_warnings);

        res.warnings.extend(check_affix_group_legitimacy(item));

        let rw_warnings = check_runeword_legitimacy_detailed(item);
        res.extend_detailed_warnings(rw_warnings);
        
        let base_stat_warnings = check_base_stat_legitimacy_detailed(item);
        res.extend_detailed_warnings(base_stat_warnings);

        res.warnings.extend(check_item_code_legitimacy(item));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe_item(code: &str, recipe: &[&str]) -> Item {
        let mut item = Item::default();
        item.code = code.to_string();
        item.is_runeword = true;
        item.header.is_runeword = true;
        item.runeword_id = Some(u16::MAX);
        item.properties_complete = true;
        item.sockets = Some(recipe.len() as u8);
        item.num_socketed_items = recipe.len() as u8;
        item.socketed_items = recipe
            .iter()
            .map(|code| {
                let mut child = Item::default();
                child.code = code.to_string();
                child.is_compact = true;
                child.properties_complete = true;
                child
            })
            .collect();
        item
    }

    #[d2r_macros::settled_contract_test]
    fn runeword_recipe_identity_matches_complete_gameplay_items() {
        for (bytes, key) in [
            (include_bytes!("../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Treachery_uea.d2s").as_slice(), "Runeword148"),
            (include_bytes!("../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Cure_xrn.d2s").as_slice(), "Runeword178"),
            (include_bytes!("../../tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Hustle_72h.d2s").as_slice(), "Runeword172"),
        ] {
            let items = Item::read_player_items_d2r_v3(bytes, &crate::item::HuffmanTree::new()).unwrap();
            let item = items.iter().find(|item| item.is_runeword).unwrap();
            let before = item.clone();
            let resolved = resolve_runeword(item).expect("complete unique source recipe");
            assert_eq!(resolved.template.name, key);
            assert_eq!(resolved.source, RunewordMatchSource::OrderedSocketRecipe);
            match lookup_spec(item) {
                Some(ItemSpec::Runeword(actual)) => assert_eq!(actual.name, key),
                _ => panic!("all consumers must share the source recipe identity"),
            }
            assert_eq!(*item, before);
        }
    }

    #[d2r_macros::settled_contract_test]
    fn runeword_recipe_identity_rejects_incomplete_or_inconsistent_evidence() {
        let base = recipe_item("uea", &["r13", "r10", "r20"]);
        assert_eq!(
            resolve_runeword(&base).unwrap().template.name,
            "Runeword148"
        );
        let mut controls = Vec::new();
        let mut reversed = base.clone();
        reversed.socketed_items.reverse();
        controls.push(reversed);
        let mut short = base.clone();
        short.socketed_items.pop();
        controls.push(short);
        let mut extra = base.clone();
        extra.socketed_items.push(extra.socketed_items[0].clone());
        controls.push(extra);
        let mut wrong_child = base.clone();
        wrong_child.socketed_items[0].code = "gcv".into();
        controls.push(wrong_child);
        let mut wrong_type = base.clone();
        wrong_type.code = "hax".into();
        controls.push(wrong_type);
        let mut unknown_type = base.clone();
        unknown_type.code = "unknown".into();
        controls.push(unknown_type);
        let mut incomplete = base.clone();
        incomplete.properties_complete = false;
        controls.push(incomplete);
        let mut incomplete_child = base.clone();
        incomplete_child.socketed_items[0].is_compact = false;
        incomplete_child.socketed_items[0].properties_complete = false;
        controls.push(incomplete_child);
        let mut nested_child = base.clone();
        nested_child.socketed_items[0]
            .socketed_items
            .push(base.socketed_items[1].clone());
        controls.push(nested_child);
        let mut socket_count = base.clone();
        socket_count.sockets = Some(4);
        controls.push(socket_count);
        let mut missing_socket_count = base.clone();
        missing_socket_count.sockets = None;
        controls.push(missing_socket_count);
        let mut declared = base.clone();
        declared.num_socketed_items = 2;
        controls.push(declared);
        let mut unflagged = base.clone();
        unflagged.is_runeword = false;
        unflagged.header.is_runeword = false;
        controls.push(unflagged);
        for item in controls {
            assert!(
                resolve_runeword(&item).is_none(),
                "unproven identity must remain unresolved: {item:?}"
            );
        }
    }

    #[d2r_macros::settled_contract_test]
    fn runeword_recipe_identity_preserves_duplicates_and_type_discrimination() {
        let recipe = ["r13", "r18", "r02"];
        assert_eq!(
            resolve_runeword(&recipe_item("hla", &recipe))
                .unwrap()
                .template
                .name,
            "Runeword171"
        );
        assert_eq!(
            resolve_runeword(&recipe_item("72h", &recipe))
                .unwrap()
                .template
                .name,
            "Runeword172"
        );
        assert_eq!(
            runeword_display_name(
                resolve_runeword(&recipe_item("hla", &["r12", "r22", "r22"]))
                    .unwrap()
                    .template
            ),
            "Bone"
        );
        assert!(resolve_runeword(&recipe_item("hla", &["r12", "r22"])).is_none());
    }

    #[d2r_macros::settled_contract_test]
    fn exact_runeword_id_keeps_precedence_over_a_different_valid_recipe() {
        let mut item = recipe_item("uap", &["r13", "r10", "r20"]);
        item.runeword_id = Some(32);
        let resolved = resolve_runeword(&item).unwrap();
        assert_eq!(resolved.source, RunewordMatchSource::SavedId);
        assert_eq!(runeword_display_name(resolved.template), "Enigma");
        assert!(check_runeword_legitimacy(&item)
            .iter()
            .any(|warning| warning.contains("incorrect rune sequence")));
        assert_eq!(item.runeword_id, Some(32));
    }

    #[test]
    fn test_get_all_item_types() {
        let axe_types = get_all_item_types("hax "); // Hand Axe
        assert!(axe_types.contains(&"axe"));
        assert!(axe_types.contains(&"mele"));
        assert!(axe_types.contains(&"weap"));

        let shld_types = get_all_item_types("buc "); // Buckler
        assert!(shld_types.contains(&"shld"));
        assert!(shld_types.contains(&"armo"));
        assert!(shld_types.contains(&"seco"));
    }

    #[test]
    fn test_runeword_sequence_violation() {
        let mut item = Item::empty_for_tests();
        item.is_runeword = true;
        item.runeword_id = Some(32); // Enigma (Jah Ith Ber = r31 r06 r30)
        item.code = "uap ".to_string(); // Archon Plate
        item.sockets = Some(3);
        
        let mut r31 = Item::empty_for_tests(); r31.code = "r31 ".to_string();
        let mut r06 = Item::empty_for_tests(); r06.code = "r06 ".to_string();
        let mut r30 = Item::empty_for_tests(); r30.code = "r30 ".to_string();
        
        // Correct order is r31, r06, r30. Let's use an incorrect one.
        item.socketed_items = vec![r31, r30, r06];
        
        let warnings = check_runeword_legitimacy(&item);
        assert!(warnings.iter().any(|w| w.contains("incorrect rune sequence")));
    }

    #[test]
    fn test_superior_ethereal_defense_validation() {
        let mut item = Item::empty_for_tests();
        item.code = "utp ".to_string(); // Archon Plate
        item.is_ethereal = true;
        item.quality = Some(crate::item::ItemQuality::High);
        
        // 15% Enhanced Defense
        item.properties.push(crate::item::ItemProperty::new(
            16,
            "item_armor_percent".to_string(),
            0,
            0,
            15,
            ItemBitRange::default(),
        ));
        
        // Expected: floor((524+1) * 1.5) * 1.15 = floor(787.5) * 1.15 = 787 * 1.15 = 905.05 -> 905?
        // Actually: floor(floor(525 * 1.5) * 1.15) = floor(787 * 1.15) = floor(905.05) = 905.
        // Wait, my check_base_stat_legitimacy uses floats.
        // Let's set it to 905.
        item.defense = Some(905);
        
        let warnings = check_base_stat_legitimacy(&item);
        assert!(warnings.is_empty(), "Expected no warnings for 905 defense, got: {:?}", warnings);
        
        // Violation case: 900 defense
        item.defense = Some(900);
        let warnings = check_base_stat_legitimacy(&item);
        assert!(warnings.iter().any(|w| w.contains("Defense 900 does not match expected 905")));
    }
}

#[cfg(test)]
mod unique_template_staffmod_tests {
    use super::*;
    use crate::engine::formatter::format_item;
    use crate::item::HuffmanTree;
    use d2r_macros::settled_contract_test;

    fn snakecord_property() -> Item {
        let mut item = Item::default();
        item.code = "vbl ".into();
        item.quality = Some(ItemQuality::Unique);
        item.unique_id = Some(113);
        item.level = Some(22);
        item.properties.push(ItemProperty::new(
            45,
            "poisonresist".into(),
            0,
            0,
            25,
            ItemBitRange::default(),
        ));
        item
    }

    #[settled_contract_test]
    fn unique_ownership_requires_identity_code_and_property_range() {
        let item = snakecord_property();
        let original = item.clone();
        assert!(check_staffmod_legitimacy_detailed(&item).is_empty());
        assert_eq!(item, original);

        for name in [
            "wrong_value24",
            "missing_id",
            "wrong_id103",
            "wrong_code_lbl",
            "magic_quality",
            "set_quality",
        ] {
            let mut control = item.clone();
            match name {
                "wrong_value24" => control.properties[0].value = 24,
                "missing_id" => control.unique_id = None,
                "wrong_id103" => control.unique_id = Some(103),
                "wrong_code_lbl" => control.code = "lbl ".into(),
                "magic_quality" => control.quality = Some(ItemQuality::Magic),
                "set_quality" => control.quality = Some(ItemQuality::Set),
                _ => unreachable!(),
            }
            let original = control.clone();
            let warnings = check_staffmod_legitimacy_detailed(&control);
            assert_eq!(warnings.len(), 1, "{name}: {warnings:?}");
            assert_eq!(warnings[0].category, "staffmod_ilvl", "{name}");
            assert!(warnings[0].message.contains("poisonresist"), "{name}");
            assert_eq!(control, original);
        }
    }

    #[settled_contract_test]
    fn actual_unique_property_owner_keeps_item_and_other_validation() {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act4/TESTDRUID_Act4_Enter.d2s"
        );
        let items = Item::read_player_items_d2r_v3(bytes, &HuffmanTree::new()).unwrap();
        let item = items
            .iter()
            .find(|item| item.quality == Some(ItemQuality::Unique) && item.unique_id == Some(113))
            .unwrap();
        assert_eq!(item.code.trim(), "vbl");
        assert_eq!(item.level, Some(22));
        assert!(
            item.properties.iter().any(|property| property.stat_id == 45
                && property.param == 0
                && property.value == 25)
        );
        let original = item.clone();
        assert!(check_staffmod_legitimacy_detailed(item).is_empty());
        let expected_base_warnings = check_base_stat_legitimacy_detailed(item);
        let formatted = format_item(item, "en", 0, 26);
        assert!(
            !formatted
                .detailed_warnings
                .iter()
                .any(|warning| warning.category == "staffmod_ilvl")
        );
        let base_warnings: Vec<_> = formatted
            .detailed_warnings
            .into_iter()
            .filter(|warning| matches!(warning.category, "defense_formula" | "defense_range"))
            .collect();
        assert_eq!(base_warnings, expected_base_warnings);
        assert_eq!(*item, original);
    }
}
