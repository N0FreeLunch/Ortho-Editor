//! Canonical D2R v3 property-list emission and writer support checks.

use std::io;

use crate::domain::item::bit_emitter::{d2r_v3_write_checked, BitEmitter};

/// Shared ID framing for the current D2R v3 property-list writers.
struct D2rV3PropertyListLayout;

impl D2rV3PropertyListLayout {
    const STAT_ID: u32 =
        crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY.stat_id_bits as u32;
    const TERMINATOR: u32 = (1u32 << Self::STAT_ID) - 1;
}

pub(super) fn d2r_v3_write_scalar_list(
    emitter: &mut BitEmitter,
    properties: &[crate::domain::stats::ItemProperty],
) -> io::Result<()> {
    use D2rV3PropertyListLayout as W;
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidInput, message);
    let (terminal, scalar_properties) = properties
        .split_last()
        .ok_or_else(|| invalid("Missing final D2R v3 property terminator"))?;
    if terminal.stat_id != W::TERMINATOR || terminal.is_opaque || !terminal.is_verified() {
        return Err(invalid("Missing verified final D2R v3 property terminator"));
    }
    for property in scalar_properties {
        if property.is_opaque
            || !property.is_verified()
            || matches!(
                property.stat_id,
                17 | 18 | 50 | 51 | 57 | 58 | 59 | W::TERMINATOR
            )
        {
            return Err(invalid(
                "Opaque, unverified, paired or premature terminal property is unsupported",
            ));
        }
        let stat = crate::data::stat_costs::STAT_COSTS
            .iter()
            .find(|stat| stat.id == property.stat_id)
            .ok_or_else(|| invalid("Missing D2R v3 property definition"))?;
        if stat.save_bits == 0 || stat.save_bits > 31 {
            return Err(invalid("Unsupported D2R v3 scalar value width"));
        }
        d2r_v3_write_checked(emitter, property.stat_id as u64, W::STAT_ID, "stat ID")?;
        d2r_v3_write_checked(
            emitter,
            property.param as u64,
            stat.save_param_bits as u32,
            "stat parameter",
        )?;
        let raw = i64::from(property.value) + i64::from(stat.save_add);
        let raw = u64::try_from(raw).map_err(|_| invalid("Negative encoded D2R v3 value"))?;
        d2r_v3_write_checked(emitter, raw, stat.save_bits as u32, "stat value")?;
    }
    emitter.write_bits(W::TERMINATOR, W::STAT_ID)
}

pub(super) fn d2r_v3_write_weapon_list(
    emitter: &mut BitEmitter,
    properties: &[crate::domain::stats::ItemProperty],
) -> io::Result<()> {
    use crate::data::generated::layout_policy::D2R_V3_PAIRED_STAT_IDS;
    use D2rV3PropertyListLayout as W;
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidInput, message);
    let (terminal, values) = properties
        .split_last()
        .ok_or_else(|| invalid("Missing weapon property terminator"))?;
    if terminal.stat_id != W::TERMINATOR
        || terminal.is_opaque
        || !terminal.is_verified()
        || terminal.param != 0
        || terminal.value != 0
        || terminal.opaque_bits.is_some()
    {
        return Err(invalid("Invalid weapon property terminator"));
    }
    let write_value = |emitter: &mut BitEmitter,
                       property: &crate::domain::stats::ItemProperty,
                       companion: bool|
     -> io::Result<()> {
        if property.is_opaque
            || !property.is_verified()
            || property.opaque_bits.is_some()
            || property.stat_id == W::TERMINATOR
            || (companion && property.param != 0)
        {
            return Err(invalid("Unsupported weapon property state"));
        }
        let stat = crate::data::stat_costs::STAT_COSTS
            .iter()
            .find(|stat| stat.id == property.stat_id)
            .ok_or_else(|| invalid("Missing weapon property definition"))?;
        if stat.save_bits == 0 || stat.save_bits > 32 {
            return Err(invalid("Unsupported weapon property width"));
        }
        if !companion {
            d2r_v3_write_checked(
                emitter,
                u64::from(property.stat_id),
                W::STAT_ID,
                "weapon stat ID",
            )?;
            d2r_v3_write_checked(
                emitter,
                u64::from(property.param),
                u32::from(stat.save_param_bits),
                "weapon stat parameter",
            )?;
        }
        let raw = u64::try_from(i64::from(property.value) + i64::from(stat.save_add))
            .map_err(|_| invalid("Negative encoded weapon property value"))?;
        d2r_v3_write_checked(emitter, raw, u32::from(stat.save_bits), "weapon stat value")
    };
    let mut index = 0;
    while index < values.len() {
        let primary = &values[index];
        write_value(emitter, primary, false)?;
        index += 1;
        if let Some((_, companions)) = D2R_V3_PAIRED_STAT_IDS
            .iter()
            .find(|(id, _)| *id == primary.stat_id)
        {
            for &expected in *companions {
                let companion = values
                    .get(index)
                    .filter(|property| property.stat_id == expected)
                    .ok_or_else(|| invalid("Missing or reordered weapon property companion"))?;
                write_value(emitter, companion, true)?;
                index += 1;
            }
        }
    }
    emitter.write_bits(W::TERMINATOR, W::STAT_ID)
}
