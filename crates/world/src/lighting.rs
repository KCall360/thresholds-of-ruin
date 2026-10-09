//! Boolean ambient illumination, independent of terrain and observer geometry.
use crate::{Location, Position, RegionId, Shared, World, WorldError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegionLight {
    pub(crate) default: bool,
    #[serde(with = "crate::checkpoint_map::shared")]
    pub(crate) cells: Shared<BTreeMap<Position, bool>>,
}
impl Default for RegionLight {
    fn default() -> Self {
        Self {
            default: true,
            cells: Shared::default(),
        }
    }
}
impl World {
    /// None denotes unloaded or nonexistent geometry, never a dark wall.
    pub fn is_lit(&self, at: Location) -> Option<bool> {
        self.contains(at).then(|| {
            self.lighting.get(&at.region).is_none_or(|light| {
                light
                    .cells
                    .get(&at.position)
                    .copied()
                    .unwrap_or(light.default)
            })
        })
    }
    /// Set every cell in a loaded region, removing prior overrides.
    pub fn set_region_light(&mut self, region: RegionId, lit: bool) -> Result<(), WorldError> {
        if self.region(region).is_none() {
            return Err(WorldError::InvalidEndpoint);
        }
        if lit {
            self.lighting.remove(&region);
        } else {
            self.lighting.insert(
                region,
                RegionLight {
                    default: false,
                    cells: Shared::default(),
                },
            );
        }
        self.sight.lighting_changed(region);
        Ok(())
    }
    pub fn set_cell_light(&mut self, at: Location, lit: bool) -> Result<(), WorldError> {
        if !self.contains(at) {
            return Err(WorldError::InvalidEndpoint);
        }
        let light = self.lighting.entry(at.region).or_default();
        if lit == light.default {
            light.cells.remove(&at.position);
        } else {
            light.cells.insert(at.position, lit);
        }
        self.sight.lighting_changed(at.region);
        Ok(())
    }
}
