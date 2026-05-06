use std::sync::atomic::{AtomicBool, Ordering};

static REFEXP_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_refexp(enabled: bool) {
    REFEXP_ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn is_refexp() -> bool {
    REFEXP_ENABLED.load(Ordering::SeqCst)
}
