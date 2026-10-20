use crate::data::bit_cursor::BitCursor;
use crate::domain::header::entity::ItemHeader;
use crate::domain::item::entity::{D2rV3AuxiliaryData, ExtendedStatsData};
use crate::domain::stats::StatsAxiom;
use crate::error::ParsingResult;
use bitstream_io::BitRead;

/// Executes the payload representation selected by the enclosing item parser.
/// Recovery and item classification remain outside this codec interface.
pub(in crate::domain::item) enum ExtendedPayloadReadPlan {
    V3Compact(Option<D2rV3AuxiliaryData>),
    V3 { save_version: u32 },
    Legacy,
    Omitted,
}

impl ExtendedPayloadReadPlan {
    pub(in crate::domain::item) fn read<R: BitRead>(
        self,
        cursor: &mut BitCursor<R>,
        code: &str,
        header: &ItemHeader,
        axiom: &StatsAxiom,
    ) -> ParsingResult<ExtendedStatsData> {
        match self {
            Self::V3Compact(auxiliary) => Ok(ExtendedStatsData {
                d2r_v3_auxiliary: auxiliary,
                ..Default::default()
            }),
            Self::V3 { save_version } => {
                ExtendedStatsData::read_d2r_v3_from_cursor(cursor, code, header, save_version)
            }
            Self::Legacy => ExtendedStatsData::read_from_cursor(
                cursor,
                code,
                header,
                header.save_is_alpha,
                axiom,
            ),
            Self::Omitted => Ok(ExtendedStatsData::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::quality::ItemQuality;
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
}
