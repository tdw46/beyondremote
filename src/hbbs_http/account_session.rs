//! Local account changes invalidate open account-authorized streams without
//! polling the hosted API. Each service/UI process publishes its own IPC update.
use hbb_common::tokio::sync::watch;

lazy_static::lazy_static! {
    static ref CHANGES: watch::Sender<u64> = watch::channel(0).0;
}

pub fn subscribe() -> watch::Receiver<u64> {
    CHANGES.subscribe()
}

pub fn has_changed() -> bool {
    *CHANGES.borrow() != 0
}

pub fn changed(previous: &str, current: &str) {
    if previous != current {
        CHANGES.send_modify(|generation| *generation = generation.wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_account_does_not_interrupt_but_logout_does() {
        let mut receiver = subscribe();
        changed("fixture", "fixture");
        assert!(!receiver.has_changed().unwrap_or(true));
        changed("fixture", "");
        assert!(receiver.has_changed().unwrap_or(false));
        receiver.borrow_and_update();
        changed("", "new fixture");
        assert!(receiver.has_changed().unwrap_or(false));
    }
}
