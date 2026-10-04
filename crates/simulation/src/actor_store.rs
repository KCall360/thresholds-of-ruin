//! Authoritative actors, anchor-region membership and derived portal-local bodies.
//! Mutations cannot escape through a mutable map. Geometry witnesses and dirty
//! identities control reuse; neither participates in saved state or equality.
use crate::navigation_map::RegionMap;
use crate::{Actor, ActorId, BodySpec};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use tor_world::{GeometrySnapshot, Location, Position, RegionId, Shared, World};

type Members = Shared<BTreeMap<RegionId, Shared<BTreeSet<ActorId>>>>;
type Occupants = Shared<BTreeMap<ActorId, u8>>;
pub(crate) type BodyCells = Shared<Vec<(Location, u8)>>;
type PerceivedBodies = Vec<(ActorId, BodyCells)>;

fn add_member(regions: &mut Members, region: RegionId, id: ActorId) {
    regions.entry(region).or_default().insert(id);
}

fn remove_member(regions: &mut Members, region: RegionId, id: ActorId) {
    let members = regions.get_mut(&region).expect("indexed actor region");
    assert!(members.remove(&id), "indexed actor identity");
    if members.is_empty() {
        regions.remove(&region);
    }
}

#[derive(Clone)]
struct BodyEntry {
    location: Location,
    orientation: u8,
    body: Shared<BodySpec>,
    cells: Option<BodyCells>,
}

impl BodyEntry {
    fn matches(&self, actor: &Actor) -> bool {
        self.location == actor.location
            && self.orientation == actor.orientation
            && self.body == actor.body
    }
}

#[derive(Clone, Default)]
struct Bodies {
    geometry: Option<GeometrySnapshot>,
    entries: Shared<BTreeMap<ActorId, Shared<BodyEntry>>>,
    cells: Shared<RegionMap<Location, Occupants>>,
    dirty: BTreeSet<ActorId>,
}

impl Bodies {
    fn refresh(&mut self, actors: &BTreeMap<ActorId, Actor>, world: &World) {
        let geometry = world.geometry_snapshot();
        if self.geometry.as_ref() != Some(&geometry) {
            *self = Self {
                geometry: Some(geometry),
                dirty: actors.keys().copied().collect(),
                ..Self::default()
            };
        }
        for id in std::mem::take(&mut self.dirty) {
            let actor = actors.get(&id);
            if actor
                .zip(self.entries.get(&id))
                .is_some_and(|(actor, entry)| entry.matches(actor))
            {
                continue;
            }
            if let Some(old) = self.entries.remove(&id) {
                for &(at, _) in old.cells.iter().flat_map(|cells| cells.iter()) {
                    let occupants = self.cells.get_mut(&at).expect("indexed body cell");
                    assert!(occupants.remove(&id).is_some(), "indexed body identity");
                    if occupants.is_empty() {
                        self.cells.remove(&at);
                    }
                }
            }
            if let Some(actor) = actor {
                let cells = crate::physics::resolve_body(
                    world,
                    actor.location,
                    actor.orientation,
                    &actor.body,
                );
                for &(at, frame) in cells.iter().flatten() {
                    if !self.cells.contains_key(&at) {
                        self.cells.insert(at, Shared::default());
                    }
                    self.cells
                        .get_mut(&at)
                        .expect("inserted body cell")
                        .insert(id, frame);
                }
                self.entries.insert(
                    id,
                    Shared::new(BodyEntry {
                        location: actor.location,
                        orientation: actor.orientation,
                        body: actor.body.clone(),
                        cells: cells.map(Shared::new),
                    }),
                );
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct ActorStore {
    entries: Shared<BTreeMap<ActorId, Actor>>,
    regions: Members,
    bodies: Mutex<Bodies>,
}

impl Clone for ActorStore {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            regions: self.regions.clone(),
            bodies: Mutex::new(
                self.bodies
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            ),
        }
    }
}

impl std::fmt::Debug for ActorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorStore")
            .field("entries", &self.entries)
            .finish()
    }
}

impl PartialEq for ActorStore {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}
impl Eq for ActorStore {}

impl Deref for ActorStore {
    type Target = BTreeMap<ActorId, Actor>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl ActorStore {
    pub fn from_entries(entries: BTreeMap<ActorId, Actor>) -> Self {
        let mut regions = Members::default();
        for (&id, actor) in &entries {
            add_member(&mut regions, actor.location.region, id);
        }
        Self {
            entries: Shared::new(entries),
            regions,
            bodies: Mutex::default(),
        }
    }

    pub fn raw_entries(&self) -> &BTreeMap<ActorId, Actor> {
        &self.entries
    }

    pub fn in_region(&self, region: RegionId) -> impl Iterator<Item = ActorId> + '_ {
        self.regions
            .get(&region)
            .into_iter()
            .flat_map(|members| members.iter().copied())
    }

    /// Includes every loaded actor's body, including corpses. Callers apply their
    /// existing life/freeze rules against authoritative entries after lookup.
    pub fn at(&self, world: &World, location: Location) -> Occupants {
        let mut bodies = self.bodies.lock().unwrap_or_else(|e| e.into_inner());
        bodies.refresh(&self.entries, world);
        bodies.cells.get(&location).cloned().unwrap_or_default()
    }

    /// Ordered identities and authored body-cell order. Query only visible
    /// regions, using the smaller of their occupied and visible cell sets.
    pub fn perceived(&self, world: &World, visible: &BTreeSet<Location>) -> PerceivedBodies {
        let mut bodies = self.bodies.lock().unwrap_or_else(|e| e.into_inner());
        bodies.refresh(&self.entries, world);
        let mut ids = BTreeSet::new();
        let mut previous_region = None;
        for at in visible {
            let region = at.region;
            if previous_region == Some(region) {
                continue;
            }
            previous_region = Some(region);
            let Some(occupied) = bodies.cells.region(region) else {
                continue;
            };
            let region_view = visible.range(
                Location {
                    region,
                    position: Position {
                        x: i32::MIN,
                        y: i32::MIN,
                        z: i32::MIN,
                    },
                }..=Location {
                    region,
                    position: Position {
                        x: i32::MAX,
                        y: i32::MAX,
                        z: i32::MAX,
                    },
                },
            );
            if region_view.clone().take(occupied.len()).count() == occupied.len() {
                for (_, occupants) in occupied.iter().filter(|(at, _)| visible.contains(at)) {
                    ids.extend(occupants.keys().copied());
                }
            } else {
                for occupants in region_view.filter_map(|at| occupied.get(at)) {
                    ids.extend(occupants.keys().copied());
                }
            }
        }
        crate::diagnostics::actor_candidates(ids.len());
        ids.into_iter()
            .filter_map(|id| Some((id, bodies.entries.get(&id)?.cells.clone()?)))
            .collect()
    }

    pub fn insert(&mut self, id: ActorId, actor: Actor) -> Option<Actor> {
        let region = actor.location.region;
        let old = self.entries.insert(id, actor);
        if let Some(old) = &old {
            remove_member(&mut self.regions, old.location.region, id);
        }
        add_member(&mut self.regions, region, id);
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(id);
        old
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<Actor> {
        let old = self.entries.remove(id)?;
        remove_member(&mut self.regions, old.location.region, *id);
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(*id);
        Some(old)
    }

    pub fn extend(&mut self, entries: impl IntoIterator<Item = (ActorId, Actor)>) {
        for (id, actor) in entries {
            self.insert(id, actor);
        }
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<ActorEdit<'_>> {
        let actor = self.entries.get_mut(id)?;
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(*id);
        Some(ActorEdit {
            id: *id,
            before: actor.location.region,
            actor,
            regions: &mut self.regions,
        })
    }
}

pub(crate) struct ActorEdit<'a> {
    id: ActorId,
    before: RegionId,
    actor: &'a mut Actor,
    regions: &'a mut Members,
}
impl Deref for ActorEdit<'_> {
    type Target = Actor;
    fn deref(&self) -> &Self::Target {
        self.actor
    }
}
impl DerefMut for ActorEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.actor
    }
}
impl Drop for ActorEdit<'_> {
    fn drop(&mut self) {
        if self.before != self.actor.location.region {
            remove_member(self.regions, self.before, self.id);
            add_member(self.regions, self.actor.location.region, self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Game;
    use std::num::NonZeroU64;
    use tor_world::{Direction, Extent, Passage, Position, Region};

    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }

    fn game() -> Game {
        let mut world = World::new(vec![], vec![]).unwrap();
        for region in [1, 2] {
            world
                .add_region(Region {
                    id: RegionId(region),
                    name: format!("room-{region}"),
                    bounds: Extent::new(16, 16, 2).unwrap(),
                })
                .unwrap();
        }
        let mut game = Game::new(world, 42);
        game.spawn_actor(at(1, 15), NonZeroU64::new(100).unwrap())
            .unwrap();
        game
    }

    fn assert_reference(game: &Game) {
        for region in [1, 2] {
            let members: Vec<_> = game
                .actors
                .iter()
                .filter(|(_, a)| a.location.region == RegionId(region))
                .map(|(&id, _)| id)
                .collect();
            assert_eq!(
                game.actors.in_region(RegionId(region)).collect::<Vec<_>>(),
                members
            );
            for x in 0..16 {
                let location = at(region, x);
                let expected: BTreeMap<_, _> = game
                    .actors
                    .iter()
                    .filter_map(|(&id, actor)| {
                        crate::physics::resolve_body(
                            &game.world,
                            actor.location,
                            actor.orientation,
                            &actor.body,
                        )?
                        .into_iter()
                        .find(|(cell, _)| *cell == location)
                        .map(|(_, frame)| (id, frame))
                    })
                    .collect();
                assert_eq!(&*game.actors.at(&game.world, location), &expected);
                let visible = BTreeSet::from([location]);
                let perceived = game.actors.perceived(&game.world, &visible);
                assert_eq!(
                    perceived.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    expected.keys().copied().collect::<Vec<_>>()
                );
                for (id, cells) in perceived {
                    let actor = &game.actors[&id];
                    assert_eq!(
                        &*cells,
                        &crate::physics::resolve_body(
                            &game.world,
                            actor.location,
                            actor.orientation,
                            &actor.body
                        )
                        .unwrap()
                    );
                }
            }
        }
    }

    #[test]
    fn bodies_follow_portal_frames_geometry_edits_and_mutations() {
        let mut game = game();
        // Exercise a cached unresolved body before the portal exists.
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .body
            .cells
            .push([1, 0, 0]);
        assert_reference(&game);
        assert!(game.actors.at(&game.world, at(1, 15)).is_empty());
        game.connect(
            Passage {
                from: at(1, 15),
                direction: Direction::East,
                to: at(2, 0),
            },
            1,
        )
        .unwrap();
        assert_reference(&game);
        let portal_frame = game.actors.at(&game.world, at(2, 0))[&ActorId(1)];
        assert_ne!(portal_frame, 0);
        assert!(game.actors.in_region(RegionId(2)).next().is_none());
        let old = game.clone();
        game.teleport(ActorId(1), at(2, 4)).unwrap();
        assert_reference(&game);
        assert_reference(&old);
        assert_eq!(
            old.actors.at(&old.world, at(2, 0))[&ActorId(1)],
            portal_frame
        );
        assert!(game.actors.at(&game.world, at(2, 0)).is_empty());
        game.set_body(ActorId(1), BodySpec::default()).unwrap();
        assert_reference(&game);
        let entries = game.actors.raw_entries().clone();
        game.actors = ActorStore::from_entries(entries);
        assert_reference(&game);
        game.actors.remove(&ActorId(1)).unwrap();
        assert_reference(&game);
    }

    #[test]
    fn interrupted_edits_still_update_region_membership_and_body_queries() {
        let mut game = game();
        assert_reference(&game);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut actor = game.actors.get_mut(&ActorId(1)).unwrap();
            actor.location = at(2, 5);
            panic!("interrupted edit");
        }));
        assert!(result.is_err());
        assert_reference(&game);
        assert!(game.actors.in_region(RegionId(1)).next().is_none());
        assert_eq!(
            game.actors.in_region(RegionId(2)).collect::<Vec<_>>(),
            vec![ActorId(1)]
        );
    }

    #[test]
    fn disclosure_preserves_identity_and_authored_cell_order_for_partial_views() {
        let mut game = game();
        game.connect(
            Passage {
                from: at(1, 15),
                direction: Direction::East,
                to: at(2, 0),
            },
            1,
        )
        .unwrap();
        game.set_body(
            ActorId(1),
            BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                ..BodySpec::default()
            },
        )
        .unwrap();
        let other = game
            .spawn_actor(at(2, 7), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(
            other,
            BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]],
                ..BodySpec::default()
            },
        )
        .unwrap();
        let samples = [at(1, 15), at(2, 0), at(2, 7), at(2, 8), at(1, 0)];
        for mask in 0..1 << samples.len() {
            let visible: BTreeSet<_> = samples
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, &at)| at)
                .collect();
            let expected: Vec<_> = game
                .actors
                .iter()
                .filter_map(|(&id, actor)| {
                    let cells = crate::physics::resolve_body(
                        &game.world,
                        actor.location,
                        actor.orientation,
                        &actor.body,
                    )?;
                    cells
                        .iter()
                        .any(|(at, _)| visible.contains(at))
                        .then_some((id, cells))
                })
                .collect();
            let actual: Vec<_> = game
                .actors
                .perceived(&game.world, &visible)
                .into_iter()
                .map(|(id, cells)| (id, (*cells).clone()))
                .collect();
            assert_eq!(actual, expected, "mask: {mask}");
        }
    }
}
