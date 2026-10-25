//! Canonical property output with layout and nested-value operations injected.
use crate::domain::stats::ItemProperty;
use crate::domain::stats::property_entry::PropertyRhythm;
use crate::wire::BitEmitter;
use std::io;

pub(crate) trait PropertyWritePolicy {
    fn rhythm(&self) -> PropertyRhythm;
    fn map_id(&self, raw_id: u32) -> u32;
    fn omit_parameter(&self) -> bool;
    fn effective_width(&self, raw_id: u32, save_bits: Option<u32>, rhythm: PropertyRhythm) -> u32;
    fn is_runeword(&self) -> bool;
    fn allows_auto_terminator(&self) -> bool;
    fn is_compact(&self) -> bool;
    fn tvs_padding_bits(&self) -> Option<u32>;
}
pub(crate) trait NestedPropertyWriter {
    fn try_write(&mut self, emitter: &mut BitEmitter, raw_id: u32) -> io::Result<bool>;
}
pub(super) fn write_property_stream(
    emitter: &mut BitEmitter,
    props: &[ItemProperty],
    terminator_bit: bool,
    properties_complete: bool,
    policy: &impl PropertyWritePolicy,
    children: &mut impl NestedPropertyWriter,
) -> io::Result<()> {
    let _start_bits = emitter.written_bits();
    let rhythm = policy.rhythm();
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;
    for prop in props {
        let raw_id = prop.stat_id;
        emitter.write_bits(raw_id, id_bits)?;
        if !children.try_write(emitter, raw_id)? {
            let mapped_id = policy.map_id(raw_id);
            let stat_cost = crate::data::stat_costs::STAT_COSTS
                .iter()
                .find(|s| s.id == mapped_id);
            let suppress_authority_params = policy.omit_parameter();
            if let Some(stat) = stat_cost {
                if stat.save_param_bits > 0 && !suppress_authority_params {
                    emitter.write_bits(prop.param as u32, stat.save_param_bits as u32)?;
                }
            }
            if raw_id != terminator {
                let effective_width = policy.effective_width(
                    raw_id,
                    stat_cost.map(|stat| u32::from(stat.save_bits)),
                    rhythm,
                );
                emitter.write_bits(prop.raw_value as u32, effective_width)?;
            }
        }
    }
    let already_has_term = props.iter().any(|p| p.stat_id == terminator);
    let is_rw = policy.is_runeword();
    if properties_complete
        && !already_has_term
        && policy.allows_auto_terminator()
        && !is_rw
        && !policy.is_compact()
    {
        emitter.write_bits(terminator, id_bits)?;
    }
    if properties_complete && rhythm.has_terminal_bit {
        emitter.write_bit(terminator_bit)?;
        if rhythm.has_extra_terminal_bit {
            emitter.write_bit(terminator_bit)?;
        }
        emitter.byte_align()?;
    }
    if properties_complete && let Some(tvs_bits) = policy.tvs_padding_bits() {
        emitter.write_bits(0, tvs_bits)?;
    }
    Ok(())
}
pub(super) struct NoNestedPropertyWriter;
impl NestedPropertyWriter for NoNestedPropertyWriter {
    fn try_write(&mut self, _: &mut BitEmitter, _: u32) -> io::Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::ItemBitRange;
    use d2r_macros::hypothesis_test;
    use std::cell::RefCell;
    struct Policy<'a>(&'a RefCell<Vec<&'static str>>);
    impl PropertyWritePolicy for Policy<'_> {
        fn rhythm(&self) -> PropertyRhythm {
            self.0.borrow_mut().push("rhythm");
            PropertyRhythm {
                id_bits: 9,
                value_bits: None,
                has_terminal_bit: false,
                has_extra_terminal_bit: false,
            }
        }
        fn map_id(&self, id: u32) -> u32 {
            self.0.borrow_mut().push("map");
            id
        }
        fn omit_parameter(&self) -> bool {
            self.0.borrow_mut().push("parameter");
            false
        }
        fn effective_width(&self, _: u32, _: Option<u32>, _: PropertyRhythm) -> u32 {
            self.0.borrow_mut().push("width");
            5
        }
        fn is_runeword(&self) -> bool {
            self.0.borrow_mut().push("runeword");
            false
        }
        fn allows_auto_terminator(&self) -> bool {
            self.0.borrow_mut().push("auto");
            true
        }
        fn is_compact(&self) -> bool {
            self.0.borrow_mut().push("compact");
            false
        }
        fn tvs_padding_bits(&self) -> Option<u32> {
            self.0.borrow_mut().push("padding");
            None
        }
    }
    struct Child<'a> {
        events: &'a RefCell<Vec<&'static str>>,
        fail: bool,
    }
    impl NestedPropertyWriter for Child<'_> {
        fn try_write(&mut self, emitter: &mut BitEmitter, _: u32) -> io::Result<bool> {
            assert_eq!(emitter.written_bits(), 9);
            self.events.borrow_mut().push("child");
            emitter.write_bit(true)?;
            if self.fail {
                return Err(io::Error::other("controlled child failure"));
            }
            Ok(true)
        }
    }
    #[hypothesis_test]
    fn injected_children_preserve_partial_emission_and_skip_scalar_policy() {
        for fail in [false, true] {
            let events = RefCell::new(Vec::new());
            let policy = Policy(&events);
            let mut child = Child {
                events: &events,
                fail,
            };
            let mut emitter = BitEmitter::new();
            let property =
                ItemProperty::new(509, "control".into(), 0, 17, 17, ItemBitRange::default());
            let result =
                write_property_stream(&mut emitter, &[property], false, true, &policy, &mut child);
            assert_eq!(result.is_err(), fail);
            assert_eq!(emitter.written_bits(), if fail { 10 } else { 19 });
            assert_eq!(
                *events.borrow(),
                if fail {
                    vec!["rhythm", "child"]
                } else {
                    vec!["rhythm", "child", "runeword", "auto", "compact", "padding"]
                }
            );
        }
    }
}
