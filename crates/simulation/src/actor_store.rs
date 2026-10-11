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
    timer_deadlines: Shared<BTreeSet<(u64, ActorId)>>,
    suspended_timers: Shared<BTreeSet<ActorId>>,
    bodies: Mutex<Bodies>,
}

impl Clone for ActorStore {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            regions: self.regions.clone(),
            timer_deadlines: self.timer_deadlines.clone(),
            suspended_timers: self.suspended_timers.clone(),
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
        let mut timer_deadlines = BTreeSet::new();
        for (&id, actor) in &entries {
            add_member(&mut regions, actor.location.region, id);
            if let Some(interval) = next_timer(actor) {
                timer_deadlines.insert((interval, id));
            }
        }
        Self {
            entries: Shared::new(entries),
            regions,
            timer_deadlines: Shared::new(timer_deadlines),
            suspended_timers: Shared::default(),
            bodies: Mutex::default(),
        }
    }

    #[cfg(test)]
    pub fn raw_entries(&self) -> &BTreeMap<ActorId, Actor> {
        &self.entries
    }

    pub fn scheduled_timers(&self) -> impl Iterator<Item = (u64, ActorId)> + '_ {
        self.timer_deadlines
            .iter()
            .copied()
            .inspect(|_| crate::diagnostics::timer_candidate())
    }

    /// Lifecycle-derived index state; never changes the actor's raw clock.
    pub fn set_timer_active(&mut self, id: ActorId, active: bool) {
        let Some(actor) = self.entries.get(&id) else {
            return;
        };
        let deadline = next_timer(actor);
        if active {
            if self.suspended_timers.remove(&id) {
                if let Some(deadline) = deadline {
                    self.timer_deadlines.insert((deadline, id));
                }
            }
        } else if self.suspended_timers.insert(id) {
            if let Some(deadline) = deadline {
                self.timer_deadlines.remove(&(deadline, id));
            }
        }
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
        let deadline = (!self.suspended_timers.contains(&id))
            .then(|| next_timer(&actor))
            .flatten();
        let old = self.entries.insert(id, actor);
        if let Some(old) = &old {
            if let Some(interval) = next_timer(old) {
                self.timer_deadlines.remove(&(interval, id));
            }
            remove_member(&mut self.regions, old.location.region, id);
        }
        if let Some(interval) = deadline {
            self.timer_deadlines.insert((interval, id));
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
        self.suspended_timers.remove(id);
        if let Some(interval) = next_timer(&old) {
            self.timer_deadlines.remove(&(interval, *id));
        }
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
        let before = std::ptr::from_ref(&*self.entries);
        let entries = &mut *self.entries;
        if std::ptr::from_ref(entries) != before {
            crate::diagnostics::actor_table_copy(entries.len());
        }
        let actor = entries.get_mut(id)?;
        let timer_active = !self.suspended_timers.contains(id);
        let before_deadline = timer_active.then(|| next_timer(actor)).flatten();
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(*id);
        Some(ActorEdit {
            id: *id,
            before: actor.location.region,
            before_deadline,
            timer_active,
            actor,
            regions: &mut self.regions,
            timer_deadlines: &mut self.timer_deadlines,
        })
    }
}

pub(crate) struct ActorEdit<'a> {
    id: ActorId,
    before: RegionId,
    before_deadline: Option<u64>,
    timer_active: bool,
    actor: &'a mut Actor,
    regions: &'a mut Members,
    timer_deadlines: &'a mut Shared<BTreeSet<(u64, ActorId)>>,
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
        let deadline = self.timer_active.then(|| next_timer(self.actor)).flatten();
        if self.before_deadline != deadline {
            if let Some(interval) = self.before_deadline {
                self.timer_deadlines.remove(&(interval, self.id));
            }
            if let Some(interval) = deadline {
                self.timer_deadlines.insert((interval, self.id));
            }
        }
        if self.before != self.actor.location.region {
            remove_member(self.regions, self.before, self.id);
            add_member(self.regions, self.actor.location.region, self.id);
        }
    }
}

#[cfg(test)]
fn needs_time(actor: &Actor) -> bool {
    next_timer(actor).is_some()
}

fn next_timer(actor: &Actor) -> Option<u64> {
    actor
        .combat
        .as_ref()
        .map(|combat| combat.creature())
        .and_then(|creature| creature.next_change_in())
        .and_then(|interval| actor.active_since.checked_add(interval))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Game;
    use std::num::NonZeroU64;
    use tor_world::{Direction, Extent, Passage, Position, Region};

    fn timed_ids(actors: &ActorStore) -> Vec<ActorId> {
        let mut ids: Vec<_> = actors.scheduled_timers().map(|(_, id)| id).collect();
        ids.sort();
        ids
    }

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
        let mut game = Game::new(
            world,
            42,
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap(),
        );
        game.spawn_actor(at(1, 15), NonZeroU64::new(100).unwrap())
            .unwrap();
        game
    }

    fn assert_reference(game: &Game) {
        let mut deadlines: Vec<_> = game
            .actors
            .iter()
            .filter_map(|(&id, actor)| {
                let interval = actor.combat.as_ref()?.creature().next_change_in()?;
                Some((actor.active_since.checked_add(interval)?, id))
            })
            .collect();
        deadlines.sort();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            deadlines
        );
        let timed: Vec<_> = game
            .actors
            .iter()
            .filter_map(|(&id, actor)| needs_time(actor).then_some(id))
            .collect();
        assert_eq!(timed_ids(&game.actors), timed);
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
    fn timed_index_tracks_edits_death_clones_removal_insertion_and_reconstruction() {
        use crate::attributes::ManaBinding;
        use crate::content_ids::{ClassId, TemplateId};
        use crate::creatures::{CreatureBuild, Template};
        use crate::progression::{HdLedger, HdSource};
        use crate::rules::{Modifier, ModifierOp, Scope, Stat};
        let mut game = game();
        let source = game
            .spawn_actor(at(2, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let species = crate::test_creatures::unmodified_species("index_subject");
        let mut template = Template::new(TemplateId::new("capacity").unwrap(), 0);
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::ResourceMax {
                    resource: crate::resources::Resource::Stamina,
                },
                value: 2,
            },
        });
        let id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::test_support::test_ruleset(),
            crate::creatures::ScenarioCatalog {
                species: BTreeMap::from([(id.clone(), species)]),
                templates: BTreeMap::from([(template.id.clone(), template)]),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(vec![HdSource::Class(ClassId::new("mage").unwrap())], 7).unwrap(),
            ManaBinding::Intellect,
        )
        .unwrap();
        game.content = build.content().clone();
        game.configure_creature(
            source,
            crate::CreatureIdentity {
                name: "source".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
        game.configure_creature(
            ActorId(1),
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        assert_reference(&game);
        let retained = game.actors.clone();
        game.tick += 300;
        game.settle_creature_time().unwrap();
        assert!(
            std::ptr::eq(
                game.creature(source).unwrap().as_ref(),
                retained
                    .get(&source)
                    .unwrap()
                    .combat
                    .as_ref()
                    .unwrap()
                    .creature(),
            ),
            "updating one timer copied an unrelated creature state"
        );
        assert!(game.actors.scheduled_timers().next().is_none());
        assert_eq!(timed_ids(&retained), vec![ActorId(1)]);
        let mut reconstructed = ActorStore::from_entries((*retained.entries).clone());
        let actor = reconstructed.remove(&ActorId(1)).unwrap();
        assert!(reconstructed.scheduled_timers().next().is_none());
        reconstructed.insert(ActorId(1), actor);
        assert_eq!(timed_ids(&reconstructed), vec![ActorId(1)]);
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build
            .set_templates(vec![TemplateId::new("capacity").unwrap()])
            .unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        assert_eq!(timed_ids(&game.actors), vec![ActorId(1)]);
        game.tick += 200;
        game.settle_creature_time().unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            100,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 100, source), (game.tick + 300, ActorId(1))]
        );
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            300,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 300, ActorId(1)), (game.tick + 300, source)]
        );
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            400,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 300, ActorId(1)), (game.tick + 400, source)]
        );
        assert_reference(&game);
        game.tick += 400;
        game.settle_creature_time().unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(1), 100).unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        assert_reference(&game);
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
        let entries = (*game.actors.entries).clone();
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
