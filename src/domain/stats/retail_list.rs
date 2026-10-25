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
