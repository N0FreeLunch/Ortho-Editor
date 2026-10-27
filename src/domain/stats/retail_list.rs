//! The non-Alpha list provider has no socket probing or child recovery.
use crate::data::bit_cursor::BitCursor;
use crate::domain::stats::property_entry::read_property_entry;
use crate::domain::stats::property_list::{ParsedPropertyList, PropertyListReader};
use crate::domain::stats::retail_entry::{NoChildRecovery, RetailEntryPolicy};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(super) struct RetailListReader<'a> {
    policy: RetailEntryPolicy<'a>,
}
impl<'a> RetailListReader<'a> {
    pub(super) fn new(code: &'a str, consume_pairs: bool) -> Self {
        Self {
            policy: RetailEntryPolicy::new(code, consume_pairs),
        }
    }
}
impl<R: BitRead, T> PropertyListReader<R, T> for RetailListReader<'_> {
    fn stop_before_entry(&self, _: &BitCursor<R>) -> bool {
        false
    }
    fn read_entry(
        &mut self,
        recorder: &mut BitCursor<R>,
    ) -> ParsingResult<Option<ParsedPropertyList<T>>> {
        read_property_entry(recorder, &self.policy, &mut NoChildRecovery)
    }
    fn recover_children(&mut self, _: &mut BitCursor<R>, _: &mut Vec<T>) -> ParsingResult<()> {
        Ok(())
    }
}

/// Read an ordinary list without constructing a legacy recovery provider.
pub(crate) fn read_retail_property_list<R: BitRead, T>(
    recorder: &mut BitCursor<R>,
    code: &str,
    alpha_runeword: bool,
    axiom: &crate::domain::stats::StatsAxiom,
    consume_d2r_v3_paired_stats: bool,
) -> ParsingResult<ParsedPropertyList<T>> {
    use crate::domain::stats::budget::PropertyListBudget;
    use crate::domain::stats::parser::property_list_depth_exhausted;
    use crate::domain::stats::property_list::{ListReadConfig, read_property_stream};

    let start_pos = recorder.pos();
    let actual_runeword = (alpha_runeword || code.trim() == "Þ.") && axiom.is_socketed;
    let budget = PropertyListBudget::new(recorder.limit(), false);
    let is_compact = axiom.is_compact || code.trim().is_empty();
    let config = ListReadConfig {
        single_entry: is_compact && !actual_runeword,
        bit_budget: None,
        mark_limit_as_terminal: false,
        child_stop_count: None,
    };
    if property_list_depth_exhausted() {
        return Ok((Vec::new(), false, false, Vec::new()));
    }
    let mut reader = RetailListReader::new(&axiom.code, consume_d2r_v3_paired_stats);
    read_property_stream(recorder, start_pos, config, budget, &mut reader)
}
