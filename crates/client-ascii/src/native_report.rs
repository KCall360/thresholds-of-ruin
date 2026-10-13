//! Opt-in process diagnostics, independent of presentation and game authority.
use minifb::Key;

struct Press {
    key: Key,
    sequence: u64,
    released_last_pump: bool,
}

pub(crate) struct NativeReport {
    enabled: bool,
    sequence: u64,
    pending: Vec<Press>,
}

impl NativeReport {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            sequence: 0,
            pending: Vec::new(),
        }
    }

    pub(crate) fn pressed(&mut self, key: Key) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("native diagnostic sequence exhausted");
        // At most one pending acknowledgement per physical key, even if
        // ordinary native input outruns the diagnostic reader.
        self.pending.retain(|press| press.key != key);
        self.pending.push(Press {
            key,
            sequence: self.sequence,
            released_last_pump: false,
        });
        Some(self.sequence)
    }

    pub(crate) fn after_pump(&mut self, mut is_down: impl FnMut(Key) -> bool) -> Vec<u64> {
        let mut released = Vec::new();
        self.pending.retain_mut(|press| {
            let up = !is_down(press.key);
            // minifb advances held durations before dispatching Windows/X11
            // events. The first up sample may still have a held duration;
            // require that up level to span the following real native pump.
            if up && press.released_last_pump {
                released.push(press.sequence);
                false
            } else {
                press.released_last_pump = up;
                true
            }
        });
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_waits_for_the_pump_that_resets_minifb_held_duration() {
        let mut report = NativeReport::new(true);
        let first = report.pressed(Key::Down).unwrap();
        assert!(report.after_pump(|_| true).is_empty());
        // Windows minifb advances held duration before applying WM_KEYUP.
        // Its first released level alone does not prove a new press can register.
        assert!(report.after_pump(|_| false).is_empty());
        assert_eq!(report.after_pump(|_| false), [first]);
        let second = report.pressed(Key::Down).unwrap();
        assert!(second > first);
        assert!(report.after_pump(|_| true).is_empty());
        assert!(report.after_pump(|_| false).is_empty());
        assert_eq!(report.after_pump(|_| false), [second]);
        assert!(report
            .after_pump(|_| panic!("idle diagnostics must not sample keys"))
            .is_empty());
    }

    #[test]
    fn a_repressed_key_needs_a_new_complete_release_boundary() {
        let mut report = NativeReport::new(true);
        let sequence = report.pressed(Key::Enter).unwrap();
        assert!(report.after_pump(|_| false).is_empty());
        assert!(report.after_pump(|_| true).is_empty());
        assert!(report.after_pump(|_| false).is_empty());
        assert_eq!(report.after_pump(|_| false), [sequence]);
    }

    #[test]
    fn diagnostic_tracking_is_bounded_fresh_and_independent_per_key() {
        let mut report = NativeReport::new(true);
        let old = report.pressed(Key::Down).unwrap();
        let current = report.pressed(Key::Down).unwrap();
        let other = report.pressed(Key::Enter).unwrap();
        assert!(current > old);
        assert_eq!(report.pending.len(), 2);
        assert!(report.after_pump(|key| key == Key::Enter).is_empty());
        assert_eq!(report.after_pump(|key| key == Key::Enter), [current]);
        assert!(report.after_pump(|_| false).is_empty());
        assert_eq!(report.after_pump(|_| false), [other]);
    }

    #[test]
    fn reporting_disabled_does_not_track_or_sample_native_keys() {
        let mut report = NativeReport::new(false);
        assert_eq!(report.pressed(Key::Down), None);
        assert_eq!(report.sequence, 0);
        assert!(report
            .after_pump(|_| panic!("disabled diagnostics sampled input"))
            .is_empty());
        assert!(report.pending.is_empty());
    }
}
