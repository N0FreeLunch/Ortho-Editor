//! Resolve source runeword identity and recipe metadata independently of warnings.

use crate::data::item_specs::Runeword;
use crate::data::{runes, runewords};
use crate::domain::item::type_hierarchy::get_all_item_types;
use crate::item::Item;

fn lookup_runeword_rune_data(rw: &'static Runeword) -> Option<&'static runes::RuneData> {
    runes::RUNES.iter().find(|entry| entry.name == rw.name)
}

pub fn runeword_display_name(rw: &'static Runeword) -> &'static str {
    if rw.id == 2 && rw.name.starts_with("Runeword") {
        return "Authority";
    }
    lookup_runeword_rune_data(rw)
        .map(|entry| entry.rune_name)
        .filter(|name: &&str| !name.is_empty())
        .unwrap_or(rw.name)
}

pub(crate) fn runeword_expected_runes(rw: &'static Runeword) -> Vec<&'static str> {
    if rw.id == 2 && rw.name.starts_with("Runeword") {
        return vec!["r15", "r13", "r08"]; // Thul Shael Ral
    }
    if !rw.runes.is_empty() {
        return rw.runes.to_vec();
    }

    lookup_runeword_rune_data(rw)
        .map(|entry| {
            [
                entry.rune1,
                entry.rune2,
                entry.rune3,
                entry.rune4,
                entry.rune5,
                entry.rune6,
            ]
            .into_iter()
            .filter(|rune: &&str| !rune.is_empty())
            .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn runeword_expected_item_types(rw: &'static Runeword) -> Vec<&'static str> {
    if !rw.item_types.is_empty() {
        return rw.item_types.to_vec();
    }

    lookup_runeword_rune_data(rw)
        .map(|entry| {
            [
                entry.itype1,
                entry.itype2,
                entry.itype3,
                entry.itype4,
                entry.itype5,
                entry.itype6,
            ]
            .into_iter()
            .filter(|item_type: &&str| !item_type.is_empty())
            .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunewordMatchSource {
    SavedId,
    OrderedSocketRecipe,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedRuneword {
    pub template: &'static Runeword,
    pub source: RunewordMatchSource,
}

fn runeword_recipe_type_matches(
    complete: bool,
    allowed: &[&str],
    excluded: &[&str],
    item_types: &[&str],
) -> bool {
    complete
        && allowed
            .iter()
            .any(|ty| !ty.is_empty() && item_types.contains(ty))
        && !excluded
            .iter()
            .any(|ty| !ty.is_empty() && item_types.contains(ty))
}

fn unique_runeword_candidate(
    mut candidates: impl Iterator<Item = &'static Runeword>,
) -> Option<&'static Runeword> {
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

/// Resolves a source identity without changing or decoding the saved raw ID.
/// Exact IDs retain precedence so malformed known recipes remain detectable.
pub fn resolve_runeword(item: &Item) -> Option<ResolvedRuneword> {
    if !item.is_runeword && !item.header.is_runeword {
        return None;
    }
    if let Some(raw_id) = item.runeword_id {
        if let Some(template) = runewords::RUNEWORDS
            .iter()
            .find(|template| template.id == u32::from(raw_id))
        {
            return Some(ResolvedRuneword {
                template,
                source: RunewordMatchSource::SavedId,
            });
        }
    }
    if !item.is_completely_parsed()
        || item.socketed_items.is_empty()
        || item.sockets.map(usize::from) != Some(item.socketed_items.len())
        || usize::from(item.num_socketed_items) != item.socketed_items.len()
        || !item
            .socketed_items
            .iter()
            .all(|child| child.is_completely_parsed() && child.socketed_items.is_empty())
    {
        return None;
    }
    let actual_runes: Vec<_> = item
        .socketed_items
        .iter()
        .map(|child| child.code.trim())
        .collect();
    let item_types = get_all_item_types(&item.code);
    let candidates = runewords::RUNEWORDS.iter().filter(|template| {
        if template.runes != actual_runes.as_slice() {
            return false;
        }
        let Some(metadata) = lookup_runeword_rune_data(template) else {
            return false;
        };
        let allowed = [
            metadata.itype1,
            metadata.itype2,
            metadata.itype3,
            metadata.itype4,
            metadata.itype5,
            metadata.itype6,
        ];
        let excluded = [metadata.etype1, metadata.etype2, metadata.etype3];
        runeword_recipe_type_matches(metadata.complete, &allowed, &excluded, &item_types)
    });
    unique_runeword_candidate(candidates).map(|template| ResolvedRuneword {
        template,
        source: RunewordMatchSource::OrderedSocketRecipe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[d2r_macros::settled_contract_test]
    fn private_candidate_qualification_preserves_existing_contract() {
        let template = &runewords::RUNEWORDS[0];
        assert!(unique_runeword_candidate([template, template].into_iter()).is_none());
        assert!(unique_runeword_candidate([].into_iter()).is_none());
        assert!(runeword_recipe_type_matches(
            true,
            &["tors"],
            &[],
            &["armo", "tors"]
        ));
        assert!(!runeword_recipe_type_matches(
            false,
            &["tors"],
            &[],
            &["tors"]
        ));
        assert!(!runeword_recipe_type_matches(
            true,
            &["tors"],
            &["tors"],
            &["tors"]
        ));
    }
}
