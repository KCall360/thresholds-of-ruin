//! One bounded contact query shared by melee reach and strike-local hit evidence.
use crate::{ActorId, Game};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{compose_rotation, inverse_rotation, rotate_vector, Location, SightCell};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BodyCell {
    offset: [i32; 3],
    location: Location,
    frame: u8,
}

/// At most 64 cells, canonicalized by authored body-local offset rather than the
/// declaration's iteration order. This does not copy the actor or definition.
#[derive(Debug, PartialEq, Eq)]
struct ContactBody {
    actor: ActorId,
    location: Location,
    frame: u8,
    cells: Vec<BodyCell>,
}
impl ContactBody {
    fn capture(game: &Game, id: ActorId) -> Option<Self> {
        Self::capture_at(game, id, None)
    }
    fn capture_at(
        game: &Game,
        id: ActorId,
        placement: Option<crate::abilities::Placement>,
    ) -> Option<Self> {
        let actor = game.actors.get(&id)?;
        let (location, orientation) = placement.map_or((actor.location, actor.orientation), |at| {
            (at.location, at.orientation)
        });
        if actor.combat.as_ref().is_none_or(|combat| combat.hp() == 0) {
            return None;
        }
        let resolved = game.body_cells(location, orientation, &actor.body)?;
        let mut cells: Vec<_> = actor
            .body
            .cells
            .iter()
            .copied()
            .zip(resolved)
            .map(|(offset, (location, frame))| BodyCell {
                offset,
                location,
                frame,
            })
            .collect();
        cells.sort_unstable_by_key(|cell| cell.offset);
        Some(Self {
            actor: id,
            location,
            frame: orientation,
            cells,
        })
    }
    fn still_matches(&self, game: &Game) -> bool {
        !game.actor_frozen(self.actor) && Self::capture(game, self.actor).as_ref() == Some(self)
    }
}

/// Relative affine embedding of source persistent body coordinates into target
/// persistent body coordinates. A different offset is ambiguous even if the
/// raw portal rotation or the candidate's away vector happens to agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RelativeEmbedding {
    rotation: u8,
    offset: [i64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ContactCandidate {
    source: BodyCell,
    target: BodyCell,
    delta: [i64; 3],
    route: u8,
}
impl ContactCandidate {
    fn embedding(self) -> RelativeEmbedding {
        // compose_rotation applies its first operand, then its second.
        let rotation = compose_rotation(
            compose_rotation(self.source.frame, self.route),
            inverse_rotation(self.target.frame),
        );
        let u = rotate_vector(inverse_rotation(self.source.frame), self.delta);
        let endpoint = std::array::from_fn(|axis| i64::from(self.source.offset[axis]) + u[axis]);
        let endpoint = rotate_vector(rotation, endpoint);
        RelativeEmbedding {
            rotation,
            offset: std::array::from_fn(|axis| {
                i64::from(self.target.offset[axis]) - endpoint[axis]
            }),
        }
    }
    fn key(self) -> (i64, i64, [i32; 3], [i32; 3]) {
        let norm = |offset: [i32; 3]| offset.into_iter().map(|v| i64::from(v).pow(2)).sum();
        (
            norm(self.source.offset),
            norm(self.target.offset),
            self.source.offset,
            self.target.offset,
        )
    }
    fn away(self) -> [i64; 3] {
        rotate_vector(
            inverse_rotation(self.target.frame),
            rotate_vector(self.route, self.delta),
        )
    }
}

/// Transient evidence belonging to exactly one primary and its ordered riders.
/// It is not a preparation operand, wire DTO, world epoch, or serialized cache.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HitContact {
    source: ContactBody,
    target: ContactBody,
    contact: ContactCandidate,
}
impl HitContact {
    /// Revalidate only participants, body mappings and the selected physical
    /// route. Unrelated world edits and newly visible contacts cannot reselect
    /// a historical hit or veto an otherwise unchanged displacement.
    pub(crate) fn current_away(
        &self,
        game: &Game,
        source: ActorId,
        target: ActorId,
    ) -> Option<[i64; 3]> {
        if self.source.actor != source
            || self.target.actor != target
            || !self.source.still_matches(game)
            || !self.target.still_matches(game)
            || game.melee_neighbor_frame(self.contact.source.location, self.contact.delta)
                != Some((self.contact.target.location, self.contact.route))
        {
            return None;
        }
        Some(self.contact.away())
    }
}

/// Ambiguous contact still means reachable: primary checks/damage are unchanged.
/// Only a selected strike witness allocates this box; reach-only queries do not.
/// The primary owns it once and its riders borrow the same witness.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MeleeContact {
    Unreachable,
    Ambiguous,
    Selected(Box<HitContact>),
}
impl MeleeContact {
    pub(crate) fn reachable(&self) -> bool {
        !matches!(self, Self::Unreachable)
    }
    pub(crate) fn current_away(
        &self,
        game: &Game,
        source: ActorId,
        target: ActorId,
    ) -> Option<[i64; 3]> {
        match self {
            Self::Selected(hit) => hit.current_away(game, source, target),
            _ => None,
        }
    }
}

struct ContactQuery<'a> {
    game: &'a Game,
    source: ContactBody,
    target: ContactBody,
    disclosed_targets: BTreeMap<Location, BodyCell>,
}
impl<'a> ContactQuery<'a> {
    fn capture(
        game: &'a Game,
        source: ActorId,
        target: ActorId,
        scene: &[SightCell],
    ) -> Option<Self> {
        Self::capture_at(game, source, target, scene, None)
    }
    fn capture_at(
        game: &'a Game,
        source: ActorId,
        target: ActorId,
        scene: &[SightCell],
        placement: Option<crate::abilities::Placement>,
    ) -> Option<Self> {
        if source == target {
            return None;
        }
        let source_body = ContactBody::capture_at(game, source, placement)?;
        let target_body = ContactBody::capture(game, target)?;
        // Preserve physical-location membership, including local awareness.
        // Different unfolded sight occurrences are not separate hit candidates.
        let owner = game.actors.get(&source)?;
        let awareness = placement.map_or_else(
            || game.local_awareness(source),
            |at| game.local_awareness_at(at.location, at.orientation, &owner.body),
        );
        let target_locations: BTreeSet<_> =
            target_body.cells.iter().map(|cell| cell.location).collect();
        let visible: BTreeSet<_> = scene
            .iter()
            .chain(&awareness)
            .map(|cell| cell.location)
            .filter(|location| target_locations.contains(location))
            .collect();
        let disclosed_targets = target_body
            .cells
            .iter()
            .copied()
            .filter(|cell| visible.contains(&cell.location))
            .map(|cell| (cell.location, cell))
            .collect();
        Some(Self {
            game,
            source: source_body,
            target: target_body,
            disclosed_targets,
        })
    }
    /// No body-pair x direction product: each route ends in one indexed target
    /// cell or no target. At most 64 * 26 routes, each bounded by 6 * 3 hops.
    fn candidates(&self, shell: i64) -> impl Iterator<Item = ContactCandidate> + '_ {
        self.source
            .cells
            .iter()
            .copied()
            .flat_map(move |source| deltas(shell).map(move |delta| (source, delta)))
            .filter_map(|(source, delta)| {
                let (location, route) = self.game.melee_neighbor_frame(source.location, delta)?;
                let target = *self.disclosed_targets.get(&location)?;
                Some(ContactCandidate {
                    source,
                    target,
                    delta,
                    route,
                })
            })
    }
    fn reachable(&self) -> bool {
        (1..=3).any(|shell| self.candidates(shell).next().is_some())
    }
    fn select(self) -> MeleeContact {
        // Complete the first nonempty shell. A longer diagonal contact cannot
        // immunize an ordinary tall body whose cardinal contacts agree.
        for shell in 1..=3 {
            let mut best: Option<ContactCandidate> = None;
            let mut embedding = None;
            let mut ambiguous = false;
            for candidate in self.candidates(shell) {
                let current = candidate.embedding();
                if embedding.is_some_and(|old| old != current) {
                    ambiguous = true;
                }
                embedding.get_or_insert(current);
                if best.is_none_or(|old| candidate.key() < old.key()) {
                    best = Some(candidate);
                }
            }
            if let Some(contact) = best {
                return if ambiguous {
                    MeleeContact::Ambiguous
                } else {
                    MeleeContact::Selected(Box::new(HitContact {
                        source: self.source,
                        target: self.target,
                        contact,
                    }))
                };
            }
        }
        MeleeContact::Unreachable
    }
}

/// Explicit cardinal, edge, corner shells. Direction iteration within a shell
/// has no semantic priority; selection compares the complete shell.
fn deltas(shell: i64) -> impl Iterator<Item = [i64; 3]> {
    (-1_i64..=1)
        .flat_map(move |x| (-1_i64..=1).flat_map(move |y| (-1_i64..=1).map(move |z| [x, y, z])))
        .filter(move |delta| delta.iter().map(|v| v * v).sum::<i64>() == shell)
}

impl Game {
    /// Observer-local physical touch endpoints, shared across concealed bodies.
    /// Keep the ordinary contact query's disclosure-membership gate; rendering
    /// geometry alone is not authority to reach through corners or portals.
    pub(crate) fn melee_contact_locations_in_scene(
        &self,
        source: ActorId,
        scene: &[SightCell],
        placement: Option<crate::abilities::Placement>,
    ) -> BTreeSet<Location> {
        crate::diagnostics::contact_neighborhood();
        let Some(body) = ContactBody::capture_at(self, source, placement) else {
            return BTreeSet::new();
        };
        let owner = &self.actors[&source];
        let awareness = placement.map_or_else(
            || self.local_awareness(source),
            |at| self.local_awareness_at(at.location, at.orientation, &owner.body),
        );
        let disclosed: BTreeSet<_> = scene
            .iter()
            .chain(&awareness)
            .map(|cell| cell.location)
            .collect();
        let mut reachable = BTreeSet::new();
        for source in body.cells {
            for delta in (1..=3).flat_map(deltas) {
                crate::diagnostics::contact_route_probe();
                if let Some((location, _)) = self.melee_neighbor_frame(source.location, delta) {
                    if disclosed.contains(&location) {
                        reachable.insert(location);
                    }
                }
            }
        }
        reachable
    }

    pub(crate) fn melee_reachable_in_scene(
        &self,
        source: ActorId,
        target: ActorId,
        scene: &[SightCell],
    ) -> bool {
        self.melee_reachable_at_placement(source, target, scene, None)
    }
    pub(crate) fn melee_reachable_at_placement(
        &self,
        source: ActorId,
        target: ActorId,
        scene: &[SightCell],
        placement: Option<crate::abilities::Placement>,
    ) -> bool {
        ContactQuery::capture_at(self, source, target, scene, placement)
            .is_some_and(|query| query.reachable())
    }
    pub(crate) fn melee_contact_in_scene(
        &self,
        source: ActorId,
        target: ActorId,
        scene: &[SightCell],
    ) -> MeleeContact {
        ContactQuery::capture(self, source, target, scene)
            .map_or(MeleeContact::Unreachable, ContactQuery::select)
    }
    /// Same terrain/door/component-order predicate used by ordinary reach and
    /// ground touch. Occupancy is checked later by physical body translation.
    pub(crate) fn melee_neighbor_frame(
        &self,
        from: Location,
        delta: [i64; 3],
    ) -> Option<(Location, u8)> {
        let mut destination = None;
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut at = from;
            let mut frame = 0;
            for axis in order {
                if delta[axis] == 0 {
                    continue;
                }
                let mut component = [0; 3];
                component[axis] = delta[axis];
                let direction = tor_world::Direction::from_delta(rotate_vector(frame, component))?;
                let (next, rotation) = self.world.physics_neighbor(at, direction)?;
                if !self.world.walkable(next) {
                    return None;
                }
                at = next;
                frame = compose_rotation(frame, rotation);
            }
            if destination.is_some_and(|old| old != (at, frame)) {
                return None;
            }
            destination = Some((at, frame));
        }
        destination
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodySpec, CreatureIdentity};
    use tor_world::{Extent, Position, Region, RegionId, World};

    fn at(x: i32, y: i32, z: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y, z },
        }
    }
    fn fixture() -> Game {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut game = Game::new(
            World::new(
                vec![Region {
                    id: RegionId(1),
                    name: "contact".into(),
                    bounds: Extent::new(12, 12, 12).unwrap(),
                }],
                vec![],
            )
            .unwrap(),
            42,
            build.content().clone(),
        );
        for location in [at(4, 4, 4), at(5, 4, 4)] {
            let actor = game
                .spawn_actor(location, std::num::NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_creature(
                actor,
                CreatureIdentity {
                    name: "subject".into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }
    fn contact(game: &Game) -> Box<HitContact> {
        let scene = game.scene(ActorId(1)).unwrap();
        match game.melee_contact_in_scene(ActorId(1), ActorId(2), &scene) {
            MeleeContact::Selected(hit) => hit,
            other => panic!("expected unambiguous shortest contact, got {other:?}"),
        }
    }
    fn body(cells: Vec<[i32; 3]>) -> BodySpec {
        BodySpec {
            cells,
            eye: [0; 3],
            mass: 80,
        }
    }

    #[test]
    fn cardinal_and_edge_contacts_choose_reference_cells_of_tall_bodies() {
        for delta in [[1, 0, 0], [1, 1, 0]] {
            let mut game = fixture();
            game.teleport(ActorId(2), at(4 + delta[0], 4 + delta[1], 4))
                .unwrap();
            for actor in [ActorId(1), ActorId(2)] {
                game.set_body(actor, body(vec![[0, 0, 1], [0, 0, 0]]))
                    .unwrap();
            }
            let hit = contact(&game);
            assert_eq!(hit.contact.source.offset, [0; 3]);
            assert_eq!(hit.contact.target.offset, [0; 3]);
            assert_eq!(
                hit.current_away(&game, ActorId(1), ActorId(2)),
                Some(delta.map(i64::from))
            );
        }
    }

    #[test]
    fn concave_contact_tie_uses_body_local_key_independent_of_body_and_scene_order() {
        for reverse in [false, true] {
            let mut game = fixture();
            game.teleport(ActorId(2), at(5, 5, 4)).unwrap();
            let mut cells = vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]];
            if reverse {
                cells.reverse();
            }
            game.set_body(ActorId(1), body(cells)).unwrap();
            let mut scene = game.scene(ActorId(1)).unwrap();
            scene.extend(scene.clone());
            if reverse {
                scene.reverse();
            }
            let MeleeContact::Selected(hit) =
                game.melee_contact_in_scene(ActorId(1), ActorId(2), &scene)
            else {
                panic!("ordinary concavity is not portal ambiguity")
            };
            assert_eq!(hit.contact.source.offset, [0, 1, 0]);
            assert_eq!(hit.contact.target.offset, [0; 3]);
            assert_eq!(hit.contact.away(), [1, 0, 0]);
        }
    }

    #[test]
    fn frame_composition_preserves_complete_contact_vector_for_all_cube_frames() {
        for source_frame in 0..24 {
            for target_frame in 0..24 {
                for route in 0..24 {
                    for delta in [[1, 0, 0], [0, -1, 0], [0, 0, 1], [1, 1, 0], [1, -1, 1]] {
                        let candidate = ContactCandidate {
                            source: BodyCell {
                                offset: [1, -1, 0],
                                location: at(4, 4, 4),
                                frame: source_frame,
                            },
                            target: BodyCell {
                                offset: [0, 1, 2],
                                location: at(5, 4, 4),
                                frame: target_frame,
                            },
                            delta,
                            route,
                        };
                        let embedding = candidate.embedding();
                        let local_delta = rotate_vector(inverse_rotation(source_frame), delta);
                        assert_eq!(
                            candidate.away(),
                            rotate_vector(embedding.rotation, local_delta)
                        );
                        let endpoint = std::array::from_fn(|axis| {
                            i64::from(candidate.source.offset[axis]) + local_delta[axis]
                        });
                        let mapped = rotate_vector(embedding.rotation, endpoint);
                        assert_eq!(
                            std::array::from_fn::<_, 3, _>(
                                |axis| mapped[axis] + embedding.offset[axis]
                            ),
                            candidate.target.offset.map(i64::from)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn moved_rotated_or_reshaped_participant_invalidates_only_captured_contact() {
        for source in [false, true] {
            for change in 0..3 {
                let mut game = fixture();
                let hit = contact(&game);
                let actor = if source { ActorId(1) } else { ActorId(2) };
                match change {
                    0 => game.teleport(actor, at(4, 5, 4)).unwrap(),
                    1 => game.actors.get_mut(&actor).unwrap().orientation = 1,
                    _ => game
                        .set_body(actor, body(vec![[0, 0, 0], [0, 0, 1]]))
                        .unwrap(),
                }
                assert_eq!(hit.current_away(&game, ActorId(1), ActorId(2)), None);
                assert!(
                    game.attack_available(ActorId(1), ActorId(2)),
                    "new reach still exists"
                );
                assert!(
                    contact(&game)
                        .current_away(&game, ActorId(1), ActorId(2))
                        .is_some(),
                    "next primary captures live evidence"
                );
            }
        }
    }

    #[test]
    fn unrelated_world_change_and_equivalent_shape_order_do_not_stale_contact() {
        let mut game = fixture();
        game.set_body(ActorId(1), body(vec![[0, 0, 0], [0, 0, 1]]))
            .unwrap();
        let hit = contact(&game);
        game.set_wall(at(10, 10, 10), true).unwrap();
        game.set_body(
            ActorId(1),
            BodySpec {
                cells: vec![[0, 0, 1], [0, 0, 0]],
                eye: [0, 0, 1],
                mass: 1,
            },
        )
        .unwrap();
        assert_eq!(
            hit.current_away(&game, ActorId(1), ActorId(2)),
            Some([1, 0, 0])
        );
    }

    #[test]
    fn chosen_diagonal_route_becoming_blocked_does_not_reselect_another_contact() {
        let mut game = fixture();
        game.teleport(ActorId(2), at(5, 5, 4)).unwrap();
        let hit = contact(&game);
        assert_eq!(hit.contact.delta, [1, 1, 0]);
        game.set_wall(at(5, 4, 4), true).unwrap();
        assert_eq!(hit.current_away(&game, ActorId(1), ActorId(2)), None);
        assert!(!game.attack_available(ActorId(1), ActorId(2)));
    }

    #[test]
    fn live_contact_after_target_moves_north_is_north_and_old_contact_is_stale() {
        let mut game = fixture();
        let east = contact(&game);
        game.teleport(ActorId(2), at(4, 3, 4)).unwrap();
        assert_eq!(east.current_away(&game, ActorId(1), ActorId(2)), None);
        assert_eq!(
            contact(&game).current_away(&game, ActorId(1), ActorId(2)),
            Some([0, -1, 0])
        );
    }

    #[test]
    fn reach_and_selected_contact_share_the_same_bounded_candidate_predicate() {
        for target in [at(5, 4, 4), at(5, 5, 4), at(5, 5, 5), at(7, 4, 4)] {
            for wall in [false, true] {
                let mut game = fixture();
                game.teleport(ActorId(2), target).unwrap();
                game.set_wall(at(4, 5, 4), wall).unwrap();
                let scene = game.scene(ActorId(1)).unwrap();
                assert_eq!(
                    game.attack_available(ActorId(1), ActorId(2)),
                    game.melee_reachable_in_scene(ActorId(1), ActorId(2), &scene)
                );
                assert_eq!(
                    game.melee_reachable_in_scene(ActorId(1), ActorId(2), &scene),
                    game.melee_contact_in_scene(ActorId(1), ActorId(2), &scene)
                        .reachable()
                );
            }
        }
    }

    #[test]
    fn retained_contact_geometry_keeps_life_combat_and_rejected_world_fresh() {
        let game = fixture();
        let source = ActorId(1);
        let target = ActorId(2);
        assert_attack_matches_fresh(&game, source, target, true);
        for changed in [source, target] {
            let mut candidate = game.clone();
            let combat = candidate.actors.get_mut(&changed).unwrap().combat.take();
            assert_attack_matches_fresh(&candidate, source, target, false);
            candidate.actors.get_mut(&changed).unwrap().combat = combat;
            assert_attack_matches_fresh(&candidate, source, target, true);
            crate::test_creatures::injure(&mut candidate, changed, u32::MAX).unwrap();
            assert_attack_matches_fresh(&candidate, source, target, false);
            assert_attack_matches_fresh(&game, source, target, true);
        }
        let mut rejected = game.clone();
        rejected.teleport(target, at(5, 5, 4)).unwrap();
        assert_attack_matches_fresh(&rejected, source, target, true);
        rejected.set_wall(at(5, 4, 4), true).unwrap();
        assert_attack_matches_fresh(&rejected, source, target, false);
        assert_attack_matches_fresh(&game, source, target, true);
    }

    #[test]
    fn warm_contact_endpoints_reuse_source_geometry_but_resolve_live_target() {
        let game = fixture();
        let source = ActorId(1);
        let target = ActorId(2);
        assert!(
            game.attack_available(source, target),
            "adjacent living bodies"
        );
        let before = crate::diagnostics::work_counts();
        for _ in 0..4 {
            assert!(game.attack_available(source, target));
        }
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.body_cells - before.body_cells,
            4,
            "only the current target body needs resolving after source geometry is witnessed"
        );
        assert_eq!(after.contact_route_probes - before.contact_route_probes, 0);
    }

    #[test]
    fn warm_attack_available_reuses_scene_and_revalidates_contact() {
        let game = fixture();
        let actor = ActorId(1);
        let target = ActorId(2);
        let scene = game.scene(actor).unwrap();
        assert!(game.attack_available_in_scene(actor, target, &scene));
        let unchanged = game.clone();
        let before = crate::diagnostics::work_counts();
        let mut body_cells = before.body_cells;
        for _ in 0..4 {
            assert!(game.attack_available(actor, target));
            let now = crate::diagnostics::work_counts();
            assert!(
                now.body_cells > body_cells,
                "each query must revalidate live bodies"
            );
            body_cells = now.body_cells;
        }
        let after = crate::diagnostics::work_counts();
        assert_eq!(game, unchanged);
        assert_eq!(
            after.ordinary_scenes - before.ordinary_scenes,
            0,
            "warm preparation reach checks must not force fresh ordinary scenes"
        );
        assert_eq!(
            after.scene_cells_copied - before.scene_cells_copied,
            0,
            "shared geometry must not be cloned into reach-only queries"
        );
    }

    // Ask the optimized entry point first. Building the forced-fresh oracle
    // earlier would warm the shared cache and mask a stale-witness bug.
    fn assert_attack_matches_fresh(game: &Game, source: ActorId, target: ActorId, expected: bool) {
        let unchanged = game.clone();
        let actual = game.attack_available(source, target);
        let scene = game.scene(source).unwrap();
        let fresh = game.attack_available_in_scene(source, target, &scene);
        assert_eq!(fresh, expected, "independent contact fixture");
        assert_eq!(actual, fresh, "cached geometry cannot change live reach");
        assert_eq!(*game, unchanged);
    }

    #[test]
    fn cached_attack_scene_revalidates_location_body_rotation_geometry_and_candidate_identity() {
        let source = ActorId(1);
        let target = ActorId(2);
        let mut game = fixture();
        assert_attack_matches_fresh(&game, source, target, true);
        game.teleport(target, at(7, 4, 4)).unwrap();
        assert_attack_matches_fresh(&game, source, target, false);
        game.set_body(source, body(vec![[0, 0, 0], [1, 0, 0], [2, 0, 0]]))
            .unwrap();
        assert_attack_matches_fresh(&game, source, target, true);
        let quarter_turn = (0..24)
            .find(|&rotation| tor_world::rotate_vector(rotation, [1, 0, 0]) == [0, 1, 0])
            .unwrap();
        game.actors.get_mut(&source).unwrap().orientation = quarter_turn;
        assert_attack_matches_fresh(&game, source, target, false);
        game.set_body(source, body(vec![[0, 0, 0]])).unwrap();
        game.teleport(source, at(6, 4, 4)).unwrap();
        game.teleport(target, at(7, 5, 4)).unwrap();
        assert_attack_matches_fresh(&game, source, target, true);
        game.set_wall(at(7, 4, 4), true).unwrap();
        assert_attack_matches_fresh(&game, source, target, false);
        game.set_wall(at(7, 4, 4), false).unwrap();
        assert_attack_matches_fresh(&game, source, target, true);

        // A discarded candidate shares the derived cache, never its authority.
        let mut discarded = game.clone();
        discarded.teleport(source, at(1, 1, 4)).unwrap();
        assert_attack_matches_fresh(&discarded, source, target, false);
        assert_attack_matches_fresh(&game, source, target, true);
        assert!(!game.attack_available(source, source));
        assert!(!game.attack_available(source, ActorId(u64::MAX)));
        assert!(!game.attack_available(ActorId(u64::MAX), target));
    }

    #[test]
    fn cached_attack_scene_preserves_hidden_contact_and_rotated_portal_boundaries() {
        use crate::perception::contract_fixtures as f;
        use crate::perception::{ActorDisclosure, PerceptionValue};
        let mut game = f::fixture(&[], &[PerceptionValue::Invisible { enabled: true }]);
        assert_attack_matches_fresh(&game, f::OWNER, f::SUBJECT, true);
        let contact = game.observe(f::OWNER).unwrap();
        let target = contact
            .visible_actors
            .iter()
            .find(|a| a.id == f::SUBJECT)
            .unwrap();
        assert_eq!(target.disclosure, ActorDisclosure::AnonymousContact);
        assert!(target.name.is_empty() && target.asset.is_none());

        game.teleport(f::OWNER, f::at(1, 8, 2)).unwrap();
        game.teleport(f::SUBJECT, f::at(2, 2, 0)).unwrap();
        assert_attack_matches_fresh(&game, f::OWNER, f::SUBJECT, false);
        assert!(game
            .observe(f::OWNER)
            .unwrap()
            .visible_actors
            .iter()
            .all(|a| a.id != f::SUBJECT));
        let quarter_turn = (0..24)
            .find(|&rotation| {
                tor_world::rotate_vector(rotation, [1, 0, 0]) == [0, 1, 0]
                    && tor_world::rotate_vector(rotation, [0, 1, 0]) == [-1, 0, 0]
            })
            .unwrap();
        game.connect(
            tor_world::Passage {
                from: f::at(1, 8, 2),
                direction: tor_world::Direction::East,
                to: f::at(2, 2, 0),
            },
            quarter_turn,
        )
        .unwrap();
        assert_attack_matches_fresh(&game, f::OWNER, f::SUBJECT, true);
        let scene = game.scene(f::OWNER).unwrap();
        assert!(scene.iter().any(|cell| {
            cell.physical && cell.location == f::at(2, 2, 0) && cell.rotation == quarter_turn
        }));
        f::apply(&mut game, f::OWNER, "fixture_blind", 1000);
        assert_attack_matches_fresh(&game, f::OWNER, f::SUBJECT, true);
        let contact = game.observe(f::OWNER).unwrap();
        let target = contact
            .visible_actors
            .iter()
            .find(|a| a.id == f::SUBJECT)
            .unwrap();
        assert_eq!(target.disclosure, ActorDisclosure::AnonymousContact);
        assert!(target.name.is_empty() && target.asset.is_none());
        game.teleport(f::SUBJECT, f::at(2, 2, 4)).unwrap();
        assert_attack_matches_fresh(&game, f::OWNER, f::SUBJECT, false);
    }
}
