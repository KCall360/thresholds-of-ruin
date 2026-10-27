//! Presentation-time capture barrier for offline reviews. Real time remains
//! available for watchdogs; every ordinary Time consumer holds the same pose.
use bevy::{prelude::*, render::view::screenshot::Screenshot, time::TimeSystem};

pub(crate) struct ReviewClockPlugin;
impl Plugin for ReviewClockPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            First,
            (hold_for_capture, remember_step).chain().before(TimeSystem),
        )
        .add_systems(First, apply_step.after(TimeSystem));
    }
}

/// Optional offline frame clock. Real time remains owned by Bevy for watchdogs.
#[derive(Resource)]
pub(crate) struct ReviewStep {
    duration: std::time::Duration,
    previous: Time<Virtual>,
}
impl ReviewStep {
    pub(crate) fn new(hz: u32) -> Result<Self, &'static str> {
        if !(20..=240).contains(&hz) {
            return Err(
                "--review-step-hz must be between 20 and 240 (animation caps steps at 50 ms)",
            );
        }
        Ok(Self {
            duration: std::time::Duration::from_secs_f64(1.0 / hz as f64),
            previous: Time::default(),
        })
    }
}
fn remember_step(time: Res<Time<Virtual>>, step: Option<ResMut<ReviewStep>>) {
    if let Some(mut step) = step {
        step.previous = *time;
    }
}
fn apply_step(
    step: Option<Res<ReviewStep>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut time: ResMut<Time>,
) {
    let Some(step) = step else { return };
    *virtual_time = step.previous;
    let delta = if virtual_time.is_paused() {
        std::time::Duration::ZERO
    } else {
        step.duration
    };
    virtual_time.advance_by(delta);
    *time = virtual_time.as_generic();
}

fn hold_for_capture(
    pending: Query<(), With<Screenshot>>,
    mut time: ResMut<Time<Virtual>>,
    mut owned_pause: Local<bool>,
) {
    if !pending.is_empty() {
        if !time.is_paused() {
            time.pause();
            *owned_pause = true;
            info!(
                "Review capture clock held at {} ns",
                time.elapsed().as_nanos()
            );
        }
    } else if *owned_pause {
        info!(
            "Review capture clock resumed at {} ns",
            time.elapsed().as_nanos()
        );
        time.unpause();
        *owned_pause = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ReviewClockPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                20,
            )));
        app.update();
        app.update();
        app
    }

    #[test]
    fn delayed_readbacks_hold_virtual_time_but_leave_watchdogs_running() {
        let mut app = app();
        let before = app.world().resource::<Time>().elapsed();
        let real = app.world().resource::<Time<Real>>().elapsed();
        let first = app.world_mut().spawn(Screenshot::primary_window()).id();
        let second = app.world_mut().spawn(Screenshot::primary_window()).id();
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(app.world().resource::<Time>().elapsed(), before);
        assert_eq!(app.world().resource::<Time>().delta(), Duration::ZERO);
        assert!(app.world().resource::<Time<Real>>().elapsed() > real);
        app.world_mut().despawn(first);
        app.update();
        assert_eq!(app.world().resource::<Time>().elapsed(), before);
        app.world_mut().despawn(second);
        app.update();
        assert_eq!(
            app.world().resource::<Time>().elapsed(),
            before + Duration::from_millis(20)
        );
    }

    #[test]
    fn a_capture_does_not_resume_a_preexisting_pause() {
        let mut app = app();
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        let capture = app.world_mut().spawn(Screenshot::primary_window()).id();
        app.update();
        app.world_mut().despawn(capture);
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(app.world().resource::<Time>().delta(), Duration::ZERO);
    }
    #[test]
    fn fixed_review_steps_ignore_wall_time_and_readback_delays() {
        for delay in [1, 37, 900] {
            let mut app = app();
            app.insert_resource(ReviewStep::new(50).unwrap());
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                delay,
            )));
            let before = app.world().resource::<Time>().elapsed();
            for _ in 0..5 {
                app.update();
            }
            assert_eq!(
                app.world().resource::<Time>().elapsed(),
                before + Duration::from_millis(100)
            );
            let held = app.world().resource::<Time>().elapsed();
            let real = app.world().resource::<Time<Real>>().elapsed();
            let capture = app.world_mut().spawn(Screenshot::primary_window()).id();
            for _ in 0..7 {
                app.update();
            }
            assert_eq!(app.world().resource::<Time>().elapsed(), held);
            assert!(app.world().resource::<Time<Real>>().elapsed() > real);
            app.world_mut().despawn(capture);
            app.update();
            assert_eq!(
                app.world().resource::<Time>().elapsed(),
                held + Duration::from_millis(20)
            );
        }
        for hz in [0, 19, 241, u32::MAX] {
            assert!(ReviewStep::new(hz).is_err());
        }
        for hz in [20, 60, 240] {
            assert!(ReviewStep::new(hz).is_ok());
        }
    }
}
