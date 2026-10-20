use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::legacy_extended_payload_code_policy;
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::domain::item::bit_emitter::BitEmitter;
use crate::domain::item::entity::{D2rV3AuxiliaryData, ExtendedStatsData};
use crate::domain::item::quality::ItemQuality;
use crate::domain::item::subdomains::affix::{
    AffixCombinator, MagicAffixSegment, RareAffixSegment, UniqueAffixSegment,
};
use crate::error::{ParsingError, ParsingResult};
use bitstream_io::BitRead;
use std::io;

/// Executes the payload representation selected by the enclosing item parser.
/// Recovery and item classification remain outside this codec interface.
pub(in crate::domain::item) enum ExtendedPayloadReadPlan {
    V3Compact(Option<D2rV3AuxiliaryData>),
    V3 { save_version: u32 },
    Legacy,
    Omitted,
}

impl ExtendedPayloadReadPlan {
    pub(in crate::domain::item) fn read<R: BitRead, P: LegacyExtendedPayloadReadPolicy>(
        self,
        cursor: &mut BitCursor<R>,
        code: &str,
        header: &ItemHeader,
        axiom: &P,
    ) -> ParsingResult<ExtendedStatsData> {
        match self {
            Self::V3Compact(auxiliary) => Ok(ExtendedStatsData {
                d2r_v3_auxiliary: auxiliary,
                ..Default::default()
            }),
            Self::V3 { save_version } => {
                read_d2r_v3_extended_payload(cursor, code, header, save_version)
            }
            Self::Legacy => {
                read_legacy_extended_payload(cursor, code, header, header.save_is_alpha, axiom)
            }
            Self::Omitted => Ok(ExtendedStatsData::default()),
        }
    }
}

/// Lazy queries for legacy payload execution, supplied by the enclosing policy.
/// Implementations retain the original receiver domains and invocation effects.
pub(crate) trait LegacyExtendedPayloadReadPolicy {
    fn class_specific_bits(&self) -> u32;
    fn has_v5_runeword_extra(&self, version: u8) -> bool;
    fn is_extended_stats_early_exit(&self, version: u8) -> bool;
    fn is_player_name_alpha_style(&self, version: u8) -> bool;
    fn is_summary_item(&self, version: u8, code: &str) -> bool;
    fn item_id_bits(&self) -> u32;
    fn item_level_bits(&self) -> u32;
    fn low_high_graphic_bits(&self) -> u32;
    fn multi_graphics_bits(&self) -> u32;
    fn needs_player_name_byte_alignment(&self, version: u8) -> bool;
    fn quality_bits(&self, is_alpha: bool) -> u32;
    fn quantity_bits(&self) -> u32;
    fn runeword_id_bits(&self) -> u32;
    fn runeword_level_bits(&self) -> u32;
    fn set_list_bits(&self) -> u32;
    fn socket_bits(&self) -> u32;
    fn stat_bits(&self, stat_id: u32) -> u32;
    fn teleport_bits(&self) -> u32;
    fn v5_runeword_extra_bits(&self) -> u32;
    fn is_alpha(&self) -> bool;
    fn is_runeword(&self, flags: u32) -> bool;
    fn reads_defense(&self) -> bool;
    fn reads_durability(&self) -> bool;
    fn reads_quantity(&self) -> bool;
    fn header_is_alpha(&self, version: u8, alpha_mode: bool) -> bool;
    fn header_is_v105_shadow(
        &self,
        version: u8,
        alpha_mode: bool,
        flags: u32,
        code_hint: Option<&str>,
    ) -> bool;
    fn in_nested_recovery(&self) -> bool;
    fn player_name_alignment_required(&self) -> bool;
}

pub fn read_player_name<R: BitRead>(
    cursor: &mut BitCursor<R>,
    _alpha_v5: bool,
) -> ParsingResult<String> {
    let mut name = String::new();
    for _ in 0..15 {
        let mut ch = 0u8;
        for j in 0..7 {
            if cursor.read_bit()? {
                ch |= 1 << j;
            }
        }
        if ch == 0 {
            break;
        }
        name.push(ch as char);
    }
    Ok(name)
}

pub fn write_player_name(emitter: &mut BitEmitter, name: &str, _alpha_v5: bool) -> io::Result<()> {
    let bytes = name.as_bytes();
    for i in 0..15 {
        let b = if i < bytes.len() { bytes[i] } else { 0 };
        emitter.write_bits(b as u32, 7)?;
        if b == 0 {
            break;
        }
    }
    Ok(())
}

pub(in crate::domain::item) fn read_d2r_v3_extended_payload<R: BitRead>(
    cursor: &mut BitCursor<R>,
    code: &str,
    header: &ItemHeader,
    save_version: u32,
) -> ParsingResult<ExtendedStatsData> {
    cursor.begin_segment(ItemSegmentType::ExtendedStats);
    let mut data = ExtendedStatsData::default();
    let trimmed_code = code.trim();

    let mut auxiliary = D2rV3AuxiliaryData {
        save_version,
        filled_sockets: cursor.read_bits::<u8>(3)?,
        ..Default::default()
    };
    data.id = Some(cursor.read_bits::<u32>(32)?);
    data.level = Some(cursor.read_bits::<u8>(7)?);
    let quality_raw = cursor.read_bits::<u8>(4)?;
    let quality = ItemQuality::from(quality_raw);
    data.quality = Some(quality);

    data.has_multiple_graphics = cursor.read_bit()?;
    if data.has_multiple_graphics {
        data.multi_graphics_bits = Some(cursor.read_bits::<u8>(3)?);
    }
    data.has_class_specific_data = cursor.read_bit()?;
    if data.has_class_specific_data {
        data.class_specific_bits = Some(cursor.read_bits::<u16>(11)?);
    }

    match quality {
        ItemQuality::Low => {
            data.low_high_graphic_bits = Some(cursor.read_bits::<u8>(3)?);
        }
        ItemQuality::Normal => {
            if matches!(trimmed_code, "cm1" | "cm2" | "cm3") {
                cursor.skip_and_record(12)?;
            } else if matches!(trimmed_code, "tsc" | "isc" | "tbk" | "ibk") {
                data.tbk_ibk_teleport = Some(cursor.read_bits::<u8>(5)?);
            }
        }
        ItemQuality::High => {
            data.low_high_graphic_bits = Some(cursor.read_bits::<u8>(3)?);
        }
        ItemQuality::Magic => {
            data.magic_prefix = Some(cursor.read_bits::<u16>(11)?);
            data.magic_suffix = Some(cursor.read_bits::<u16>(11)?);
        }
        ItemQuality::Set => {
            data.unique_id = Some(cursor.read_bits::<u16>(12)?);
        }
        ItemQuality::Rare | ItemQuality::Crafted => {
            data.rare_name_1 = Some(cursor.read_bits::<u8>(8)?);
            data.rare_name_2 = Some(cursor.read_bits::<u8>(8)?);
            for affix in &mut data.rare_affixes {
                if cursor.read_bit()? {
                    *affix = Some(cursor.read_bits::<u16>(11)?);
                }
            }
        }
        ItemQuality::Unique => {
            data.unique_id = Some(cursor.read_bits::<u16>(12)?);
        }
    }

    if header.is_runeword {
        data.runeword_id = Some(cursor.read_bits::<u16>(16)?);
    }
    if header.is_personalized {
        let bits_per_char = if save_version >= 0x62 { 8 } else { 7 };
        let mut name = String::new();
        for _ in 0..16 {
            let ch = cursor.read_bits::<u8>(bits_per_char)?;
            if ch == 0 {
                break;
            }
            name.push(char::from(ch));
        }
        data.personalized_player_name = Some(name);
    }

    let has_realm_data = cursor.read_bit()?;
    if has_realm_data {
        let realm_data_count = if save_version > 96 {
            4
        } else if save_version > 93 {
            3
        } else {
            2
        };
        for _ in 0..realm_data_count {
            auxiliary.realm_data.push(cursor.read_bits::<u32>(32)?);
        }
    }

    if trimmed_code == "gld" {
        let _not_gold = cursor.read_bit()?;
        data.quantity = Some(cursor.read_bits::<u32>(12)?);
    }

    let template =
        crate::data::repository::legacy_item_template(trimmed_code).ok_or_else(|| {
            cursor.fail(ParsingError::UnexpectedValue {
                field: "D2R v3 item template".to_string(),
                value: trimmed_code.to_string(),
                reason: "The type-specific field layout is unavailable for this item code"
                    .to_string(),
            })
        })?;

    if template.is_armor {
        let width = d2r_v3_stat_width(cursor, 31)?;
        let raw = cursor.read_bits::<u32>(width)?;
        let save_add = crate::domain::stats::stat_save_add(31).ok_or_else(|| {
            cursor.fail(ParsingError::UnexpectedValue {
                field: "D2R v3 armor defense".to_string(),
                value: raw.to_string(),
                reason: "Missing defense SaveAdd metadata".to_string(),
            })
        })?;
        let logical = i64::from(raw) - i64::from(save_add);
        data.defense = Some(u32::try_from(logical).map_err(|_| {
            cursor.fail(ParsingError::UnexpectedValue {
                field: "D2R v3 armor defense".to_string(),
                value: raw.to_string(),
                reason: "Stored defense minus SaveAdd is outside the logical u32 range".to_string(),
            })
        })?);
    }
    if template.has_durability || template.is_weapon {
        let max_width = d2r_v3_stat_width(cursor, 73)?;
        let current_width = d2r_v3_stat_width(cursor, 72)?;
        let maximum = cursor.read_bits::<u32>(max_width)?;
        data.max_durability = Some(maximum);
        if maximum > 0 {
            data.current_durability = Some(cursor.read_bits::<u32>(current_width)?);
        }
    }

    if save_version >= 0x69 {
        auxiliary.has_quantity = cursor.read_bit()?;
        if auxiliary.has_quantity {
            data.quantity = Some(cursor.read_bits::<u32>(9)?);
        }
    } else if template.is_stackable {
        data.quantity = Some(cursor.read_bits::<u32>(9)?);
    }
    if header.is_socketed {
        let width = d2r_v3_stat_width(cursor, 194)?;
        data.sockets = Some(cursor.read_bits::<u8>(width)?);
    }
    if quality == ItemQuality::Set {
        for flag in &mut data.d2r_v3_set_property_flags {
            *flag = cursor.read_bit()?;
        }
        auxiliary.set_property_flags = Some(data.d2r_v3_set_property_flags);
        data.set_list_count = data
            .d2r_v3_set_property_flags
            .iter()
            .filter(|flag| **flag)
            .count() as u8;
    }

    data.d2r_v3_auxiliary = Some(auxiliary);
    cursor.end_segment();
    Ok(data)
}

fn d2r_v3_stat_width<R: BitRead>(cursor: &BitCursor<R>, stat_id: u32) -> ParsingResult<u32> {
    crate::domain::stats::stat_save_bits(stat_id).ok_or_else(|| {
        cursor.fail(ParsingError::UnexpectedValue {
            field: "D2R v3 stat-cost width".to_string(),
            value: stat_id.to_string(),
            reason: "The required physical field width is missing from the D2R stat-cost table"
                .to_string(),
        })
    })
}

pub(in crate::domain::item) fn read_legacy_extended_payload<
    R: BitRead,
    P: LegacyExtendedPayloadReadPolicy,
>(
    cursor: &mut BitCursor<R>,
    code: &str,
    header: &ItemHeader,
    alpha_mode: bool,
    axiom: &P,
) -> ParsingResult<ExtendedStatsData> {
    cursor.begin_segment(ItemSegmentType::ExtendedStats);
    let mut data = ExtendedStatsData::default();
    let trimmed_code = code.trim();
    let code_policy = legacy_extended_payload_code_policy(trimmed_code);
    let version = header.version;
    let is_compact = header.is_compact;
    let is_socketed_flag = header.is_socketed;
    let is_runeword = axiom.is_runeword(header.flags);
    let is_personalized = header.is_personalized;
    let is_fragment = axiom.header_is_alpha(version, alpha_mode)
        && (version == 5 || version == 2 || version == 1)
        && ((header.flags & (1 << 26)) != 0 || (header.flags & (1 << 27)) != 0);
    let mut is_alpha_early_exit =
        axiom.header_is_alpha(version, alpha_mode) && axiom.is_extended_stats_early_exit(version);
    if code_policy.continues_after_alpha_early_exit {
        is_alpha_early_exit = false;
    }
    let soft_truncate_on_limit = alpha_mode && code_policy.soft_truncate_on_io_boundary;
    trait IntoParsingError {
        fn into_parsing_error(self) -> crate::error::ParsingError;
    }
    impl IntoParsingError for crate::error::ParsingFailure {
        fn into_parsing_error(self) -> crate::error::ParsingError {
            self.error
        }
    }
    impl IntoParsingError for std::io::Error {
        fn into_parsing_error(self) -> crate::error::ParsingError {
            crate::error::ParsingError::Io(self.to_string())
        }
    }

    macro_rules! read_or_truncate {
        ($cursor:expr, $label:expr, direct $req_bits:expr, $expr:expr) => {{
            let pos = $cursor.pos();
            let rem = $cursor.remaining();
            let lim = $cursor.limit();
            let req_b: u32 = $req_bits as u32;
            let ctx_label = format!(
                "ExtendedStats:{} [requested_width_bits={}, pos={}, remaining_bits={}, limit={:?}]",
                $label, req_b, pos, rem, lim
            );
            $cursor.push_context(&ctx_label);
            let res = $expr;
            match res {
                Ok(value) => {
                    $cursor.pop_context();
                    value
                }
                Err(e) => {
                    let err_kind = e.into_parsing_error();
                    let failure = $cursor.fail(err_kind);
                    $cursor.pop_context();
                    if soft_truncate_on_limit
                        && matches!(
                            &failure.error,
                            crate::error::ParsingError::Io(msg)
                                if msg.contains("Bit limit exceeded")
                                    || msg.contains("failed to fill whole buffer")
                                    || msg.contains("end of bitstream")
                        )
                    {
                        $cursor.end_segment();
                        return Ok(data);
                    }
                    return Err(failure);
                }
            }
        }};
        ($cursor:expr, $label:expr, composite, $expr:expr) => {{
            let pos = $cursor.pos();
            let rem = $cursor.remaining();
            let lim = $cursor.limit();
            let ctx_label = format!(
                "ExtendedStats:{} [requested_width_bits=unknown, pos={}, remaining_bits={}, limit={:?}]",
                $label, pos, rem, lim
            );
            $cursor.push_context(&ctx_label);
            let res = $expr;
            match res {
                Ok(value) => {
                    $cursor.pop_context();
                    value
                }
                Err(e) => {
                    let err_kind = e.into_parsing_error();
                    let failure = $cursor.fail(err_kind);
                    $cursor.pop_context();
                    if soft_truncate_on_limit
                        && matches!(
                            &failure.error,
                            crate::error::ParsingError::Io(msg)
                                if msg.contains("Bit limit exceeded")
                                    || msg.contains("failed to fill whole buffer")
                                    || msg.contains("end of bitstream")
                        )
                    {
                        $cursor.end_segment();
                        return Ok(data);
                    }
                    return Err(failure);
                }
            }
        }};
    }
    if axiom.is_alpha() {
        if axiom.header_is_alpha(version, alpha_mode)
            && axiom.is_summary_item(version, trimmed_code)
        {
            // Alpha v105 (Slice 2): Summary items use unified 16-bit ID,
            // bits already consumed in parse_item_body.
            data.id = Some(0);
            cursor.end_segment();
            return Ok(data);
        }

        if !is_compact {
            let q_bits = axiom.quality_bits(true) as u32;
            let quality_raw = read_or_truncate!(cursor, "alpha_quality", direct q_bits, cursor.read_bits::<u8>(q_bits));
            let quality = ItemQuality::from(quality_raw);
            data.alpha_quality_raw = Some(quality_raw);
            data.quality = Some(quality);

            let is_authority_runeword = alpha_mode && code_policy.authority_container;
            if axiom.has_v5_runeword_extra(version)
                && (is_runeword
                    || is_fragment
                    || axiom.header_is_v105_shadow(version, alpha_mode, header.flags, Some(&code)))
            {
                if is_authority_runeword {
                    data.id = Some(0);
                } else {
                    let v5_bits = axiom.v5_runeword_extra_bits() as u32;
                    data.v5_runeword_extra = Some(read_or_truncate!(
                        cursor,
                        "AlphaV5RunewordExtra",
                        direct v5_bits,
                        cursor.read_bits::<u8>(v5_bits)
                    ));
                    data.id = Some(0);
                }
            // Mirror the emitter: alpha items only carry id/level here on v0/v2.
            } else if !alpha_mode || version == 0 || version == 2 {
                let id_bits = axiom.item_id_bits() as u32;
                data.id = Some(read_or_truncate!(
                    cursor,
                    "id",
                    direct id_bits,
                    cursor.read_bits::<u32>(id_bits)
                ));
                let lvl_bits = axiom.item_level_bits() as u32;
                data.level = Some(read_or_truncate!(
                    cursor,
                    "level",
                    direct lvl_bits,
                    cursor.read_bits::<u8>(lvl_bits)
                ));
            }
        } else {
            data.id = Some(0);
        }
    } else {
        let is_nested = axiom.in_nested_recovery();
        if is_nested && is_compact {
            data.id = Some(0);
        } else {
            let id_bits = axiom.item_id_bits() as u32;
            data.id = Some(read_or_truncate!(
                cursor,
                "id",
                direct id_bits,
                cursor.read_bits::<u32>(id_bits)
            ));
            let lvl_bits = axiom.item_level_bits() as u32;
            data.level = Some(read_or_truncate!(
                cursor,
                "level",
                direct lvl_bits,
                cursor.read_bits::<u8>(lvl_bits)
            ));
            let q_bits = axiom.quality_bits(false) as u32;
            let quality_raw = read_or_truncate!(
                cursor,
                "quality",
                direct q_bits,
                cursor.read_bits::<u8>(q_bits)
            );
            data.quality = Some(ItemQuality::from(quality_raw));
        }
    }
    if is_alpha_early_exit {
        cursor.end_segment();
        return Ok(data);
    }

    let template = crate::data::repository::legacy_item_template(trimmed_code);
    data.has_multiple_graphics = code_policy.has_multiple_graphics;
    data.has_class_specific_data = false;

    if data.has_multiple_graphics {
        let mg_bits = axiom.multi_graphics_bits() as u32;
        data.multi_graphics_bits = Some(read_or_truncate!(
            cursor,
            "multi_graphics_bits",
            direct mg_bits,
            cursor.read_bits::<u8>(mg_bits)
        ) as u8);
    }
    if data.has_class_specific_data {
        let cs_bits = axiom.class_specific_bits() as u32;
        data.class_specific_bits = Some(read_or_truncate!(
            cursor,
            "class_specific_bits",
            direct cs_bits,
            cursor.read_bits::<u16>(cs_bits)
        ) as u16);
    }
    let quality_val = data.quality.unwrap_or(ItemQuality::Normal);
    match quality_val {
        ItemQuality::Low | ItemQuality::High => {
            let lh_bits = axiom.low_high_graphic_bits() as u32;
            data.low_high_graphic_bits = Some(read_or_truncate!(
                cursor,
                "low_high_graphic_bits",
                direct lh_bits,
                cursor.read_bits::<u8>(lh_bits)
            ) as u8);
        }
        ItemQuality::Magic => {
            let seg = read_or_truncate!(
                cursor,
                "MagicAffixSegment",
                composite,
                MagicAffixSegment::parse(cursor)
            );
            data.magic_prefix = seg.prefix;
            data.magic_suffix = seg.suffix;
        }
        ItemQuality::Rare | ItemQuality::Crafted => {
            let seg = read_or_truncate!(
                cursor,
                "RareAffixSegment",
                composite,
                RareAffixSegment::parse(cursor)
            );
            data.rare_name_1 = seg.names[0];
            data.rare_name_2 = seg.names[1];
            data.rare_affixes = seg.affixes;
        }
        ItemQuality::Set | ItemQuality::Unique => {
            let seg = read_or_truncate!(
                cursor,
                "UniqueAffixSegment",
                composite,
                UniqueAffixSegment::parse(cursor)
            );
            let uid = seg.unique_id.unwrap_or(0);
            if alpha_mode {
                data.alpha_unique_id_raw = Some(uid);
            }
            data.unique_id = Some(uid);
        }
        _ => {}
    }
    let trace_alpha = alpha_mode && crate::item::item_trace_enabled();
    if is_runeword && !is_fragment && version != 5 {
        let rw_id_bits = axiom.runeword_id_bits() as u32;
        data.runeword_id = Some(read_or_truncate!(
            cursor,
            "runeword_id",
            direct rw_id_bits,
            cursor.read_bits::<u16>(rw_id_bits)
        ) as u16);
        let rw_lvl_bits = axiom.runeword_level_bits() as u32;
        data.runeword_level = Some(read_or_truncate!(
            cursor,
            "runeword_level",
            direct rw_lvl_bits,
            cursor.read_bits::<u8>(rw_lvl_bits)
        ) as u8);
        if trace_alpha && trimmed_code == "wa2" {
            eprintln!(
                "[runeword-parse] wa2 runeword_id={:?} runeword_level={:?} pos={}",
                data.runeword_id,
                data.runeword_level,
                cursor.pos()
            );
        }
    }
    if is_personalized {
        if alpha_mode
            && axiom.needs_player_name_byte_alignment(version)
            && axiom.player_name_alignment_required()
        {
            read_or_truncate!(cursor, "byte_align", composite, cursor.byte_align());
        }
        data.personalized_player_name = Some(read_or_truncate!(
            cursor,
            "personalized_player_name",
            composite,
            read_player_name(
                cursor,
                alpha_mode && axiom.is_player_name_alpha_style(version)
            )
        ));
    }
    if code_policy.has_teleport_bits {
        let tp_bits = axiom.teleport_bits() as u32;
        data.tbk_ibk_teleport = Some(read_or_truncate!(
            cursor,
            "tbk_ibk_teleport",
            direct tp_bits,
            cursor.read_bits::<u8>(tp_bits)
        ) as u8);
    }

    let is_ear = !alpha_mode && (header.flags & (1 << 24)) != 0;
    if !is_ear && !alpha_mode && header.version != 5 && header.version != 7 {
        let has_realm_data =
            read_or_truncate!(cursor, "has_realm_data", direct 1, cursor.read_bit());
        if has_realm_data {
            let _realm_data_1 =
                read_or_truncate!(cursor, "realm_data_1", direct 32, cursor.read_bits::<u32>(32));
            let _realm_data_2 =
                read_or_truncate!(cursor, "realm_data_2", direct 32, cursor.read_bits::<u32>(32));
            let _realm_data_3 =
                read_or_truncate!(cursor, "realm_data_3", direct 32, cursor.read_bits::<u32>(32));
        }
    }
    data.timestamp_flag = read_or_truncate!(cursor, "timestamp_flag", direct 1, cursor.read_bit());
    if trace_alpha && trimmed_code == "wa2" {
        eprintln!(
            "[timestamp-parse] wa2 timestamp_flag={} pos={}",
            data.timestamp_flag,
            cursor.pos()
        );
    }
    let (reads_defense, mut reads_durability, reads_quantity) = if let Some(template) = template {
        (
            template.is_armor,
            template.has_durability,
            template.is_stackable,
        )
    } else {
        let is_scroll = code_policy.scroll_quantity_fallback;
        let is_authority_xrs = alpha_mode && code_policy.authority_container;
        let armor_like_unknown =
            data.has_class_specific_data || trimmed_code.contains(' ') || is_authority_xrs;
        (armor_like_unknown, armor_like_unknown, is_scroll)
    };
    if code_policy.force_durability {
        reads_durability = true;
    }
    if reads_defense && axiom.reads_defense() {
        let def_bits = axiom.stat_bits(31) as u32;
        data.defense = Some(read_or_truncate!(
            cursor,
            "defense",
            direct def_bits,
            cursor.read_bits::<u32>(def_bits)
        ));
    }
    if reads_durability && axiom.reads_durability() {
        let max_bits = if alpha_mode {
            8
        } else {
            axiom.stat_bits(73) as u32
        };
        let cur_bits = if alpha_mode {
            8
        } else {
            axiom.stat_bits(72) as u32
        };
        let m_dur = read_or_truncate!(cursor, "max_durability", direct max_bits, cursor.read_bits::<u32>(max_bits));
        data.max_durability = Some(m_dur);
        if m_dur > 0 {
            data.current_durability = Some(
                read_or_truncate!(cursor, "current_durability", direct cur_bits, cursor.read_bits::<u32>(cur_bits)),
            );
            let _extra = read_or_truncate!(cursor, "durability_extra", direct 1, cursor.read_bit());
        }
    }
    if reads_quantity && axiom.reads_quantity() {
        let q_bits = axiom.quantity_bits() as u32;
        data.quantity = Some(read_or_truncate!(
            cursor,
            "quantity",
            direct q_bits,
            cursor.read_bits::<u32>(q_bits)
        ));
    }
    if is_socketed_flag {
        let sock_bits = axiom.socket_bits() as u32;
        data.sockets = Some(
            read_or_truncate!(cursor, "sockets", direct sock_bits, cursor.read_bits::<u8>(sock_bits))
                as u8,
        );
    }
    if quality_val == ItemQuality::Set {
        let set_bits = axiom.set_list_bits() as u32;
        let val = read_or_truncate!(cursor, "set_list_val", direct set_bits, cursor.read_bits::<u8>(set_bits));
        data.alpha_set_list_val = Some(val);
        data.set_list_count = match val {
            1 => 1,
            3 => 2,
            7 => 3,
            15 => 4,
            31 => 5,
            _ => 0,
        };
    }
    cursor.end_segment();
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::quality::ItemQuality;
    use crate::domain::stats::StatsAxiom;
    use bitstream_io::{BitReader, LittleEndian};
    use d2r_macros::hypothesis_test;
    use std::io::Cursor;

    fn state<R: BitRead>(cursor: &BitCursor<R>) -> String {
        format!(
            "{:?}",
            (
                cursor.pos(),
                cursor.limit(),
                cursor.remaining(),
                cursor.recorded_bits(),
                cursor.segments(),
                cursor.active_segments(),
                cursor.context_stack(),
                cursor.operation_events(),
                cursor.alpha_quality,
            )
        )
    }

    #[hypothesis_test]
    fn extended_payload_execution_preserves_data_and_cursor_effects() {
        let buffers = [vec![], vec![0], vec![0; 8], vec![0; 64], vec![255; 64]];
        let headers = [
            ItemHeader::default(),
            ItemHeader {
                version: 5,
                save_is_alpha: true,
                ..Default::default()
            },
            ItemHeader {
                version: 0x60,
                is_compact: true,
                ..Default::default()
            },
        ];
        let mut rows = 0;
        let mut successful_reads = 0;
        let mut failed_reads = 0;
        let mut zero_read_plans = 0;
        for header in &headers {
            for code in ["hp1", "rin", "cap", "xrs"] {
                for bytes in &buffers {
                    for start in [0, 3] {
                        for limit in [None, Some(0), Some(24)] {
                            for case in 0..8 {
                                let mut actual = BitCursor::new(BitReader::endian(
                                    Cursor::new(bytes),
                                    LittleEndian,
                                ));
                                let mut expected = BitCursor::new(BitReader::endian(
                                    Cursor::new(bytes),
                                    LittleEndian,
                                ));
                                for cursor in [&mut actual, &mut expected] {
                                    cursor.set_trace(true);
                                    cursor.push_context("enclosing payload owner");
                                    cursor.read_bits_as_vec_up_to(start);
                                    if let Some(limit) = limit {
                                        cursor.set_limit(limit);
                                    }
                                }
                                let before = state(&actual);
                                let auxiliary = if case == 1 {
                                    Some(D2rV3AuxiliaryData {
                                        save_version: 0x69,
                                        stored_in: Some(4),
                                        filled_sockets: 3,
                                        quest_difficulty: Some(2),
                                        realm_data: vec![0, 17, u32::MAX],
                                        alignment_padding: Some(vec![true, false, true]),
                                        ..Default::default()
                                    })
                                } else {
                                    None
                                };
                                let axiom = StatsAxiom::new(
                                    header.version,
                                    ItemQuality::Normal,
                                    header.save_is_alpha,
                                )
                                .with_compact(header.is_compact)
                                .with_code(code);
                                let (plan, reference) = match case {
                                    0 | 1 => (
                                        ExtendedPayloadReadPlan::V3Compact(auxiliary.clone()),
                                        Ok(ExtendedStatsData {
                                            d2r_v3_auxiliary: auxiliary,
                                            ..Default::default()
                                        }),
                                    ),
                                    2..=5 => {
                                        let save_version = [0, 0x60, 0x69, u32::MAX][case - 2];
                                        (
                                            ExtendedPayloadReadPlan::V3 { save_version },
                                            ExtendedStatsData::read_d2r_v3_from_cursor(
                                                &mut expected,
                                                code,
                                                header,
                                                save_version,
                                            ),
                                        )
                                    }
                                    6 => (
                                        ExtendedPayloadReadPlan::Legacy,
                                        ExtendedStatsData::read_from_cursor(
                                            &mut expected,
                                            code,
                                            header,
                                            header.save_is_alpha,
                                            &axiom,
                                        ),
                                    ),
                                    7 => (
                                        ExtendedPayloadReadPlan::Omitted,
                                        Ok(ExtendedStatsData::default()),
                                    ),
                                    _ => unreachable!(),
                                };
                                let result = plan.read(&mut actual, code, header, &axiom);
                                assert_eq!(
                                    format!("{result:?}"),
                                    format!("{reference:?}"),
                                    "case={case}, code={code}, bytes={}, start={start}, limit={limit:?}",
                                    bytes.len()
                                );
                                assert_eq!(state(&actual), state(&expected));
                                if matches!(case, 0 | 1 | 7) {
                                    assert_eq!(state(&actual), before);
                                    zero_read_plans += 1;
                                } else if result.is_ok() {
                                    successful_reads += 1;
                                } else {
                                    failed_reads += 1;
                                }
                                rows += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(rows, 2880);
        assert_eq!(zero_read_plans, 1080);
        assert!(successful_reads > 0);
        assert!(failed_reads > 0);
        println!(
            "payload effects: rows={rows}, zero_read={zero_read_plans}, successes={successful_reads}, errors={failed_reads}"
        );
    }

    #[derive(Clone, Copy)]
    struct LegacyReaderCase {
        header_version: u8,
        stats_version: u8,
        reader_alpha: bool,
        stats_alpha: bool,
        compact: bool,
        nested: bool,
        flags: u32,
        limit: Option<u64>,
        empty: bool,
        fill: u8,
        personalized: bool,
    }

    #[hypothesis_test]
    fn legacy_extended_payload_reader_observations() {
        let base = LegacyReaderCase {
            header_version: 5,
            stats_version: 5,
            reader_alpha: true,
            stats_alpha: true,
            compact: false,
            nested: false,
            flags: 0,
            limit: None,
            empty: false,
            fill: 0,
            personalized: false,
        };
        let mut cases = Vec::new();
        for (header_version, stats_version) in [
            (0, 0),
            (2, 2),
            (5, 5),
            (6, 6),
            (7, 7),
            (96, 96),
            (255, 255),
            (6, 5),
            (5, 96),
        ] {
            for (reader_alpha, stats_alpha) in
                [(false, false), (true, true), (true, false), (false, true)]
            {
                cases.push(LegacyReaderCase {
                    header_version,
                    stats_version,
                    reader_alpha,
                    stats_alpha,
                    ..base
                });
            }
        }
        for version in [6, 7] {
            for limit in [0, 3, 8, 24, 64, 128] {
                cases.push(LegacyReaderCase {
                    header_version: version,
                    stats_version: version,
                    limit: Some(limit),
                    ..base
                });
            }
        }
        for version in [0, 2, 5, 6, 7, 96] {
            for nested in [false, true] {
                cases.push(LegacyReaderCase {
                    header_version: version,
                    stats_version: version,
                    compact: true,
                    nested,
                    reader_alpha: false,
                    stats_alpha: false,
                    ..base
                });
            }
        }
        for version in [1, 5, 6, 7] {
            for flags in [1 << 26, 1 << 27] {
                cases.push(LegacyReaderCase {
                    header_version: version,
                    stats_version: version,
                    flags,
                    ..base
                });
            }
        }
        for (version, alpha) in [(96, false), (5, true), (6, true)] {
            cases.push(LegacyReaderCase {
                header_version: version,
                stats_version: version,
                reader_alpha: alpha,
                stats_alpha: alpha,
                empty: true,
                ..base
            });
        }
        for version in [0, 2, 5, 6, 7, 96] {
            cases.push(LegacyReaderCase {
                header_version: version,
                stats_version: version,
                personalized: true,
                ..base
            });
        }
        for (version, reader_alpha, stats_alpha) in [
            (0, false, false),
            (5, true, true),
            (6, true, true),
            (96, true, false),
        ] {
            cases.push(LegacyReaderCase {
                header_version: version,
                stats_version: version,
                reader_alpha,
                stats_alpha,
                fill: 255,
                ..base
            });
        }
        assert_eq!(cases.len(), 81);
        let codes = [
            "jav", "buc", "ucb8", "xrs", "c8xr", "rhd", "wa2", "bst", "rin", "amu", "cm1", "cm2",
            "cm3", "tbk", "ibk", "tsc", "isc", "hp1", "cap", "pik ", "unlisted",
        ];
        let previous_nested = crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.get());
        let mut rows = Vec::new();
        let mut successes = 0;
        let mut errors = 0;
        for code in codes {
            for raw_code in [
                code.to_string(),
                format!(" \t{code}\u{2003}"),
                format!("{code}\0"),
            ] {
                for (case_index, case) in cases.iter().enumerate() {
                    crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(case.nested));
                    let bytes = vec![case.fill; if case.empty { 0 } else { 128 }];
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.set_trace(true);
                    cursor.push_context("legacy payload witness");
                    if let Some(limit) = case.limit {
                        cursor.set_limit(limit);
                    }
                    let header = ItemHeader {
                        version: case.header_version,
                        is_compact: case.compact,
                        is_socketed: true,
                        is_personalized: case.personalized,
                        flags: case.flags,
                        save_is_alpha: case.reader_alpha,
                        ..Default::default()
                    };
                    let axiom =
                        StatsAxiom::new(case.stats_version, ItemQuality::Normal, case.stats_alpha)
                            .with_compact(case.compact)
                            .with_code(&raw_code);
                    let result = ExtendedStatsData::read_from_cursor(
                        &mut cursor,
                        &raw_code,
                        &header,
                        case.reader_alpha,
                        &axiom,
                    );
                    if result.is_ok() {
                        successes += 1;
                    } else {
                        errors += 1;
                    }
                    rows.push(serde_json::json!({
                        "code": raw_code,
                        "case": case_index,
                        "input": {
                            "header_version": case.header_version, "stats_version": case.stats_version,
                            "reader_alpha": case.reader_alpha, "stats_alpha": case.stats_alpha,
                            "compact": case.compact, "nested": case.nested,
                            "flags": case.flags, "limit": case.limit,
                            "bytes": bytes.len(), "fill": case.fill, "personalized": case.personalized,
                            "socketed": true
                        },
                        "result": format!("{result:?}"),
                        "cursor": state(&cursor)
                    }));
                }
            }
        }
        crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(previous_nested));
        assert_eq!(rows.len(), 5103);
        assert!(successes > 0 && errors > 0);
        assert_eq!(successes + errors, rows.len());
        if let Some(path) = std::env::var_os("D2R_LEGACY_PAYLOAD_POLICY_OUTPUT") {
            let output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            serde_json::to_writer_pretty(
                output,
                &serde_json::json!({
                    "schema": 1, "cases": rows.len(), "reader_cases": cases.len(),
                    "codes": codes.len(), "successes": successes, "errors": errors, "rows": rows
                }),
            )
            .unwrap();
        }
    }

    #[d2r_macros::hypothesis_test]
    fn legacy_codec_queries_are_lazy_without_forensic_provider() {
        struct RecordingPolicy(std::cell::RefCell<Vec<&'static str>>);
        impl LegacyExtendedPayloadReadPolicy for RecordingPolicy {
            fn class_specific_bits(&self) -> u32 {
                panic!("unexpected query: class_specific_bits")
            }
            fn has_v5_runeword_extra(&self, _version: u8) -> bool {
                panic!("unexpected query: has_v5_runeword_extra")
            }
            fn is_extended_stats_early_exit(&self, _version: u8) -> bool {
                panic!("unexpected query: is_extended_stats_early_exit")
            }
            fn is_player_name_alpha_style(&self, _version: u8) -> bool {
                panic!("unexpected query: is_player_name_alpha_style")
            }
            fn is_summary_item(&self, _version: u8, _code: &str) -> bool {
                panic!("unexpected query: is_summary_item")
            }
            fn item_id_bits(&self) -> u32 {
                self.0.borrow_mut().push("item_id_bits");
                32
            }
            fn item_level_bits(&self) -> u32 {
                panic!("unexpected query: item_level_bits")
            }
            fn low_high_graphic_bits(&self) -> u32 {
                panic!("unexpected query: low_high_graphic_bits")
            }
            fn multi_graphics_bits(&self) -> u32 {
                panic!("unexpected query: multi_graphics_bits")
            }
            fn needs_player_name_byte_alignment(&self, _version: u8) -> bool {
                panic!("unexpected query: needs_player_name_byte_alignment")
            }
            fn quality_bits(&self, _is_alpha: bool) -> u32 {
                panic!("unexpected query: quality_bits")
            }
            fn quantity_bits(&self) -> u32 {
                panic!("unexpected query: quantity_bits")
            }
            fn runeword_id_bits(&self) -> u32 {
                panic!("unexpected query: runeword_id_bits")
            }
            fn runeword_level_bits(&self) -> u32 {
                panic!("unexpected query: runeword_level_bits")
            }
            fn set_list_bits(&self) -> u32 {
                panic!("unexpected query: set_list_bits")
            }
            fn socket_bits(&self) -> u32 {
                panic!("unexpected query: socket_bits")
            }
            fn stat_bits(&self, _stat_id: u32) -> u32 {
                panic!("unexpected query: stat_bits")
            }
            fn teleport_bits(&self) -> u32 {
                panic!("unexpected query: teleport_bits")
            }
            fn v5_runeword_extra_bits(&self) -> u32 {
                panic!("unexpected query: v5_runeword_extra_bits")
            }
            fn is_alpha(&self) -> bool {
                self.0.borrow_mut().push("stats_alpha");
                false
            }
            fn is_runeword(&self, _flags: u32) -> bool {
                self.0.borrow_mut().push("runeword");
                false
            }
            fn reads_defense(&self) -> bool {
                panic!("unexpected query: reads_defense")
            }
            fn reads_durability(&self) -> bool {
                panic!("unexpected query: reads_durability")
            }
            fn reads_quantity(&self) -> bool {
                panic!("unexpected query: reads_quantity")
            }
            fn header_is_alpha(&self, _version: u8, _alpha_mode: bool) -> bool {
                self.0.borrow_mut().push("header_alpha");
                false
            }
            fn header_is_v105_shadow(
                &self,
                _version: u8,
                _alpha_mode: bool,
                _flags: u32,
                _code_hint: Option<&str>,
            ) -> bool {
                panic!("unexpected query: header_is_v105_shadow")
            }
            fn in_nested_recovery(&self) -> bool {
                self.0.borrow_mut().push("nested");
                false
            }
            fn player_name_alignment_required(&self) -> bool {
                panic!("unexpected query: player_name_alignment_required")
            }
        }
        let policy = RecordingPolicy(std::cell::RefCell::new(Vec::new()));
        let bytes: [u8; 0] = [];
        let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
        cursor.set_trace(true);
        cursor.set_limit(0);
        cursor.push_context("policy probe");
        let result = read_legacy_extended_payload(
            &mut cursor,
            "hp1",
            &ItemHeader::default(),
            false,
            &policy,
        );
        assert!(result.is_err());
        assert_eq!(cursor.pos(), 0);
        assert_eq!(cursor.get_context_count(), 2);
        assert_eq!(
            *policy.0.borrow(),
            [
                "runeword",
                "header_alpha",
                "header_alpha",
                "stats_alpha",
                "nested",
                "item_id_bits"
            ]
        );
    }

    #[hypothesis_test]
    fn v3_extended_payload_reader_observations() {
        fn observe(
            rows: &mut Vec<serde_json::Value>,
            family: &str,
            code: &str,
            quality: u8,
            version: u32,
            pattern: u8,
            prefix: u32,
            limit: Option<u64>,
        ) {
            // Only the fixed quality nibble is selected; all other bits are arbitrary inputs.
            let mut bytes: Vec<u8> = (0..192)
                .map(|i| match pattern {
                    0 => 0,
                    1 => 255,
                    _ => (i as u8).wrapping_mul(37).wrapping_add(91),
                })
                .collect();
            for bit in 0..4 {
                let offset = usize::try_from(prefix + 42 + bit).unwrap();
                let mask = 1 << (offset % 8);
                bytes[offset / 8] =
                    (bytes[offset / 8] & !mask) | (((quality >> bit) & 1) << (offset % 8));
            }
            let header = ItemHeader {
                is_socketed: pattern != 0,
                is_runeword: pattern != 0,
                is_personalized: pattern == 1,
                ..Default::default()
            };
            let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            cursor.set_trace(true);
            cursor.push_context("V3 payload witness");
            cursor.read_bits_as_vec_up_to(prefix);
            if let Some(limit) = limit {
                cursor.set_limit(limit);
            }
            let result =
                ExtendedStatsData::read_d2r_v3_from_cursor(&mut cursor, code, &header, version);
            let before_following = state(&cursor);
            let following = cursor.read_bits::<u8>(5);
            rows.push(serde_json::json!({
                "family": family, "code": code, "quality": quality, "version": version,
                "pattern": pattern, "prefix": prefix, "limit": limit,
                "success": result.is_ok(), "result": format!("{result:?}"),
                "cursor": before_following, "following": format!("{following:?}"),
                "cursor_after_following": state(&cursor)
            }));
        }
        let codes = [
            "rin", "amu", "cm1", "cm2", "cm3", "tsc", "isc", "tbk", "ibk", "gld", "cap", "hax",
            "jav", "aqv", "hp1", "jew", "r01", "unlisted",
        ];
        let mut rows = Vec::new();
        for code in codes {
            for quality in 1..=8 {
                for version in [93, 94, 96, 97, 98, 105] {
                    for pattern in 0..3 {
                        for prefix in [0, 3] {
                            observe(
                                &mut rows, "matrix", code, quality, version, pattern, prefix, None,
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(rows.len(), 5184);
        for code in ["rin", "cap", "hax"] {
            for quality in 1..=8 {
                for pattern in 0..2 {
                    for limit in [
                        0, 2, 3, 34, 35, 41, 42, 45, 46, 47, 50, 58, 64, 75, 96, 128, 256, 512,
                    ] {
                        for prefix in [0, 3] {
                            observe(
                                &mut rows,
                                "truncated",
                                code,
                                quality,
                                105,
                                pattern,
                                prefix,
                                Some(limit),
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(rows.len(), 6912);
        for code in ["rin", "cap", "gld"] {
            for raw_code in [format!(" \t{code}\u{2003}"), format!("{code}\0")] {
                for quality in 1..=8 {
                    for version in [97, 105] {
                        for pattern in 0..3 {
                            observe(
                                &mut rows,
                                "code-boundary",
                                &raw_code,
                                quality,
                                version,
                                pattern,
                                0,
                                None,
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(rows.len(), 7200);
        let successes = rows.iter().filter(|r| r["success"] == true).count();
        let errors = rows.len() - successes;
        assert!(successes > 0 && errors > 0);
        if let Some(path) = std::env::var_os("D2R_V3_PAYLOAD_OUTPUT") {
            let output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            serde_json::to_writer_pretty(
                output,
                &serde_json::json!({
                    "schema": 1, "cases": rows.len(), "codes": codes.len(),
                    "successes": successes, "errors": errors, "rows": rows
                }),
            )
            .unwrap();
        }
    }
}
