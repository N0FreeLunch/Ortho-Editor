//! Project existing poison-tooltip numbers independently of localization.

use crate::item::ItemProperty;

pub(crate) struct PoisonDisplay {
    pub min_damage: i64,
    pub max_damage: i64,
    pub duration_seconds: i64,
}

pub(crate) fn displayed_poison(props: &[ItemProperty]) -> Option<PoisonDisplay> {
    use crate::domain::stats::entity::StatMaturityTier;

    let group = props.get(..3)?;
    if group.iter().map(|p| p.stat_id).ne([57, 58, 59])
        || group.iter().any(|p| {
            p.param != 0
                || p.is_opaque
                || p.opaque_bits.is_some()
                || p.maturity != StatMaturityTier::Verified
                || p.value < 0
        })
        || group[0].value > group[1].value
        || group[2].value == 0
    {
        return None;
    }

    // Poison rates are damage/256 per frame; duration uses 25 frames/second.
    // Truncate totals before choosing the equal-damage or range template.
    let frames = i64::from(group[2].value);
    let min = i64::from(group[0].value).checked_mul(frames)? / 256;
    let max = i64::from(group[1].value).checked_mul(frames)? / 256;
    Some(PoisonDisplay {
        min_damage: min,
        max_damage: max,
        duration_seconds: frames / 25,
    })
}
