// Character Combat Sheet & Derived Stats Engine
// Accurately calculates effective/stacked resistances, stat bonuses, and breakpoints
// following D2R game rules, including 9-bit bias ranges and Anya quest flags.

use crate::domain::item::stream::ItemContainer;
use crate::domain::progression::quest::QuestSet;
use crate::domain::stats::AttributeSection;
use crate::item::Item;
use serde::{Deserialize, Serialize};

pub const STAT_STRENGTH: u32 = 0;
pub const STAT_ENERGY: u32 = 1;
pub const STAT_DEXTERITY: u32 = 2;
pub const STAT_VITALITY: u32 = 3;

pub const STAT_DAMAGE_RESIST: u32 = 36;
pub const STAT_MAGIC_RESIST: u32 = 37;
pub const STAT_FIRE_RESIST: u32 = 39;
pub const STAT_MAX_FIRE_RESIST: u32 = 40;
pub const STAT_LIGHT_RESIST: u32 = 41;
pub const STAT_MAX_LIGHT_RESIST: u32 = 42;
pub const STAT_COLD_RESIST: u32 = 43;
pub const STAT_MAX_COLD_RESIST: u32 = 44;
pub const STAT_POISON_RESIST: u32 = 45;
pub const STAT_MAX_POISON_RESIST: u32 = 46;

pub const STAT_IAS: u32 = 93;
pub const STAT_FHR: u32 = 99;
pub const STAT_FBR: u32 = 102;
pub const STAT_FCR: u32 = 105;

/// Game difficulty determining base resistance penalty
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Normal,
    Nightmare,
    Hell,
}

impl Difficulty {
    pub fn penalty(&self) -> i32 {
        match self {
            Difficulty::Normal => 0,
            Difficulty::Nightmare => 40,
            Difficulty::Hell => 100,
        }
    }
}

/// Decodes raw bitstream values into signed logical in-game values
/// accounting for Save Add biases (e.g., save_add=200 for resistances).
pub fn decode_stat_value(stat_id: u32, raw_value: i32) -> i32 {
    match stat_id {
        STAT_FIRE_RESIST
        | STAT_LIGHT_RESIST
        | STAT_COLD_RESIST
        | STAT_POISON_RESIST
        | STAT_DAMAGE_RESIST
        | STAT_MAGIC_RESIST => raw_value - 200,
        _ => raw_value,
    }
}

/// Encodes signed logical in-game values back into raw bitstream values.
pub fn encode_stat_value(stat_id: u32, logical_value: i32) -> i32 {
    match stat_id {
        STAT_FIRE_RESIST
        | STAT_LIGHT_RESIST
        | STAT_COLD_RESIST
        | STAT_POISON_RESIST
        | STAT_DAMAGE_RESIST
        | STAT_MAGIC_RESIST => logical_value + 200,
        _ => logical_value,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeDetail {
    pub base: i32,
    pub bonus: i32,
    pub total: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResistanceDetail {
    /// Effective applied in-game resistance: min(max_cap, stacked_raw), clamped to [-100, 95]
    pub effective: i32,
    /// Absolute maximum cap: min(95, 75 + sum of item max resist bonuses)
    pub max_cap: i32,
    /// Raw un-capped stacked total: gear + charm + anya - difficulty_penalty
    pub stacked_raw: i32,
    /// Contribution from Anya quest scroll of resistance (+10 per difficulty cleared)
    pub anya_bonus: i32,
    /// Contribution from equipped gear (including negative values from sunder charms)
    pub gear_bonus: i32,
    /// Contribution from charms in player inventory
    pub charm_bonus: i32,
    /// Active penalty from difficulty (-0, -40, or -100)
    pub difficulty_penalty: i32,
    /// Stacked margin above the max cap (stacked_raw - max_cap)
    pub margin: i32,
}

impl ResistanceDetail {
    pub fn calculate(
        gear_bonus: i32,
        gear_max_bonus: i32,
        charm_bonus: i32,
        charm_max_bonus: i32,
        anya_bonus: i32,
        difficulty: Difficulty,
    ) -> Self {
        let total_max_bonus = gear_max_bonus + charm_max_bonus;
        let max_cap = (75 + total_max_bonus).min(95);

        let penalty = difficulty.penalty();
        let stacked_raw = gear_bonus + charm_bonus + anya_bonus - penalty;
        let effective = stacked_raw.min(max_cap).max(-100);
        let margin = stacked_raw - max_cap;

        Self {
            effective,
            max_cap,
            stacked_raw,
            anya_bonus,
            gear_bonus,
            charm_bonus,
            difficulty_penalty: penalty,
            margin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterCombatSheet {
    pub difficulty: Difficulty,
    pub fire_res: ResistanceDetail,
    pub cold_res: ResistanceDetail,
    pub light_res: ResistanceDetail,
    pub poison_res: ResistanceDetail,
    pub damage_reduction_pct: i32,
    pub magic_res: i32,
    pub fcr: i32,
    pub fhr: i32,
    pub ias: i32,
    pub fbr: i32,
    pub strength: AttributeDetail,
    pub dexterity: AttributeDetail,
    pub vitality: AttributeDetail,
    pub energy: AttributeDetail,
    pub unspent_stat_points: u32,
    pub unspent_skill_points: u32,
}

fn is_charm_item_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed == "cm1" || trimmed == "cm2" || trimmed == "cm3"
}

/// Evaluates Anya quest bonus by counting completed scrolls of resistance across difficulties
pub fn count_anya_scroll_bonuses(quests: Option<&QuestSet>) -> i32 {
    let Some(qs) = quests else { return 0 };
    let mut completed_count = 0;
    for q in qs.quests() {
        if q.name() == "Prison of Ice" && (q.is_completed() || q.is_consumed_scroll()) {
            completed_count += 1;
        }
    }
    completed_count * 10
}

#[derive(Default)]
struct ResistanceBonuses {
    resistance: i32,
    maximum: i32,
}

#[derive(Default)]
struct ElementalBonuses {
    fire: ResistanceBonuses,
    cold: ResistanceBonuses,
    light: ResistanceBonuses,
    poison: ResistanceBonuses,
}

/// Aggregates item properties based on container eligibility
pub fn calculate_combat_sheet(
    attributes: Option<&AttributeSection>,
    quests: Option<&QuestSet>,
    items: &[(Item, ItemContainer)],
    difficulty: Difficulty,
) -> CharacterCombatSheet {
    let anya_bonus = count_anya_scroll_bonuses(quests);

    let mut gear = ElementalBonuses::default();
    let mut charms = ElementalBonuses::default();

    let mut damage_reduction_pct = 0;
    let mut magic_res = 0;
    let mut fcr = 0;
    let mut fhr = 0;
    let mut ias = 0;
    let mut fbr = 0;

    let mut bonus_str = 0;
    let mut bonus_dex = 0;
    let mut bonus_vit = 0;
    let mut bonus_ene = 0;

    for (item, container) in items {
        let is_equipped = *container == ItemContainer::Equipment;
        let is_inv_charm = *container == ItemContainer::Inventory && is_charm_item_code(&item.code);

        if !is_equipped && !is_inv_charm {
            continue;
        }

        let elemental = if is_equipped { &mut gear } else { &mut charms };
        for prop in &item.properties {
            let val = prop.value;

            match prop.stat_id {
                STAT_FIRE_RESIST => elemental.fire.resistance += val,
                STAT_MAX_FIRE_RESIST => elemental.fire.maximum += val,
                STAT_COLD_RESIST => elemental.cold.resistance += val,
                STAT_MAX_COLD_RESIST => elemental.cold.maximum += val,
                STAT_LIGHT_RESIST => elemental.light.resistance += val,
                STAT_MAX_LIGHT_RESIST => elemental.light.maximum += val,
                STAT_POISON_RESIST => elemental.poison.resistance += val,
                STAT_MAX_POISON_RESIST => elemental.poison.maximum += val,
                STAT_DAMAGE_RESIST => damage_reduction_pct += val,
                STAT_MAGIC_RESIST => magic_res += val,
                STAT_FCR => fcr += val,
                STAT_FHR => fhr += val,
                STAT_IAS => ias += val,
                STAT_FBR => fbr += val,
                STAT_STRENGTH => bonus_str += val,
                STAT_DEXTERITY => bonus_dex += val,
                STAT_VITALITY => bonus_vit += val,
                STAT_ENERGY => bonus_ene += val,
                _ => {}
            }
        }
    }

    let fire_res = ResistanceDetail::calculate(
        gear.fire.resistance,
        gear.fire.maximum,
        charms.fire.resistance,
        charms.fire.maximum,
        anya_bonus,
        difficulty,
    );
    let cold_res = ResistanceDetail::calculate(
        gear.cold.resistance,
        gear.cold.maximum,
        charms.cold.resistance,
        charms.cold.maximum,
        anya_bonus,
        difficulty,
    );
    let light_res = ResistanceDetail::calculate(
        gear.light.resistance,
        gear.light.maximum,
        charms.light.resistance,
        charms.light.maximum,
        anya_bonus,
        difficulty,
    );
    let poison_res = ResistanceDetail::calculate(
        gear.poison.resistance,
        gear.poison.maximum,
        charms.poison.resistance,
        charms.poison.maximum,
        anya_bonus,
        difficulty,
    );

    let (base_str, base_dex, base_vit, base_ene, unspent_stat, unspent_skill) = match attributes {
        Some(attr) => (
            attr.actual_value(STAT_STRENGTH, false).unwrap_or(0),
            attr.actual_value(STAT_DEXTERITY, false).unwrap_or(0),
            attr.actual_value(STAT_VITALITY, false).unwrap_or(0),
            attr.actual_value(STAT_ENERGY, false).unwrap_or(0),
            attr.actual_value(4, false).unwrap_or(0).max(0) as u32,
            attr.actual_value(5, false).unwrap_or(0).max(0) as u32,
        ),
        None => (0, 0, 0, 0, 0, 0),
    };

    CharacterCombatSheet {
        difficulty,
        fire_res,
        cold_res,
        light_res,
        poison_res,
        damage_reduction_pct,
        magic_res,
        fcr,
        fhr,
        ias,
        fbr,
        strength: AttributeDetail {
            base: base_str,
            bonus: bonus_str,
            total: base_str + bonus_str,
        },
        dexterity: AttributeDetail {
            base: base_dex,
            bonus: bonus_dex,
            total: base_dex + bonus_dex,
        },
        vitality: AttributeDetail {
            base: base_vit,
            bonus: bonus_vit,
            total: base_vit + bonus_vit,
        },
        energy: AttributeDetail {
            base: base_ene,
            bonus: bonus_ene,
            total: base_ene + bonus_ene,
        },
        unspent_stat_points: unspent_stat,
        unspent_skill_points: unspent_skill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::ItemBitRange;
    use crate::item::ItemProperty;

    fn make_test_prop(stat_id: u32, logical_value: i32) -> ItemProperty {
        let raw_value = encode_stat_value(stat_id, logical_value);
        ItemProperty::new(
            stat_id,
            "test_prop".to_string(),
            0,
            raw_value,
            logical_value,
            ItemBitRange::default(),
        )
    }

    #[d2r_macros::hypothesis_test]
    fn current_combat_sources_preserve_values_scope_and_input() {
        fn source_item(code: &str, values: &[(u32, i32)]) -> Item {
            let mut item = Item::default();
            item.code = code.into();
            item.properties = values
                .iter()
                .map(|&(id, value)| {
                    ItemProperty::new(id, String::new(), 0, 12_345, value, ItemBitRange::default())
                })
                .collect();
            item
        }

        let gear_values = [
            (STAT_FIRE_RESIST, 20),
            (STAT_FIRE_RESIST, 20),
            (STAT_MAX_FIRE_RESIST, 5),
            (STAT_COLD_RESIST, 50),
            (STAT_MAX_COLD_RESIST, 6),
            (STAT_LIGHT_RESIST, 60),
            (STAT_MAX_LIGHT_RESIST, 7),
            (STAT_POISON_RESIST, 70),
            (STAT_MAX_POISON_RESIST, 8),
            (STAT_STRENGTH, 11),
            (STAT_ENERGY, 12),
            (STAT_DEXTERITY, 13),
            (STAT_VITALITY, 14),
            (STAT_DAMAGE_RESIST, 5),
            (STAT_MAGIC_RESIST, 6),
            (STAT_IAS, 7),
            (STAT_FHR, 8),
            (STAT_FBR, 9),
            (STAT_FCR, 10),
            (999, 9_000),
        ];
        let mut gear = source_item("cap", &gear_values);
        gear.is_runeword = true;
        gear.runeword_attributes = vec![make_test_prop(STAT_FIRE_RESIST, 5_000)];
        gear.set_attributes = vec![vec![make_test_prop(STAT_FIRE_RESIST, 5_000)]];
        gear.socketed_items = vec![source_item("r01", &[(STAT_FIRE_RESIST, 5_000)])];
        let mut charm = source_item(
            "cm2 ",
            &[
                (STAT_FIRE_RESIST, -15),
                (STAT_MAX_FIRE_RESIST, 1),
                (STAT_COLD_RESIST, -25),
                (STAT_MAX_COLD_RESIST, 2),
                (STAT_LIGHT_RESIST, -35),
                (STAT_MAX_LIGHT_RESIST, 3),
                (STAT_POISON_RESIST, -45),
                (STAT_MAX_POISON_RESIST, 4),
            ],
        );
        for id in [
            STAT_STRENGTH,
            STAT_ENERGY,
            STAT_DEXTERITY,
            STAT_VITALITY,
            STAT_DAMAGE_RESIST,
            STAT_MAGIC_RESIST,
            STAT_IAS,
            STAT_FHR,
            STAT_FBR,
            STAT_FCR,
        ] {
            charm.properties.push(make_test_prop(id, 1));
        }
        let equipped_charm = source_item(
            "cm1 ",
            &[
                (STAT_FIRE_RESIST, 5),
                (STAT_MAX_FIRE_RESIST, 1),
                (STAT_COLD_RESIST, 5),
                (STAT_MAX_COLD_RESIST, 1),
                (STAT_LIGHT_RESIST, 5),
                (STAT_MAX_LIGHT_RESIST, 1),
                (STAT_POISON_RESIST, 5),
                (STAT_MAX_POISON_RESIST, 1),
            ],
        );
        let excluded: Vec<_> = gear_values.iter().map(|&(id, _)| (id, 777)).collect();
        let items = vec![
            (gear, ItemContainer::Equipment),
            (equipped_charm, ItemContainer::Equipment),
            (charm, ItemContainer::Inventory),
            (source_item("cap", &excluded), ItemContainer::Inventory),
            (source_item("cm3 ", &excluded), ItemContainer::Stash),
        ];
        let before = items.clone();
        for difficulty in [Difficulty::Normal, Difficulty::Nightmare, Difficulty::Hell] {
            let sheet = calculate_combat_sheet(None, None, &items, difficulty);
            assert_eq!(
                sheet.fire_res,
                ResistanceDetail::calculate(45, 6, -15, 1, 0, difficulty)
            );
            assert_eq!(
                sheet.cold_res,
                ResistanceDetail::calculate(55, 7, -25, 2, 0, difficulty)
            );
            assert_eq!(
                sheet.light_res,
                ResistanceDetail::calculate(65, 8, -35, 3, 0, difficulty)
            );
            assert_eq!(
                sheet.poison_res,
                ResistanceDetail::calculate(75, 9, -45, 4, 0, difficulty)
            );
            assert_eq!((sheet.damage_reduction_pct, sheet.magic_res), (6, 7));
            assert_eq!((sheet.ias, sheet.fhr, sheet.fbr, sheet.fcr), (8, 9, 10, 11));
            assert_eq!(
                (
                    sheet.strength.total,
                    sheet.energy.total,
                    sheet.dexterity.total,
                    sheet.vitality.total
                ),
                (12, 13, 14, 15)
            );
            assert_eq!(
                (sheet.unspent_stat_points, sheet.unspent_skill_points),
                (0, 0)
            );
        }
        assert_eq!(items, before);
    }

    #[test]
    fn test_resistance_bias_decoding_and_encoding() {
        // Raw 130 with save_add 200 should decode to -70 (Flame Rift sunder charm)
        assert_eq!(decode_stat_value(STAT_FIRE_RESIST, 130), -70);
        assert_eq!(encode_stat_value(STAT_FIRE_RESIST, -70), 130);

        // Raw 230 with save_add 200 should decode to +30 (standard +30% Fire Resist)
        assert_eq!(decode_stat_value(STAT_FIRE_RESIST, 230), 30);
        assert_eq!(encode_stat_value(STAT_FIRE_RESIST, 30), 230);

        // Zero point: raw 200 -> 0%
        assert_eq!(decode_stat_value(STAT_FIRE_RESIST, 200), 0);
        assert_eq!(encode_stat_value(STAT_FIRE_RESIST, 0), 200);

        // Max resist has no save_add (save_add: 0)
        assert_eq!(decode_stat_value(STAT_MAX_FIRE_RESIST, 10), 10);
        assert_eq!(encode_stat_value(STAT_MAX_FIRE_RESIST, 10), 10);
    }

    #[test]
    fn test_effective_and_stacked_resistance_calculation() {
        // Gear: +120% Fire Resist, +10% Max Fire Resist
        // Charm: +25% Fire Resist
        // Anya: 3 difficulties cleared (+30%)
        // Difficulty: Hell (-100% penalty)
        let res = ResistanceDetail::calculate(120, 10, 25, 0, 30, Difficulty::Hell);

        // Max cap: 75 + 10 = 85%
        assert_eq!(res.max_cap, 85);
        // Stacked raw: 120 + 25 + 30 - 100 = 75%
        assert_eq!(res.stacked_raw, 75);
        // Effective: min(85, 75) = 75%
        assert_eq!(res.effective, 75);
        // Margin: 75 - 85 = -10 (needs +10% to cap)
        assert_eq!(res.margin, -10);

        // High stacked case (exceeding cap):
        // Gear: +180% Fire Resist, +15% Max Fire Resist
        let res_high = ResistanceDetail::calculate(180, 15, 20, 0, 30, Difficulty::Hell);
        // Max cap: min(95, 75 + 15) = 90%
        assert_eq!(res_high.max_cap, 90);
        // Stacked raw: 180 + 20 + 30 - 100 = 130%
        assert_eq!(res_high.stacked_raw, 130);
        // Effective: min(90, 130) = 90%
        assert_eq!(res_high.effective, 90);
        // Margin: 130 - 90 = +40% margin over golden res
        assert_eq!(res_high.margin, 40);

        // Max cap clamp at 95%
        let res_capped = ResistanceDetail::calculate(200, 30, 0, 0, 30, Difficulty::Hell);
        assert_eq!(res_capped.max_cap, 95);
    }

    #[test]
    fn test_sunder_charm_negative_resistance_interaction() {
        // Character wearing sunder charm (-70% Fire Res) in Hell with no gear
        let res_sunder = ResistanceDetail::calculate(-70, 0, 0, 0, 30, Difficulty::Hell);
        // Stacked raw: -70 + 0 + 30 - 100 = -140%
        assert_eq!(res_sunder.stacked_raw, -140);
        // Effective clamped to minimum -100%
        assert_eq!(res_sunder.effective, -100);
        assert_eq!(res_sunder.margin, -140 - 75);
    }

    #[test]
    fn test_charm_container_isolation() {
        // Items in Stash or Cube must not contribute to combat sheet
        let mut charm_inv = Item::default();
        charm_inv.code = "cm1 ".to_string();
        charm_inv.properties.push(make_test_prop(STAT_FIRE_RESIST, 20));

        let mut charm_stash = Item::default();
        charm_stash.code = "cm1 ".to_string();
        charm_stash.properties.push(make_test_prop(STAT_FIRE_RESIST, 30));

        let items = vec![
            (charm_inv, ItemContainer::Inventory),
            (charm_stash, ItemContainer::Stash),
        ];

        let sheet = calculate_combat_sheet(None, None, &items, Difficulty::Normal);

        // Only inventory charm (+20%) is counted
        assert_eq!(sheet.fire_res.charm_bonus, 20);
        assert_eq!(sheet.fire_res.gear_bonus, 0);
        assert_eq!(sheet.fire_res.stacked_raw, 20);
    }
}
