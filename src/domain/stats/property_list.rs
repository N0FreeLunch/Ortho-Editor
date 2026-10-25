//! Property-list sequencing with injected entry and child-recovery operations.
use crate::data::bit_cursor::BitCursor;
use crate::domain::stats::ItemProperty;
use crate::domain::stats::budget::PropertyListBudget;
use crate::error::ParsingResult;
use bitstream_io::BitRead;

/// Properties, observed terminator, terminal flag, and children in stream order.
pub(crate) type ParsedPropertyList<T> = (Vec<ItemProperty>, bool, bool, Vec<T>);

pub(super) struct ListReadConfig {
    pub single_entry: bool,
    pub bit_budget: Option<u64>,
    pub mark_limit_as_terminal: bool,
    pub child_stop_count: Option<usize>,
}

pub(crate) trait PropertyListReader<R: BitRead, T> {
    fn stop_before_entry(&self, recorder: &BitCursor<R>) -> bool;
    fn read_entry(
        &mut self,
        recorder: &mut BitCursor<R>,
    ) -> ParsingResult<Option<ParsedPropertyList<T>>>;
    fn recover_children(
        &mut self,
        recorder: &mut BitCursor<R>,
        children: &mut Vec<T>,
    ) -> ParsingResult<()>;
}

pub(super) fn read_property_stream<R: BitRead, T>(
    recorder: &mut BitCursor<R>,
    start_pos: u64,
    config: ListReadConfig,
    budget: PropertyListBudget,
    reader: &mut impl PropertyListReader<R, T>,
) -> ParsingResult<ParsedPropertyList<T>> {
    let mut props = Vec::new();
    let mut nested_items = Vec::new();
    let mut terminator_bit = false;
    let mut saw_terminator = false;
    loop {
        if config.single_entry && props.len() >= 1 {
            break;
        }
        if config
            .bit_budget
            .is_some_and(|limit| (recorder.pos() - start_pos) > limit)
        {
            return Err(
                recorder.fail(crate::error::ParsingError::BitBudgetExceeded {
                    bit_offset: recorder.pos(),
                }),
            );
        }
        if reader.stop_before_entry(recorder) {
            saw_terminator = true;
            terminator_bit = false;
            break;
        }
        if let Some(limit) = budget.incoming_limit() {
            if recorder.pos()
                + u64::from(
                    crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY
                        .stat_id_bits,
                )
                >= limit
            {
                if config.mark_limit_as_terminal && !saw_terminator {
                    saw_terminator = true;
                    terminator_bit = false;
                }
                break;
            }
        }
        match reader.read_entry(recorder) {
            Ok(Some((parsed_properties, is_term, term_bit, items))) => {
                props.extend(parsed_properties);
                nested_items.extend(items);
                if config
                    .child_stop_count
                    .is_some_and(|limit| nested_items.len() >= limit)
                {
                    saw_terminator = true;
                    terminator_bit = false;
                    break;
                }
                if is_term {
                    saw_terminator = true;
                    terminator_bit = term_bit;
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let is_eof = match &e.error {
                    crate::error::ParsingError::Io(msg) => {
                        msg.contains("failed to fill whole buffer")
                            || msg.contains("unexpected end of file")
                    }
                    _ => false,
                };
                if is_eof {
                    return Ok((props, true, false, nested_items));
                }
                return Err(e);
            }
        }
    }
    reader.recover_children(recorder, &mut nested_items)?;
    if let Some(limit) = budget.completion_limit(recorder.pos()) {
        recorder.set_limit(limit);
    }
    Ok((props, saw_terminator, terminator_bit, nested_items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitReader, LittleEndian};
    use d2r_macros::hypothesis_test;
    use std::io::Cursor;

    struct Script {
        eof: bool,
        reads: usize,
        completions: usize,
    }
    impl<R: BitRead> PropertyListReader<R, ()> for Script {
        fn stop_before_entry(&self, _: &BitCursor<R>) -> bool {
            false
        }
        fn read_entry(
            &mut self,
            rec: &mut BitCursor<R>,
        ) -> ParsingResult<Option<ParsedPropertyList<()>>> {
            self.reads += 1;
            if self.eof {
                return Err(rec.fail(crate::error::ParsingError::Io(
                    "unexpected end of file".into(),
                )));
            }
            Ok(Some((Vec::new(), true, true, vec![()])))
        }
        fn recover_children(&mut self, _: &mut BitCursor<R>, _: &mut Vec<()>) -> ParsingResult<()> {
            self.completions += 1;
            Ok(())
        }
    }

    #[hypothesis_test]
    fn generic_children_keep_stop_priority_and_eof_skips_completion() {
        for eof in [false, true] {
            let mut rec = BitCursor::new(BitReader::endian(Cursor::new([0u8; 1]), LittleEndian));
            let budget = PropertyListBudget::new(rec.limit(), true);
            rec.set_limit(budget.entry_extension().unwrap());
            let mut script = Script {
                eof,
                reads: 0,
                completions: 0,
            };
            let result = read_property_stream(
                &mut rec,
                0,
                ListReadConfig {
                    single_entry: false,
                    bit_budget: None,
                    mark_limit_as_terminal: true,
                    child_stop_count: Some(1),
                },
                budget,
                &mut script,
            )
            .unwrap();
            assert!(result.0.is_empty());
            assert!(result.1);
            assert!(!result.2);
            assert_eq!(result.3.len(), usize::from(!eof));
            assert_eq!(script.reads, 1);
            assert_eq!(script.completions, usize::from(!eof));
            assert_eq!(rec.limit(), Some(if eof { u64::MAX } else { 512 }));
        }
    }
}
