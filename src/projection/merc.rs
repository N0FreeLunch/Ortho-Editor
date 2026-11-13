//! Mercenary domain helpers for projection, slot compatibility, and `.d2s` writeback.

use crate::domain::forensic::v105::mercenary::MercenaryState;

/// Returns the minimum experience required for the given mercenary level (`1..=98`).
pub fn exp_for_merc_level(level: i32) -> u32 {
    MercenaryState::min_experience_for_level(level.clamp(1, 98) as u8)
}

/// Infers the canonical D2R mercenary act (`0` = unhired, `1 | 2 | 3 | 5`) from `MercenaryState`.
pub fn inferred_merc_act(state: &MercenaryState, has_items: bool) -> i32 {
    let hired = state.hireling_id != 0
        || state.class_id != 0
        || state.experience > 0
        || has_items;
    if !hired {
        0
    } else if state.class_id == 9 {
        5
    } else if state.class_id == 1 {
        3
    } else if state.hireling_id >= 8 {
        2
    } else {
        1
    }
}

/// Updates `MercenaryState` identity fields (`class_id`, `subtype_id`, `hireling_id`, `name_id`)
/// to match the requested Act (`1`, `2`, `3`, or `5`).
pub fn apply_merc_act_preset(state: &mut MercenaryState, act_index: i32) {
    match act_index {
        1 => {
            state.class_id = 0;
            state.subtype_id = 1;
            state.name_id = state.name_id.max(1);
        }
        2 => {
            state.class_id = 0;
            state.subtype_id = 8;
            state.name_id = state.name_id.max(1);
        }
        3 => {
            state.class_id = 1;
            if !matches!(state.subtype_id, 15..=17) {
                state.subtype_id = 16; // Cold Iron Wolf default
            }
            state.name_id = if state.name_id == 0 { 0x25 } else { state.name_id };
        }
        5 => {
            state.class_id = 9;
            state.subtype_id = 0;
            state.name_id = state.name_id.max(1);
        }
        _ => {}
    }
    state.hireling_id = if state.class_id == 0 {
        state.subtype_id
    } else {
        state.class_id
    };
}

/// Maps a mercenary `body_loc` (`1, 3, 4, 5`) to its canonical slot label.
pub fn merc_slot_label(body_loc: i32) -> Option<&'static str> {
    match body_loc {
        1 => Some("Head"),
        3 => Some("Armor"),
        4 => Some("Weapon1"),
        5 => Some("Weapon2"),
        _ => None,
    }
}

/// Validates whether `item_code` can be equipped in mercenary `body_loc` for `act_index`.
pub fn item_fits_merc_slot(
    item_code: &str,
    body_loc: i32,
    act_index: i32,
) -> Result<(), &'static str> {
    if act_index == 0 {
        return Err("merc_not_hired");
    }
    let types = crate::engine::validation::get_all_item_types(item_code);
    if types.is_empty() {
        return match body_loc {
            1 | 3 | 4 | 5 => Ok(()),
            _ => Err("invalid_merc_slot"),
        };
    }
    let has = |t: &str| types.iter().any(|k| *k == t);
    match body_loc {
        1 => {
            if has("helm") || has("phlm") || has("pelt") || has("circ") {
                Ok(())
            } else {
                Err("merc_slot_requires_helm")
            }
        }
        3 => {
            if has("tors") {
                Ok(())
            } else {
                Err("merc_slot_requires_armor")
            }
        }
        4 => {
            let valid = match act_index {
                1 => has("bow") || has("abow"),
                2 => has("spea") || has("aspe") || has("pole"),
                3 => has("swor"),
                5 => has("swor"),
                _ => false,
            };
            if valid {
                Ok(())
            } else {
                Err("merc_weapon_incompatible_with_act")
            }
        }
        5 => {
            let valid = match act_index {
                3 => has("shie") || has("ashd"),
                5 => has("swor"),
                _ => false,
            };
            if valid {
                Ok(())
            } else {
                Err("merc_offhand_incompatible_with_act")
            }
        }
        _ => Err("invalid_merc_slot"),
    }
}
