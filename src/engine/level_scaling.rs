//! Project existing per-level stat numbers independently of localized text.

use crate::data::stat_costs::StatCost;

pub(crate) struct LevelScaledDisplay {
    pub value: i32,
    pub per_level_factor: Option<f64>,
}

pub(crate) fn displayed_level_scaling(
    value: i32,
    char_level: u8,
    cost: &StatCost,
) -> LevelScaledDisplay {
    let mut display_value = value;
    let mut per_level_factor = None;
    if let Some(op) = cost.op {
        if op != 0 {
            match op {
                2 | 4 | 5 => {
                    let shift = cost.op_param.unwrap_or(0);
                    display_value = (value * char_level as i32) >> shift;

                    if shift > 0 {
                        let factor = value as f64 / (1 << shift) as f64;
                        per_level_factor = Some(factor);
                    }
                }
                _ => {}
            }
        }
    }

    LevelScaledDisplay {
        value: display_value,
        per_level_factor,
    }
}
