//! Local event deduplication. A local observer tick is never a network heartbeat.
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct EventGate {
    desired: String,
    attempts: u8,
    acknowledged: bool,
    blocked: bool,
    next: Option<Instant>,
}
impl EventGate {
    pub fn ready(&mut self, fingerprint: String, now: Instant) -> bool {
        if self.desired != fingerprint {
            self.desired = fingerprint;
            self.attempts = 0;
            self.acknowledged = false;
        }
        if self.blocked
            || self.acknowledged
            || self.attempts >= 3
            || self.next.map_or(false, |n| now < n)
        {
            return false;
        }
        self.attempts += 1;
        self.next = Some(now + Duration::from_secs(5 * u64::from(self.attempts)));
        true
    }
    pub fn accepted(&mut self) {
        self.acknowledged = true;
    }
    pub fn block(&mut self) {
        self.blocked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_day_sends_once() {
        let now = Instant::now();
        let mut gate = EventGate::default();
        let mut count = 0;
        for tick in 0..28800 {
            if gate.ready("online".into(), now + Duration::from_secs(tick * 3)) {
                count += 1;
                gate.accepted();
            }
        }
        assert_eq!(count, 1);
        assert!(gate.ready("offline".into(), now + Duration::from_secs(86400)));
    }
    #[test]
    fn failure_stops_after_three_and_new_event_can_recover() {
        let now = Instant::now();
        let mut gate = EventGate::default();
        let mut count = 0;
        for tick in 0..28800 {
            if gate.ready("online".into(), now + Duration::from_secs(tick * 3)) {
                count += 1;
            }
        }
        assert_eq!(count, 3);
        assert!(gate.ready("offline".into(), now + Duration::from_secs(86400)));
        gate.block();
        assert!(!gate.ready("online".into(), now + Duration::from_secs(86410)));
    }
}
