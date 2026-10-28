//! `--frame-stats SECS`: live frame-time measurement. Once the world first
//! appears it waits until the scene has run SETTLE_SECS without a stall,
//! noting the longest frame meanwhile (creature construction stalls show
//! there), then times every frame for SECS and prints one JSON report of
//! frame times, animation work, per-pass render times and what was drawn.
use super::*;
use bevy::render::mesh::skinning::SkinnedMesh;

/// Seconds without a stall before frames are timed.
pub(crate) const SETTLE_SECS: f32 = 5.0;
/// A frame this long restarts the settling wait.
pub(crate) const STALL_MS: f32 = 250.0;

/// Frame timing for one run.
#[derive(Resource)]
pub(crate) struct FrameStats {
    measure_secs: f32,
    /// When the scene last stalled (or first appeared), in real seconds.
    settling_since: Option<f32>,
    /// When timing began.
    timing_since: Option<f32>,
    worst_settling_ms: f32,
    frames_ms: Vec<f32>,
    /// Animation work summed over the timed frames: pose/joints, robe
    /// fitting, drapes/segments and hair/contact fitting, then hair's own
    /// control, deformation, constraint and mesh-write stages.
    motion_ms: [f64; 4],
    hair_ms: [f64; 4],
    rigs: usize,
}

impl FrameStats {
    pub(crate) fn new(measure_secs: f32) -> Self {
        Self {
            measure_secs,
            settling_since: None,
            timing_since: None,
            worst_settling_ms: 0.0,
            frames_ms: Vec::new(),
            motion_ms: [0.0; 4],
            hair_ms: [0.0; 4],
            rigs: 0,
        }
    }
}

/// Mean, percentiles and extremes of a set of frame times in milliseconds.
#[derive(Debug, PartialEq)]
pub(crate) struct Summary {
    pub frames: usize,
    pub mean_ms: f32,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
}

pub(crate) fn summarize(frames_ms: &[f32]) -> Option<Summary> {
    if frames_ms.is_empty() {
        return None;
    }
    let mut sorted = frames_ms.to_vec();
    sorted.sort_by(f32::total_cmp);
    // Nearest-rank percentile.
    let pick =
        |q: f32| sorted[((q * sorted.len() as f32).ceil() as usize).clamp(1, sorted.len()) - 1];
    Some(Summary {
        frames: sorted.len(),
        mean_ms: sorted.iter().sum::<f32>() / sorted.len() as f32,
        p50_ms: pick(0.5),
        p95_ms: pick(0.95),
        p99_ms: pick(0.99),
        max_ms: sorted[sorted.len() - 1],
    })
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Drawn<'w, 's> {
    bodies: Query<'w, 's, &'static ActorBody>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
    skinned: Query<'w, 's, (), With<SkinnedMesh>>,
    lamps: Query<'w, 's, &'static PointLight>,
}

pub(crate) fn frame_stats(
    time: Res<Time<Real>>,
    game: Res<Game>,
    stats: Option<ResMut<FrameStats>>,
    motion: Option<Res<creatures::MotionProfile>>,
    drawn: Drawn,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut exit: EventWriter<AppExit>,
) {
    let Some(mut stats) = stats else {
        return;
    };
    let now = time.elapsed_secs();
    let frame_ms = time.delta_secs() * 1000.0;
    if game.view.is_none() {
        return;
    }
    if stats.timing_since.is_none() {
        let since = *stats.settling_since.get_or_insert(now);
        stats.worst_settling_ms = stats.worst_settling_ms.max(frame_ms);
        if frame_ms > STALL_MS {
            stats.settling_since = Some(now);
        } else if now >= since + SETTLE_SECS {
            stats.timing_since = Some(now);
        }
        return;
    }
    let timing_since = stats.timing_since.expect("timing began");
    if now < timing_since + stats.measure_secs {
        stats.frames_ms.push(frame_ms);
        if let Some(motion) = motion {
            for (sum, phase) in stats.motion_ms.iter_mut().zip(motion.phases) {
                *sum += phase.as_secs_f64() * 1000.0;
            }
            for (sum, phase) in stats.hair_ms.iter_mut().zip(motion.hair_phases) {
                *sum += phase.as_secs_f64() * 1000.0;
            }
            stats.rigs = motion.rigs;
        }
        return;
    }
    let per_frame = |sums: [f64; 4]| sums.map(|sum| sum / stats.frames_ms.len().max(1) as f64);
    let Some(summary) = summarize(&stats.frames_ms) else {
        return;
    };
    println!(
        "{}",
        serde_json::json!({
            "type": "3d_frame_stats",
            "frames": summary.frames,
            "mean_ms": summary.mean_ms,
            "fps": 1000.0 / summary.mean_ms,
            "p50_ms": summary.p50_ms,
            "p95_ms": summary.p95_ms,
            "p99_ms": summary.p99_ms,
            "max_ms": summary.max_ms,
            "worst_settling_ms": stats.worst_settling_ms,
            "creatures": drawn.bodies.iter().count(),
            "mesh_entities": drawn.meshes.iter().count(),
            "skinned_meshes": drawn.skinned.iter().count(),
            "point_lights": drawn.lamps.iter().count(),
            "shadowed_lights": drawn.lamps.iter().filter(|lamp| lamp.shadows_enabled).count(),
            "posed_rigs": stats.rigs,
            "motion_ms_per_frame": {
                "pose": per_frame(stats.motion_ms)[0], "robes": per_frame(stats.motion_ms)[1],
                "drapes": per_frame(stats.motion_ms)[2], "hair": per_frame(stats.motion_ms)[3],
            },
            "hair_ms_per_frame": per_frame(stats.hair_ms),
            "passes_ms": passes(&diagnostics),
        })
    );
    exit.write(AppExit::Success);
}

/// Recent average GPU and CPU time of each render pass, from Bevy's render
/// diagnostics; empty where the device cannot time passes.
fn passes(
    diagnostics: &bevy::diagnostic::DiagnosticsStore,
) -> serde_json::Map<String, serde_json::Value> {
    let mut passes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| {
            let path = d.path().as_str();
            let pass = path.strip_prefix("render/")?;
            Some((pass.to_owned(), d.average()?))
        })
        .collect();
    passes.sort_by(|a, b| a.0.cmp(&b.0));
    passes
        .into_iter()
        .map(|(pass, ms)| (pass, serde_json::json!((ms * 100.0).round() / 100.0)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_use_nearest_rank_percentiles() {
        let frames: Vec<f32> = (1..=100).map(|ms| ms as f32).collect();
        let summary = summarize(&frames).unwrap();
        assert_eq!(summary.frames, 100);
        assert_eq!(summary.mean_ms, 50.5);
        assert_eq!(
            (
                summary.p50_ms,
                summary.p95_ms,
                summary.p99_ms,
                summary.max_ms
            ),
            (50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(summarize(&[7.0]).unwrap().p95_ms, 7.0);
        assert!(summarize(&[]).is_none());
    }
}
