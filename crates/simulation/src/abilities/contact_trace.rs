//! Pure first-body discovery for an unpublished contact release.
//! No checks, RNG, payment, activation, loading or effect application live here.
use crate::{ActorId, Game};
use std::collections::BTreeSet;
use tor_world::{Direction, Location, RayStep, RegionId};

/// Operational work ceiling, not a distance or source-geometry admission limit.
/// A longer nonrepeating search fails atomically; it never publishes a miss.
pub(crate) const MAX_CONTACT_STATES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContactEnd {
    Body(ActorId),
    Obstruction,
    Frontier,
    Cycle,
    NeedsRegions(BTreeSet<RegionId>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContactTrace {
    pub end: ContactEnd,
    /// Retain traversed geometry through all load-only closure iterations.
    /// These are residency dependencies, never ACTIVE population roots.
    pub regions: BTreeSet<RegionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContactTraceError {
    InvalidOrigin,
    WorkLimit,
}

impl Game {
    /// `max_states` is an explicit execution-resource guard, not gameplay range.
    /// Its production value must be selected independently of sight/bounce caps.
    /// Error must propagate through the enclosing command transaction unchanged.
    pub(crate) fn trace_first_body(
        &self,
        actor: ActorId,
        origin: Location,
        mut direction: Direction,
        launch_footprint: &[Location],
        max_states: usize,
    ) -> Result<ContactTrace, ContactTraceError> {
        if launch_footprint.is_empty()
            || !launch_footprint.windows(2).all(|pair| pair[0] < pair[1])
            || launch_footprint.binary_search(&origin).is_err()
            || !self.actors.contains_key(&actor)
        {
            return Err(ContactTraceError::InvalidOrigin);
        }
        let mut at = origin;
        let mut exited_source = false;
        let mut seen = BTreeSet::new();
        let mut regions = BTreeSet::from([origin.region]);
        let end =
            loop {
                let state = (at, direction, exited_source);
                if seen.contains(&state) {
                    break ContactEnd::Cycle;
                }
                if seen.len() == max_states {
                    return Err(ContactTraceError::WorkLimit);
                }
                seen.insert(state);
                let (step, dependencies) = self.world.ray_step_dependencies(at, direction);
                regions.extend(dependencies.iter().copied());
                let missing: BTreeSet<_> = dependencies
                    .into_iter()
                    .filter(|region| self.world.region(*region).is_none())
                    .collect();
                if !missing.is_empty() {
                    regions.extend(missing.iter().copied());
                    break ContactEnd::NeedsRegions(missing);
                }
                match step {
                    RayStep::Solid | RayStep::Ambiguous => break ContactEnd::Obstruction,
                    RayStep::Unavailable => break ContactEnd::Frontier,
                    RayStep::Clear { to, rotation } => {
                        at = to;
                        direction = direction.rotated(rotation);
                        regions.insert(at.region);
                        if launch_footprint.binary_search(&at).is_err() {
                            exited_source = true;
                        }
                        // Contact is occupancy, not combat eligibility or observer sight.
                        // Loaded frozen bodies and noncombat bodies are real interceptors.
                        if let Some(target) =
                            self.actors.at(&self.world, at).keys().copied().find(|id| {
                                self.actors[id].alive() && (*id != actor || exited_source)
                            })
                        {
                            break ContactEnd::Body(target);
                        }
                    }
                }
            };
        Ok(ContactTrace { end, regions })
    }
}
