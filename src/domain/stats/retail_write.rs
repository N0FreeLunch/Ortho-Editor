//! Non-Alpha writer policy uses shared static layout and width policies.
use crate::data::generated::layout_policy::LEGACY_ITEM_FLAG_POLICY;
use crate::domain::stats::property_entry::{PropertyEntryPolicy, PropertyRhythm};
use crate::domain::stats::property_write::PropertyWritePolicy;
use crate::domain::stats::retail_entry::RetailEntryPolicy;

pub(super) struct RetailWritePolicy<'a> {
    entry: RetailEntryPolicy<'a>,
    flag_code: &'a str,
    compact: bool,
}
impl<'a> RetailWritePolicy<'a> {
    pub(super) fn new(outer_code: &'a str, axiom_code: &'a str, compact: bool) -> Self {
        Self {
            entry: RetailEntryPolicy::new(outer_code, false),
            flag_code: axiom_code,
            compact,
        }
    }
}
impl PropertyWritePolicy for RetailWritePolicy<'_> {
    fn rhythm(&self) -> PropertyRhythm {
        self.entry.rhythm(0)
    }
    fn map_id(&self, id: u32) -> u32 {
        self.entry.map_id(id)
    }
    fn omit_parameter(&self) -> bool {
        self.entry.omit_parameter()
    }
    fn effective_width(&self, raw: u32, bits: Option<u32>, rhythm: PropertyRhythm) -> u32 {
        self.entry.effective_width(raw, bits, rhythm)
    }
    fn is_runeword(&self) -> bool {
        LEGACY_ITEM_FLAG_POLICY
            .unconditional_runeword_codes
            .contains(&self.flag_code.trim())
    }
    fn allows_auto_terminator(&self) -> bool {
        true
    }
    fn is_compact(&self) -> bool {
        self.compact
    }
    fn tvs_padding_bits(&self) -> Option<u32> {
        None
    }
}
