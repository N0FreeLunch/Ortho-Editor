//! Select composite and damage groups independently of localized rendering.

use std::collections::HashSet;

use crate::item::ItemProperty;

pub(crate) struct CompositeMatch {
    pub value: i32,
    pub indices: Vec<usize>,
}

pub(crate) fn matching_composite_rule(
    props: &[ItemProperty],
    stat_ids: &[u32],
    skip_indices: &HashSet<usize>,
) -> Option<CompositeMatch> {
    // Check if all stat_ids in the rule are present with the same value and non-opaque
    let mut matched_indices = Vec::with_capacity(stat_ids.len());
    let mut common_val: Option<i32> = None;
    let mut all_match = true;

    for &target_stat_id in stat_ids {
        let found = props.iter().enumerate().find(|(idx, p)| {
            !skip_indices.contains(idx)
                && p.stat_id == target_stat_id
                && !p.is_opaque
                && p.opaque_bits.is_none()
        });

        if let Some((idx, p)) = found {
            if let Some(val) = common_val {
                if p.value != val {
                    all_match = false;
                    break;
                }
            } else {
                common_val = Some(p.value);
            }
            matched_indices.push(idx);
        } else {
            all_match = false;
            break;
        }
    }

    if all_match && matched_indices.len() == stat_ids.len() {
        if let Some(val) = common_val {
            return Some(CompositeMatch {
                value: val,
                indices: matched_indices,
            });
        }
    }
    None
}

pub(crate) fn damage_companion_indices(
    props: &[ItemProperty],
    primary: usize,
) -> Option<Vec<usize>> {
    use crate::domain::stats::entity::StatMaturityTier;

    let anchor = props.get(primary)?;
    let family = match anchor.stat_id {
        21 => [21, 23, 159],
        22 => [22, 24, 160],
        _ => return None,
    };
    let members: Vec<_> = props
        .iter()
        .enumerate()
        .filter(|(_, p)| family.contains(&p.stat_id))
        .collect();
    if !(2..=3).contains(&members.len()) || members.first()?.0 != primary || anchor.value <= 0 {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    if members.iter().any(|(_, p)| {
        !ids.insert(p.stat_id)
            || p.param != 0
            || p.is_opaque
            || p.opaque_bits.is_some()
            || p.maturity != StatMaturityTier::Verified
            || p.value != anchor.value
            || p.raw_value != p.value
    }) {
        return None;
    }
    Some(
        members
            .into_iter()
            .skip(1)
            .map(|(index, _)| index)
            .collect(),
    )
}

pub(crate) enum ElementalKind {
    Fire,
    Lightning,
    Magic,
}

pub(crate) struct ElementalPair {
    pub max_index: usize,
    pub min: i32,
    pub max: i32,
    pub kind: ElementalKind,
}

pub(crate) fn matching_elemental_pair(
    props: &[ItemProperty],
    primary: usize,
    skip_indices: &HashSet<usize>,
) -> Option<ElementalPair> {
    let p = &props[primary];
    let (max_stat_id, kind) = match p.stat_id {
        48 => (49, ElementalKind::Fire),
        50 => (51, ElementalKind::Lightning),
        52 => (53, ElementalKind::Magic),
        _ => return None,
    };
    let (max_idx, max_p) = props.iter().enumerate().find(|(idx, other)| {
        *idx > primary && other.stat_id == max_stat_id && !skip_indices.contains(idx)
    })?;
    let min = p.value;
    let max = max_p.value;
    Some(ElementalPair {
        max_index: max_idx,
        min,
        max,
        kind,
    })
}
