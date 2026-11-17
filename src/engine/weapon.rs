//! Project existing weapon-tooltip values independently of localization.

use crate::data::DataRepository;
use crate::data::item_specs::get_base_item_spec;
use crate::data::weapons::get_weapon_base_speed;
use crate::item::Item;

pub(crate) struct DamageRange {
    pub min: i32,
    pub max: i32,
}

pub(crate) enum AttackSpeedTier {
    VeryFast,
    Fast,
    Normal,
    Slow,
    VerySlow,
}

pub(crate) struct AttackSpeed {
    pub base_wsm: i8,
    pub tier: AttackSpeedTier,
}

pub(crate) struct WeaponDisplay {
    pub one_hand: Option<DamageRange>,
    pub two_hand: Option<DamageRange>,
    pub speed: Option<AttackSpeed>,
}

fn damage_range(
    mut base_min: i32,
    mut base_max: i32,
    ethereal: bool,
    ed_pct: i32,
    add_min: i32,
    add_max: i32,
) -> DamageRange {
    if ethereal {
        base_min = (base_min * 150) / 100;
        base_max = (base_max * 150) / 100;
    }
    let min = ((base_min * (100 + ed_pct)) / 100 + add_min).max(1);
    let max = ((base_max * (100 + ed_pct)) / 100 + add_max).max(min);
    DamageRange { min, max }
}

pub(crate) fn displayed_weapon(item: &Item) -> Option<WeaponDisplay> {
    let template = DataRepository::get_item_template(item.code.trim())?;
    if !template.is_weapon {
        return None;
    }
    let spec = get_base_item_spec(&item.code)?;
    let ed_pct: i32 = item
        .properties
        .iter()
        .filter(|p| p.stat_id == 17)
        .map(|p| p.value)
        .sum();
    let add_min: i32 = item
        .properties
        .iter()
        .filter(|p| p.stat_id == 21)
        .map(|p| p.value)
        .sum();
    let add_max: i32 = item
        .properties
        .iter()
        .filter(|p| p.stat_id == 22)
        .map(|p| p.value)
        .sum();
    let one_hand = (spec.min_dmg > 0 && spec.max_dmg > 0).then(|| {
        damage_range(
            spec.min_dmg as i32,
            spec.max_dmg as i32,
            item.is_ethereal,
            ed_pct,
            add_min,
            add_max,
        )
    });
    let two_hand = (spec.min_dmg2h > 0 && spec.max_dmg2h > 0).then(|| {
        damage_range(
            spec.min_dmg2h as i32,
            spec.max_dmg2h as i32,
            item.is_ethereal,
            ed_pct,
            add_min,
            add_max,
        )
    });
    let speed = get_weapon_base_speed(&item.code).map(|base_wsm| {
        let ias: i32 = item
            .properties
            .iter()
            .filter(|p| p.stat_id == 93)
            .map(|p| p.value)
            .sum();
        let effective_speed = (base_wsm as i32) - ias;
        let tier = if effective_speed <= -20 {
            AttackSpeedTier::VeryFast
        } else if effective_speed <= -10 {
            AttackSpeedTier::Fast
        } else if effective_speed <= 0 {
            AttackSpeedTier::Normal
        } else if effective_speed <= 15 {
            AttackSpeedTier::Slow
        } else {
            AttackSpeedTier::VerySlow
        };
        AttackSpeed { base_wsm, tier }
    });
    Some(WeaponDisplay {
        one_hand,
        two_hand,
        speed,
    })
}
