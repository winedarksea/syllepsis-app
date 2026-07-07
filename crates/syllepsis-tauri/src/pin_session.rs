//! In-memory PIN-lock session for the currently open book (privacy-security.md "PIN-Locked
//! Notes"). Session state is process-only — it never persists — so the book is always relocked on
//! app restart, and always cleared when the open book changes.
//!
//! Time is passed in explicitly (`SystemTime`, not a bare `Instant::now()` call inside the type) so
//! tests can fast-forward the idle clock without sleeping; the 30s relock-poll task in `lib.rs`
//! setup is the only real caller that supplies `SystemTime::now()`.

use std::time::SystemTime;

use syllepsis_core::pinlock::BookKey;

/// Whether a key is currently held and, if so, how long it's been idle.
pub struct PinSession {
    key: Option<BookKey>,
    last_activity: Option<SystemTime>,
}

impl PinSession {
    pub fn new() -> Self {
        PinSession {
            key: None,
            last_activity: None,
        }
    }

    /// Unlock (or refresh) the session with a verified key.
    pub fn unlock(&mut self, key: BookKey, now: SystemTime) {
        self.key = Some(key);
        self.last_activity = Some(now);
    }

    /// Relock: drop the key. Zeroizes on drop via `BookKey`'s `Zeroizing` field.
    pub fn lock(&mut self) {
        self.key = None;
        self.last_activity = None;
    }

    /// Record activity against an already-unlocked session (a read/save of a locked note). A
    /// locked session has nothing to touch.
    pub fn touch(&mut self, now: SystemTime) {
        if self.key.is_some() {
            self.last_activity = Some(now);
        }
    }

    pub fn key(&self) -> Option<&BookKey> {
        self.key.as_ref()
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// If unlocked and idle for at least `idle_minutes`, relock and return `true` (the caller emits
    /// `pin-session-changed`). A `0` timeout means "never auto-relock".
    pub fn relock_if_idle(&mut self, now: SystemTime, idle_minutes: u32) -> bool {
        if idle_minutes == 0 || self.key.is_none() {
            return false;
        }
        let Some(last) = self.last_activity else {
            return false;
        };
        let idle = now
            .duration_since(last)
            .unwrap_or(std::time::Duration::ZERO);
        if idle.as_secs() >= idle_minutes as u64 * 60 {
            self.lock();
            true
        } else {
            false
        }
    }
}

impl Default for PinSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key() -> BookKey {
        BookKey::new([1u8; 32], "abcd1234".to_string())
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn starts_locked() {
        let session = PinSession::new();
        assert!(!session.is_unlocked());
        assert!(session.key().is_none());
    }

    #[test]
    fn unlock_then_lock_round_trips() {
        let mut session = PinSession::new();
        session.unlock(key(), at(0));
        assert!(session.is_unlocked());
        assert_eq!(session.key().unwrap().key_id(), "abcd1234");

        session.lock();
        assert!(!session.is_unlocked());
    }

    #[test]
    fn relocks_after_idle_timeout_elapses() {
        let mut session = PinSession::new();
        session.unlock(key(), at(0));

        assert!(!session.relock_if_idle(at(14 * 60), 15), "not idle yet");
        assert!(session.is_unlocked());

        assert!(session.relock_if_idle(at(15 * 60), 15), "idle timeout hit");
        assert!(!session.is_unlocked());
    }

    #[test]
    fn touch_resets_the_idle_clock() {
        let mut session = PinSession::new();
        session.unlock(key(), at(0));
        session.touch(at(10 * 60));

        assert!(
            !session.relock_if_idle(at(20 * 60), 15),
            "touch should have reset the 15-minute window"
        );
        assert!(session.is_unlocked());
    }

    #[test]
    fn zero_minute_timeout_never_relocks() {
        let mut session = PinSession::new();
        session.unlock(key(), at(0));
        assert!(!session.relock_if_idle(at(1_000_000), 0));
        assert!(session.is_unlocked());
    }

    #[test]
    fn relock_on_a_locked_session_is_a_noop() {
        let mut session = PinSession::new();
        assert!(!session.relock_if_idle(at(100), 15));
    }
}
