//! Item-owned finite fuel and physical portable illumination.
//! Ambient World lighting and item identity remain independent authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeldLightSpec {
    pub radius: u8,
    pub fuel_ticks: u64,
}
impl HeldLightSpec {
    pub fn valid(self) -> bool {
        (1..=crate::limits::SIGHT_RANGE as u8).contains(&self.radius)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.fuel_ticks)
    }
}
impl<'de> Deserialize<'de> for HeldLightSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: u8,
            fuel_ticks: u64,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            radius: fields.radius,
            fuel_ticks: fields.fuel_ticks,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid held light policy"));
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FuelState {
    pub remaining: u64,
    pub lit: bool,
}
impl<'de> Deserialize<'de> for FuelState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            remaining: u64,
            lit: bool,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.remaining > crate::limits::NUMERIC as u64 || (fields.lit && fields.remaining == 0)
        {
            return Err(serde::de::Error::custom("invalid portable fuel"));
        }
        Ok(Self {
            remaining: fields.remaining,
            lit: fields.lit,
        })
    }
}

/// Controlled inventory projection; no source radius or concealed identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortableLightView {
    pub remaining: u64,
    pub lit: bool,
    pub ignitable: bool,
    pub extinguishable: bool,
}

pub(crate) fn initial_fuel(spec: &crate::ItemSpec) -> Option<FuelState> {
    spec.held_light.map(|policy| FuelState {
        remaining: policy.fuel_ticks,
        lit: false,
    })
}
pub(crate) fn valid_item(item: &crate::Item) -> bool {
    match (item.spec.held_light, item.fuel) {
        (None, None) => true,
        (Some(policy), Some(fuel)) => {
            policy.valid()
                && item.quantity == 1
                && fuel.remaining <= policy.fuel_ticks
                && (!fuel.lit || fuel.remaining > 0)
        }
        _ => false,
    }
}

impl crate::Game {
    pub(crate) fn light_work_available(
        &self,
        actor: crate::ActorId,
        item: crate::ItemId,
        lit: bool,
    ) -> bool {
        let Some(owner) = self.actors.get(&actor).filter(|owner| owner.alive()) else {
            return false;
        };
        let Some(gear) = self.items.get(&item) else {
            return false;
        };
        gear.location == crate::ItemLocation::Carried(actor)
            && owner.equipment.values().any(|held| *held == item)
            && gear.spec.held_light.is_some()
            && gear
                .fuel
                .is_some_and(|fuel| fuel.remaining > 0 && fuel.lit != lit)
    }
    pub(crate) fn portable_light_view(
        &self,
        actor: crate::ActorId,
        item: crate::ItemId,
    ) -> Option<PortableLightView> {
        let fuel = self.items.get(&item)?.fuel?;
        Some(PortableLightView {
            remaining: fuel.remaining,
            lit: fuel.lit,
            ignitable: self.light_work_available(actor, item, true),
            extinguishable: self.light_work_available(actor, item, false),
        })
    }
    pub(crate) fn set_item_light(
        &mut self,
        item: crate::ItemId,
        lit: bool,
    ) -> Result<(), crate::GameError> {
        let fuel = self
            .items
            .get(&item)
            .and_then(|item| item.fuel)
            .ok_or(crate::GameError::ItemUnavailable)?;
        if lit && fuel.remaining == 0 {
            return Err(crate::GameError::ItemUnavailable);
        }
        self.items
            .edit(item, |item| {
                item.fuel.as_mut().expect("validated fuel").lit = lit
            })
            .ok_or(crate::GameError::ItemUnavailable)?;
        self.refresh_light_boundary();
        Ok(())
    }
}

use std::collections::BTreeSet;
use tor_world::{Location, RegionId};

/// Unpublished host-only request. This carries no activation or population authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightRegionBoundary {
    pub tick: u64,
    pub required: BTreeSet<RegionId>,
    pub optical: Option<LightOpticalWitness>,
}
/// Exact transient optical dependencies, not saved state or an authority token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightOpticalWitness {
    world: tor_world::PerceptionSnapshot,
    emitters: EmitterSignature,
    actors: Vec<(crate::ActorId, crate::observation::ActorSceneWitness)>,
    references: Vec<LightReferenceWitness>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct LightReferenceWitness {
    id: crate::ReferencePointId,
    point: crate::ReferencePoint,
    eye: Option<(Location, u8, Vec<RegionId>)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmitterSignature(pub Vec<(crate::ItemId, Location, u8, u8)>);

impl crate::Game {
    pub(crate) fn light_optical_witness(&self) -> Option<LightOpticalWitness> {
        let emitters = self.emitter_signature();
        if emitters.0.is_empty() {
            return None;
        }
        Some(LightOpticalWitness {
            world: self.world.perception_snapshot(),
            emitters,
            actors: self
                .actors
                .keys()
                .map(|id| (*id, self.actor_scene_witness(*id).expect("loaded actor")))
                .collect(),
            references: self
                .reference_points()
                .map(|(id, point)| LightReferenceWitness {
                    id,
                    point: *point,
                    eye: self.observer_eye(point.target, &|_| true),
                })
                .collect(),
        })
    }
    pub(crate) fn emitter_signature(&self) -> EmitterSignature {
        EmitterSignature(
            self.items
                .iter()
                .filter_map(|(id, item)| {
                    if !item.fuel.is_some_and(|fuel| fuel.lit && fuel.remaining > 0) {
                        return None;
                    }
                    let policy = item.spec.held_light?;
                    let (at, frame) = match item.location {
                        crate::ItemLocation::Ground(at) => (at, item.orientation),
                        crate::ItemLocation::Carried(owner) => {
                            let actor = self.actors.get(&owner)?;
                            if !actor.alive() || !actor.equipment.values().any(|held| held == id) {
                                return None;
                            }
                            (actor.location, actor.orientation)
                        }
                    };
                    self.world
                        .region(at.region)
                        .map(|_| (*id, at, frame, policy.radius))
                })
                .collect(),
        )
    }
    pub(crate) fn portable_lit_cells(&self) -> BTreeSet<Location> {
        self.emitter_signature()
            .0
            .into_iter()
            .flat_map(|(_, at, frame, radius)| {
                self.world
                    .eye_scene(at, frame, radius)
                    .into_iter()
                    .map(|cell| cell.location)
            })
            .collect()
    }
    pub(crate) fn illuminated_scene(
        &self,
        at: Location,
        frame: u8,
        radius: u8,
    ) -> Vec<tor_world::SightCell> {
        let portable = self.portable_lit_cells();
        if portable.is_empty() {
            return self.world.illuminated_eye_scene(at, frame, radius);
        }
        self.world
            .eye_scene(at, frame, radius)
            .into_iter()
            .filter(|cell| {
                self.world.is_lit(cell.location) == Some(true) || portable.contains(&cell.location)
            })
            .collect()
    }
    pub(crate) fn light_required_regions(&self) -> BTreeSet<RegionId> {
        self.emitter_signature()
            .0
            .into_iter()
            .flat_map(|(_, at, frame, radius)| {
                self.world
                    .eye_scene_regions(at, frame, radius)
                    .unwrap_or_default()
            })
            .collect()
    }
    pub fn light_region_boundary(&self) -> Option<&LightRegionBoundary> {
        self.light_region_boundary.as_ref()
    }
    pub fn light_missing_regions(&self, boundary: &LightRegionBoundary) -> BTreeSet<RegionId> {
        boundary
            .required
            .iter()
            .filter(|id| self.world.region(**id).is_none())
            .copied()
            .collect()
    }
    pub(crate) fn refresh_light_boundary(&mut self) {
        if !self.host_region_continuation {
            return;
        }
        let optical = self.light_optical_witness();
        let changed = self.light_settled_witness != optical;
        let boundary = LightRegionBoundary {
            tick: self.tick,
            required: self.light_required_regions(),
            optical,
        };
        self.light_region_boundary =
            (changed || !self.light_missing_regions(&boundary).is_empty()).then_some(boundary);
    }
    fn fuel_burns(&self, id: crate::ItemId, item: &crate::Item) -> bool {
        if !item.fuel.is_some_and(|fuel| fuel.lit && fuel.remaining > 0) {
            return false;
        }
        let at = match item.location {
            crate::ItemLocation::Ground(at) => at,
            crate::ItemLocation::Carried(owner) => {
                let Some(actor) = self.actors.get(&owner) else {
                    return false;
                };
                if !actor.alive() || !actor.equipment.values().any(|held| *held == id) {
                    return false;
                }
                actor.location
            }
        };
        self.region_state(at.region) == Some(crate::RegionState::Active)
    }
    pub(crate) fn next_fuel_expiry(&self) -> Option<u64> {
        self.items
            .iter()
            .filter(|(id, item)| self.fuel_burns(**id, item))
            .filter_map(|(_, item)| {
                self.tick
                    .checked_add(item.fuel.expect("lit fuel").remaining)
            })
            .min()
    }
    /// Consume the interval at its old physical location, before same-tick moves.
    pub(crate) fn burn_light_fuel(&mut self, elapsed: u64) {
        if elapsed == 0 {
            return;
        }
        let burning: Vec<_> = self
            .items
            .iter()
            .filter(|(id, item)| self.fuel_burns(**id, item))
            .map(|(id, _)| *id)
            .collect();
        for id in burning {
            self.items.edit(id, |item| {
                let fuel = item.fuel.as_mut().expect("lit fuel");
                fuel.remaining = fuel.remaining.saturating_sub(elapsed);
                if fuel.remaining == 0 {
                    fuel.lit = false;
                }
            });
        }
    }
}
