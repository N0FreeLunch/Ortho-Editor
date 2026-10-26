//! Inner legacy field admission, separate from each caller's template gate.
use crate::data::generated::layout_policy::legacy_extended_payload_code_policy;

pub(crate) struct LegacyPayloadPresence<'a> {
    compact: bool,
    receiver_code: &'a str,
}

impl<'a> LegacyPayloadPresence<'a> {
    pub(crate) fn new(compact: bool, receiver_code: &'a str) -> Self {
        Self {
            compact,
            receiver_code,
        }
    }

    pub(crate) fn allows_defense(&self) -> bool {
        !self.compact
    }

    pub(crate) fn allows_durability(&self) -> bool {
        !self.compact
            || legacy_extended_payload_code_policy(self.receiver_code.trim()).force_durability
    }

    pub(crate) fn allows_quantity(&self) -> bool {
        !self.compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::hypothesis_test;

    #[hypothesis_test]
    fn outer_template_admission_and_receiver_code_remain_distinct() {
        for (outer, receiver, compact, outer_forced, expected) in [
            ("wa2", "cap", true, true, (false, false, false)),
            ("cap", "wa2", true, false, (false, true, false)),
            ("rhd", "rhd\0", true, true, (false, false, false)),
            (
                "plain",
                "\u{2003}rhd\u{2003}",
                true,
                false,
                (false, true, false),
            ),
            ("wa2", "RHD", true, true, (false, false, false)),
            ("plain", "unknown", false, false, (true, true, true)),
        ] {
            assert_eq!(
                legacy_extended_payload_code_policy(outer.trim()).force_durability,
                outer_forced
            );
            let presence = LegacyPayloadPresence::new(compact, receiver);
            assert_eq!(
                (
                    presence.allows_defense(),
                    presence.allows_durability(),
                    presence.allows_quantity()
                ),
                expected
            );
        }
    }
}
