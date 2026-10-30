//! Sampling for closed organic masses, separate from fitted shell resolution.

/// Keep a useful number of cells across small features while capping the work
/// for large masses. Projected surface-net vertices recover the actual contour.
pub(super) fn closed_surface_step(feature_radius: f32) -> f32 {
    assert!(feature_radius.is_finite() && feature_radius > 0.0);
    (feature_radius * 0.3).min(0.01)
}
