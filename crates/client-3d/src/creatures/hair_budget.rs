//! Fitting loose hair against the body is the costliest animation work, and
//! a crowd cannot have it every frame on the main thread. In play, solves
//! run on background threads instead: between them each wearer's hair stays
//! on its head in its last fitted shape, and a finished solve is written in
//! a later frame. Free workers go to the wearers that have waited longest,
//! sooner the nearer they are to the camera.
use bevy::prelude::Resource;

/// Where hair contact solves run.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HairSolving {
    /// Every frame, on the main thread, before the frame is drawn: exact
    /// and deterministic, for tests, benchmarks and still reviews.
    #[default]
    Inline,
    /// On background threads, written when finished: for play.
    Background,
}

/// Distance at which waiting counts half as much as right beside the camera.
pub(crate) const NEAR: f32 = 4.0;
/// The most motion one hair solve answers at once, in seconds.
pub(crate) const MAX_STEP: f32 = 0.25;

/// One wearer whose hair awaits a solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Waiting {
    /// Seconds since its last solve began.
    pub wait: f32,
    /// Distance from the camera.
    pub distance: f32,
}

impl Waiting {
    fn priority(&self) -> f32 {
        let far = self.distance / NEAR;
        self.wait / (1.0 + far * far)
    }
}

/// The waiting wearers to solve now, most pressing first, at most `free`.
pub(crate) fn choose(waiting: &[Waiting], free: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..waiting.len())
        .filter(|&i| waiting[i].wait > 0.0)
        .collect();
    order.sort_by(|&a, &b| waiting[b].priority().total_cmp(&waiting[a].priority()));
    order.truncate(free);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(wait: f32, distance: f32) -> Waiting {
        Waiting { wait, distance }
    }

    #[test]
    fn free_workers_go_to_the_longest_waits() {
        let jobs = [waiting(0.1, 5.0), waiting(0.4, 5.0), waiting(0.2, 5.0)];
        assert_eq!(choose(&jobs, 2), vec![1, 2]);
    }

    #[test]
    fn nearer_wearers_are_solved_sooner_for_the_same_wait() {
        let jobs = [waiting(0.1, 12.0), waiting(0.1, 2.0)];
        assert_eq!(choose(&jobs, 1), vec![1]);
    }

    #[test]
    fn no_free_worker_or_no_motion_solves_nothing() {
        assert!(choose(&[waiting(0.3, 1.0)], 0).is_empty());
        assert!(choose(&[waiting(0.0, 1.0)], 4).is_empty());
        assert!(choose(&[], 4).is_empty());
    }
}
