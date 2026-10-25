//! Property-entry bit consumption with injected layout and recovery policies.
use crate::data::bit_cursor::BitCursor;
use crate::domain::item::entity::ItemBitRange;
use crate::domain::stats::{ItemProperty, StatMaturityTier};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

#[derive(Debug, Clone, Copy)]
pub struct PropertyRhythm {
    pub id_bits: u32,
    pub value_bits: Option<u32>,
    pub has_terminal_bit: bool,
    pub has_extra_terminal_bit: bool,
}

pub(crate) trait PropertyEntryPolicy {
    fn trace_entry(&self, entry_start: u64, stat_id: u32);
    fn rhythm(&self, stat_id: u32) -> PropertyRhythm;
    fn terminal_is_authority(&self) -> bool;
    fn tvs_padding_bits(&self) -> Option<u32>;
    fn map_id(&self, stat_id: u32) -> u32;
    fn omit_parameter(&self) -> bool;
    fn effective_width(
        &self,
        stat_id: u32,
        stat_save_bits: Option<u32>,
        rhythm: PropertyRhythm,
    ) -> u32;
    fn consume_paired_stats(&self) -> bool;
    fn maturity(&self, stat_id: u32, known: bool) -> StatMaturityTier;
}

pub(crate) trait PropertyEntryRecovery<R: BitRead, T> {
    fn recover(
        &mut self,
        recorder: &mut BitCursor<R>,
        stat_id: u32,
        nested_items: &mut Vec<T>,
    ) -> ParsingResult<bool>;
}

pub(crate) fn read_property_entry<R: BitRead, P: PropertyEntryPolicy, T>(
    recorder: &mut BitCursor<R>,
    policy: &P,
    recovery: &mut impl PropertyEntryRecovery<R, T>,
) -> ParsingResult<Option<(Vec<ItemProperty>, bool, bool, Vec<T>)>> {
    let entry_start = recorder.pos();
    let id_bits = u32::from(
        crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY.stat_id_bits,
    );
    let stat_id = recorder.read_bits::<u32>(id_bits)?;
    policy.trace_entry(entry_start, stat_id);

    let rhythm = policy.rhythm(stat_id);

    let id_bits = rhythm.id_bits;
    let terminator = (1u32 << id_bits) - 1;

    if stat_id == terminator {
        let is_authority_host = policy.terminal_is_authority();
        let mut term_bit = false;
        if rhythm.has_terminal_bit {
            if !is_authority_host || recorder.remaining() >= 1 {
                term_bit = recorder.read_bit()?;
            }
            if rhythm.has_extra_terminal_bit {
                if !is_authority_host || recorder.remaining() >= 1 {
                    let _extra = recorder.read_bit()?;
                }
            }
            while recorder.pos() % 8 != 0 {
                if is_authority_host && recorder.remaining() == 0 {
                    break;
                }
                let _p = recorder.read_bit()?;
            }
        }

        if let Some(tvs_bits) = policy.tvs_padding_bits() {
            if !is_authority_host || recorder.remaining() >= u64::from(tvs_bits) {
                let _tvs = recorder.read_bits::<u32>(tvs_bits)?;
            }
        }
        return Ok(Some((
            vec![ItemProperty {
                stat_id,
                raw_value: 0,
                param: 0,
                name: "terminator".to_string(),
                value: 0,
                range: ItemBitRange {
                    start: entry_start,
                    end: recorder.pos(),
                },
                is_opaque: false,
                opaque_bits: None,
                maturity: crate::domain::stats::StatMaturityTier::Verified,
            }],
            true,
            term_bit,
            Vec::new(),
        )));
    }

    let mut param = 0;
    let mut nested_items = Vec::new();
    let mapped_id = policy.map_id(stat_id);
    let stat_cost = crate::data::stat_costs::STAT_COSTS
        .iter()
        .find(|s| s.id == mapped_id);
    let suppress_authority_params = policy.omit_parameter();
    if let Some(stat) = stat_cost {
        if stat.save_param_bits > 0 && !suppress_authority_params {
            param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
        }
    }

    let effective_width = policy.effective_width(
        stat_id,
        stat_cost.map(|stat| u32::from(stat.save_bits)),
        rhythm,
    );

    let handled = recovery.recover(recorder, stat_id, &mut nested_items)?;

    let properties = super::property_value::read_property_values(
        recorder,
        super::property_value::PropertyValueLayout {
            entry_start,
            stat_id,
            param,
            effective_width,
            recovered: handled,
            stat_cost,
        },
        crate::data::generated::layout_policy::D2R_V3_PAIRED_STAT_IDS,
        || policy.consume_paired_stats(),
        |id, known| policy.maturity(id, known),
    )?;

    Ok(Some((properties, false, false, nested_items)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitReader, LittleEndian};
    use d2r_macros::hypothesis_test;
    use std::cell::RefCell;
    use std::io::Cursor;

    struct Policy<'a> {
        events: &'a RefCell<Vec<&'static str>>,
        terminal: bool,
    }

    impl PropertyEntryPolicy for Policy<'_> {
        fn trace_entry(&self, _: u64, _: u32) {
            self.events.borrow_mut().push("trace");
        }
        fn rhythm(&self, _: u32) -> PropertyRhythm {
            self.events.borrow_mut().push("rhythm");
            PropertyRhythm {
                id_bits: 9,
                value_bits: Some(5),
                has_terminal_bit: self.terminal,
                has_extra_terminal_bit: false,
            }
        }
        fn terminal_is_authority(&self) -> bool {
            self.events.borrow_mut().push("authority");
            false
        }
        fn tvs_padding_bits(&self) -> Option<u32> {
            self.events.borrow_mut().push("padding");
            None
        }
        fn map_id(&self, id: u32) -> u32 {
            self.events.borrow_mut().push("map");
            id
        }
        fn omit_parameter(&self) -> bool {
            self.events.borrow_mut().push("parameter");
            false
        }
        fn effective_width(&self, _: u32, _: Option<u32>, _: PropertyRhythm) -> u32 {
            self.events.borrow_mut().push("width");
            5
        }
        fn consume_paired_stats(&self) -> bool {
            self.events.borrow_mut().push("pairs");
            false
        }
        fn maturity(&self, _: u32, known: bool) -> StatMaturityTier {
            assert!(!known);
            self.events.borrow_mut().push("maturity");
            StatMaturityTier::Speculative
        }
    }

    struct Recovery<'a>(&'a RefCell<Vec<&'static str>>);

    impl<R: BitRead> PropertyEntryRecovery<R, ()> for Recovery<'_> {
        fn recover(
            &mut self,
            _: &mut BitCursor<R>,
            _: u32,
            _: &mut Vec<()>,
        ) -> ParsingResult<bool> {
            self.0.borrow_mut().push("recovery");
            Ok(false)
        }
    }

    #[hypothesis_test]
    fn selected_entry_accepts_non_item_ports_and_keeps_callback_order() {
        assert!(
            crate::data::stat_costs::STAT_COSTS
                .iter()
                .all(|s| s.id != 509)
        );
        let bytes = (509u16 | (7u16 << 9)).to_le_bytes();
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_limit(14);
        let events = RefCell::new(Vec::new());
        let policy = Policy {
            events: &events,
            terminal: false,
        };
        let mut recovery = Recovery(&events);
        let (properties, complete, terminal, children): (_, _, _, Vec<()>) =
            read_property_entry(&mut cursor, &policy, &mut recovery)
                .unwrap()
                .unwrap();
        assert_eq!(cursor.pos(), 14);
        assert_eq!(properties.len(), 1);
        assert_eq!((properties[0].stat_id, properties[0].raw_value), (509, 7));
        assert_eq!(
            (properties[0].range.start, properties[0].range.end),
            (0, 14)
        );
        assert!(properties[0].is_opaque);
        assert!(!complete && !terminal && children.is_empty());
        assert_eq!(
            *events.borrow(),
            [
                "trace",
                "rhythm",
                "map",
                "parameter",
                "width",
                "recovery",
                "maturity",
                "pairs"
            ]
        );
    }

    #[hypothesis_test]
    fn terminal_entry_uses_padding_port_without_value_or_child_ports() {
        let bytes = (511u16 | (1u16 << 9)).to_le_bytes();
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_limit(16);
        let events = RefCell::new(Vec::new());
        let policy = Policy {
            events: &events,
            terminal: true,
        };
        let mut recovery = Recovery(&events);
        let (properties, complete, terminal, children): (_, _, _, Vec<()>) =
            read_property_entry(&mut cursor, &policy, &mut recovery)
                .unwrap()
                .unwrap();
        assert_eq!(cursor.pos(), 16);
        assert_eq!(properties[0].stat_id, 511);
        assert!(complete && terminal && children.is_empty());
        assert_eq!(
            *events.borrow(),
            ["trace", "rhythm", "authority", "padding"]
        );
    }

    #[hypothesis_test]
    fn failed_id_read_does_not_invoke_layout_or_recovery_ports() {
        let bytes: &[u8] = &[];
        let reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let events = RefCell::new(Vec::new());
        let policy = Policy {
            events: &events,
            terminal: false,
        };
        let mut recovery = Recovery(&events);
        assert!(read_property_entry(&mut cursor, &policy, &mut recovery).is_err());
        assert!(events.borrow().is_empty());
    }
}
