use std::sync::atomic::{AtomicBool, Ordering};

static FAULTED: AtomicBool = AtomicBool::new(false);

/// Fault the program with a reason
pub fn set_to_faulted(reason: &str) {
    if !FAULTED.load(Ordering::Acquire) {
        FAULTED.store(true, Ordering::Release);
        if cfg!(debug_assertions) {
            panic!("{}", reason)
        } else {
            println!("{}", reason)
        }
    }
}

#[inline]
/// Check for program faulting
pub fn is_faulted() -> bool {
    FAULTED.load(Ordering::Relaxed)
}