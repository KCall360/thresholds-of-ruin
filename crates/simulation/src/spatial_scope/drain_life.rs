//! Exact scope-local Drain membership, independent of sight and room lighting.
use crate::{Game, GameError};
use std::collections::BTreeSet;
use tor_world::{Location, RegionId};

impl Game {
    pub(crate) fn drain_life_regions(&self, at: Location) -> Result<BTreeSet<RegionId>, GameError> {
        let scope = self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&at.region).is_ok())
            .ok_or(GameError::InvalidLocation)?;
        self.spatial_registry
            .lookup(&scope.id, at)
            .ok_or(GameError::InvalidLocation)?;
        Ok(scope.members.iter().copied().collect())
    }
    pub(crate) fn apply_drain_life(
        &mut self,
        _outcome: &crate::abilities::Outcome,
        _activity: &mut crate::lifecycle::DamageActivityGuard,
    ) -> Result<bool, GameError> {
        // Intentional source-first seam: paid setup and strict adapters are real,
        // but the area effect is absent until the grouped acceptance red is read.
        // Complete implementation is protected in the preceding checkpoint.
        Ok(false)
    }
}
