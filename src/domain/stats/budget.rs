/// Continuation limits for one property list and its downstream readers.
pub(super) struct PropertyListBudget {
    incoming: Option<u64>,
    alpha_recovery: bool,
}

impl PropertyListBudget {
    // This is an operational recovery allowance, measured from the list end.
    const RECOVERY_ALLOWANCE_BITS: u64 = 512;

    pub(super) fn new(incoming: Option<u64>, alpha_recovery: bool) -> Self {
        Self {
            incoming,
            alpha_recovery,
        }
    }

    /// Keep preflight checks tied to the caller's bound before entry extension.
    pub(super) fn incoming_limit(&self) -> Option<u64> {
        self.incoming
    }

    /// None means no change; the extension also survives non-finalized exits.
    pub(super) fn entry_extension(&self) -> Option<u64> {
        if self.alpha_recovery && self.incoming.is_none() {
            Some(u64::MAX)
        } else {
            None
        }
    }

    /// Publish continuation only at normal completion, not on EOF or unwind.
    pub(super) fn completion_limit(&self, list_end: u64) -> Option<u64> {
        if self.alpha_recovery {
            let recovery_limit = list_end.saturating_add(Self::RECOVERY_ALLOWANCE_BITS);
            Some(
                self.incoming
                    .map_or(recovery_limit, |limit| limit.min(recovery_limit)),
            )
        } else {
            self.incoming
        }
    }
}
