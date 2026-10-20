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
}
