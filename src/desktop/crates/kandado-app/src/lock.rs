//! When the app locks, and how many wrong passwords it tolerates.
//!
//! Both are plain state machines that take the time as an argument, so they
//! are tested without waiting.

use std::time::{Duration, Instant};

use crate::settings::LockAfter;

/// Why the app locked. Shown nowhere yet, but logged and useful in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockReason {
    Manual,
    Idle,
    Sleep,
    SessionSwitch,
}

/// Tracks the last time the person used the app.
#[derive(Debug, Clone, Copy)]
pub struct IdleTimer {
    last_activity: Instant,
}

impl IdleTimer {
    pub fn new(now: Instant) -> Self {
        Self { last_activity: now }
    }

    /// Call on any keypress, click or focus.
    pub fn touch(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// True when the idle time has reached the limit for `setting`.
    pub fn expired(&self, setting: LockAfter, now: Instant) -> bool {
        match setting.idle_limit() {
            Some(limit) => now.saturating_duration_since(self.last_activity) >= limit,
            None => false,
        }
    }
}

/// Five wrong passwords in a row lock unlocking for a minute (D-04).
pub const MAX_ATTEMPTS: u32 = 5;
pub const LOCKOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    /// Go ahead and try the password.
    Allowed,
    /// Too many wrong passwords; try again after this long.
    Wait(Duration),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnlockGuard {
    failures: u32,
    blocked_until: Option<Instant>,
}

impl UnlockGuard {
    /// Asks whether a password may be tried now.
    pub fn check(&mut self, now: Instant) -> Attempt {
        if let Some(until) = self.blocked_until {
            if now < until {
                return Attempt::Wait(until - now);
            }
            self.blocked_until = None;
            self.failures = 0;
        }
        Attempt::Allowed
    }

    /// Records a wrong password and returns how many tries are left before
    /// the wait starts (0 when it just started).
    pub fn failed(&mut self, now: Instant) -> u32 {
        self.failures += 1;
        if self.failures >= MAX_ATTEMPTS {
            self.blocked_until = Some(now + LOCKOUT);
            0
        } else {
            MAX_ATTEMPTS - self.failures
        }
    }

    pub fn succeeded(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_timer_locks_after_the_chosen_time() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(start);
        let minute = Duration::from_secs(60);
        assert!(!timer.expired(LockAfter::Minutes1, start + minute - Duration::from_secs(1)));
        assert!(timer.expired(LockAfter::Minutes1, start + minute));
        // Activity pushes the deadline out.
        timer.touch(start + minute);
        assert!(!timer.expired(
            LockAfter::Minutes1,
            start + minute * 2 - Duration::from_secs(1)
        ));
        assert!(timer.expired(LockAfter::Minutes1, start + minute * 2));
    }

    #[test]
    fn immediately_locks_at_once_and_never_does_not() {
        let start = Instant::now();
        let timer = IdleTimer::new(start);
        assert!(timer.expired(LockAfter::Immediately, start));
        let much_later = start + Duration::from_secs(10 * 24 * 3600);
        assert!(!timer.expired(LockAfter::Never, much_later));
        assert!(!timer.expired(LockAfter::OnRestart, much_later));
    }

    #[test]
    fn five_wrong_passwords_block_for_a_minute() {
        let start = Instant::now();
        let mut guard = UnlockGuard::default();
        for expected in [4, 3, 2, 1, 0] {
            assert_eq!(guard.check(start), Attempt::Allowed);
            assert_eq!(guard.failed(start), expected);
        }
        assert_eq!(
            guard.check(start + Duration::from_secs(20)),
            Attempt::Wait(Duration::from_secs(40))
        );
        // After the wait the count starts again.
        assert_eq!(guard.check(start + LOCKOUT), Attempt::Allowed);
        assert_eq!(guard.failed(start + LOCKOUT), 4);
    }

    #[test]
    fn a_correct_password_resets_the_count() {
        let now = Instant::now();
        let mut guard = UnlockGuard::default();
        guard.failed(now);
        guard.failed(now);
        guard.succeeded();
        assert_eq!(guard.failed(now), 4);
    }
}
