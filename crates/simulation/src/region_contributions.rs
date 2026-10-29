//! Derived region edges owned by certified entity mutation history.
//! Policy-dependent points/links and fixed-point ordering remain in streaming.
use crate::{ActorId, EntityMutationIdentity, Game, ItemId, ItemLocation};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use tor_world::{RegionId, Shared};

// First-red gate. Enable only after the unchanged-reconstruction test fails.
pub(crate) const ENABLED: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Actor(ActorId),
    Item(ItemId),
}
#[derive(Clone, Default)]
struct Row {
    source: Option<RegionId>,
    has_reach: bool,
    loaded: BTreeSet<RegionId>,
    active: BTreeSet<RegionId>,
    active_loaded: BTreeSet<RegionId>,
    actors: BTreeSet<ActorId>,
    items: BTreeSet<ItemId>,
}
#[derive(Clone, Default)]
struct Edges {
    // Source ownership is independent of whether its current edge set is empty.
    contributors: usize,
    actors: usize,
    reaches: usize,
    loaded: BTreeMap<RegionId, usize>,
    active: BTreeMap<RegionId, usize>,
    active_loaded: BTreeMap<RegionId, usize>,
}
#[derive(Clone, Default)]
pub(crate) struct RegionContributions {
    rows: Shared<BTreeMap<Owner, Shared<Row>>>,
    buckets: Shared<BTreeMap<RegionId, Shared<Edges>>>,
    actor_users: Shared<BTreeMap<ActorId, Shared<BTreeSet<Owner>>>>,
    item_users: Shared<BTreeMap<ItemId, Shared<BTreeSet<Owner>>>>,
}
#[derive(Clone)]
struct Snapshot {
    identity: EntityMutationIdentity,
    index: RegionContributions,
}
#[derive(Default)]
pub(crate) struct ContributionCache(Mutex<Option<Snapshot>>);
impl Clone for ContributionCache {
    fn clone(&self) -> Self {
        Self(Mutex::new(
            self.0.lock().expect("region contributions").clone(),
        ))
    }
}
impl std::fmt::Debug for ContributionCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContributionCache")
    }
}
impl PartialEq for ContributionCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ContributionCache {}

fn change_counts(counts: &mut BTreeMap<RegionId, usize>, members: &BTreeSet<RegionId>, add: bool) {
    for &region in members {
        if add {
            *counts.entry(region).or_default() += 1;
        } else {
            let count = counts.get_mut(&region).expect("owned region edge");
            *count -= 1;
            if *count == 0 {
                counts.remove(&region);
            }
        }
    }
}
fn remove_user<K: Ord + Copy>(
    map: &mut BTreeMap<K, Shared<BTreeSet<Owner>>>,
    key: K,
    owner: Owner,
) {
    let users = map.get_mut(&key).expect("owned reverse dependency");
    users.remove(&owner);
    if users.is_empty() {
        map.remove(&key);
    }
}
impl RegionContributions {
    fn remove(&mut self, owner: Owner) {
        let Some(row) = self.rows.remove(&owner) else {
            return;
        };
        for &id in &row.actors {
            remove_user(&mut self.actor_users, id, owner);
        }
        for &id in &row.items {
            remove_user(&mut self.item_users, id, owner);
        }
        if let Some(source) = row.source {
            let edges = self.buckets.get_mut(&source).expect("owned source region");
            edges.contributors -= 1;
            if matches!(owner, Owner::Actor(_)) {
                edges.actors -= 1;
                edges.reaches -= usize::from(row.has_reach);
            }
            change_counts(&mut edges.loaded, &row.loaded, false);
            change_counts(&mut edges.active, &row.active, false);
            change_counts(&mut edges.active_loaded, &row.active_loaded, false);
            if edges.contributors == 0 {
                self.buckets.remove(&source);
            }
        }
    }
    fn insert(&mut self, owner: Owner, row: Row) {
        for &id in &row.actors {
            self.actor_users.entry(id).or_default().insert(owner);
        }
        for &id in &row.items {
            self.item_users.entry(id).or_default().insert(owner);
        }
        if let Some(source) = row.source {
            let edges = self.buckets.entry(source).or_default();
            edges.contributors += 1;
            if matches!(owner, Owner::Actor(_)) {
                edges.actors += 1;
                edges.reaches += usize::from(row.has_reach);
            }
            change_counts(&mut edges.loaded, &row.loaded, true);
            change_counts(&mut edges.active, &row.active, true);
            change_counts(&mut edges.active_loaded, &row.active_loaded, true);
        }
        self.rows.insert(owner, Shared::new(row));
    }
    fn refresh(&mut self, game: &Game, owner: Owner) {
        self.remove(owner);
        let row = match owner {
            Owner::Actor(id) => game.actor_region_contribution(id),
            Owner::Item(id) => game.item_region_contribution(id),
        };
        if let Some(row) = row {
            self.insert(owner, row);
        }
    }
    pub(crate) fn coverage(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
    ) -> (usize, usize) {
        loaded
            .union(active)
            .filter_map(|region| self.buckets.get(region).map(|edges| (region, edges)))
            .fold((0, 0), |(actors, reaches), (region, edges)| {
                (
                    actors + edges.actors,
                    reaches
                        + if active.contains(region) {
                            edges.reaches
                        } else {
                            0
                        },
                )
            })
    }
    pub(crate) fn requirements(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
    ) -> crate::RegionTransition {
        let mut need = crate::RegionTransition::default();
        for source in loaded.union(active) {
            let Some(edges) = self.buckets.get(source) else {
                continue;
            };
            crate::diagnostics::region_contribution_bucket_visit();
            need.loaded.extend(edges.loaded.keys().copied());
            if active.contains(source) {
                need.active.extend(edges.active.keys().copied());
                need.loaded.extend(edges.active_loaded.keys().copied());
            }
        }
        need
    }
}
impl Game {
    fn actor_region_contribution(&self, id: ActorId) -> Option<Row> {
        let actor = self.actors.get(&id)?;
        crate::diagnostics::region_contribution_actor_rebuild();
        let mut row = Row {
            source: Some(actor.location.region),
            ..Row::default()
        };
        row.actors.insert(id);
        row.loaded = self.live_regions(id);
        row.active = row.loaded.clone();
        if actor.alive() {
            row.has_reach = true;
            let (reach, view) = self.streaming_actor_regions(id);
            row.active.extend(reach);
            row.active_loaded.extend(view);
        }
        if let Some(fall) = actor.motion.active_fall {
            row.actors.insert(fall.owner);
            row.active_loaded
                .extend(self.known_actor_region(fall.owner));
        }
        if let Some(pending) = &actor.pending {
            row.actors.extend(pending.plan.work().target());
            for reference in pending.item_payment.refs() {
                row.items.insert(reference.item);
                if let Some(ItemLocation::Carried(carrier)) =
                    self.items.get(&reference.item).map(|item| item.location)
                {
                    row.actors.insert(carrier);
                }
            }
        }
        if let Some(combat) = &actor.combat {
            row.actors.extend(
                combat
                    .creature()
                    .conditions()
                    .instances()
                    .filter_map(|entry| entry.reserved_by),
            );
        }
        Some(row)
    }
    fn item_region_contribution(&self, id: ItemId) -> Option<Row> {
        let item = self.items.get(&id).filter(|item| !item.holds.is_empty())?;
        crate::diagnostics::region_contribution_item_rebuild();
        let mut row = Row::default();
        row.items.insert(id);
        row.source = match item.location {
            ItemLocation::Ground(at) => Some(at.region),
            ItemLocation::Carried(carrier) => {
                row.actors.insert(carrier);
                self.known_actor_region(carrier)
            }
        };
        for hold in item.holds.iter() {
            let owner = hold.key.lineage.actor;
            row.actors.insert(owner);
            row.loaded.extend(self.known_actor_region(owner));
        }
        row.active = row.loaded.clone();
        Some(row)
    }
    pub(crate) fn region_contributions(&self) -> RegionContributions {
        // No lock is held while canonical scene/body helpers compute contributions.
        let prior = self
            .lifecycle
            .region_contributions
            .0
            .lock()
            .expect("region contributions")
            .clone();
        let frame = prior
            .as_ref()
            .and_then(|prior| self.mutation_frame_since(&prior.identity));
        let reusable = frame.as_ref().is_some_and(|frame| {
            let g = &frame.globals;
            !g.world_changed
                && !g.content_changed
                && !g.spatial_registry_changed
                && !g.time_changed
                && !g.residency_changed
                && !g.light_changed
        });
        let mut index = if reusable {
            prior.as_ref().expect("certified predecessor").index.clone()
        } else {
            RegionContributions::default()
        };
        let mut dirty = BTreeSet::new();
        if reusable {
            let frame = frame.as_ref().expect("certified mutation frame");
            for &id in frame.actors.keys() {
                dirty.insert(Owner::Actor(id));
                if let Some(users) = index.actor_users.get(&id) {
                    dirty.extend(users.iter().copied());
                }
            }
            for (&id, change) in &frame.items {
                dirty.insert(Owner::Item(id));
                // A transfer changes both carrier buckets even when neither
                // actor payload was edited. Refresh their reverse dependents
                // as well as direct users of the transferred item.
                if change.before != change.after {
                    for location in [change.before, change.after].into_iter().flatten() {
                        if let ItemLocation::Carried(carrier) = location {
                            dirty.insert(Owner::Actor(carrier));
                            if let Some(users) = index.actor_users.get(&carrier) {
                                dirty.extend(users.iter().copied());
                            }
                        }
                    }
                }
                if let Some(users) = index.item_users.get(&id) {
                    dirty.extend(users.iter().copied());
                }
            }
        } else {
            crate::diagnostics::region_contribution_cold_refresh();
            dirty.extend(self.actors.keys().copied().map(Owner::Actor));
            dirty.extend(self.items.claimed().map(|(id, _)| Owner::Item(id)));
        }
        for owner in dirty {
            index.refresh(self, owner);
        }
        *self
            .lifecycle
            .region_contributions
            .0
            .lock()
            .expect("region contributions") = Some(Snapshot {
            identity: frame
                .map(|frame| frame.to)
                .unwrap_or_else(|| self.mutation_identity()),
            index: index.clone(),
        });
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position};
    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn fixture() -> (Game, ActorId, ActorId) {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let owner = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let target = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        for actor in [owner, target] {
            crate::test_creatures::configure(
                &mut game,
                actor,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        (game, owner, target)
    }
    fn frontier() -> crate::RegionTransition {
        crate::RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1)]),
        }
    }
    #[test]
    fn contribution_query_matches_cold_before_unchanged_reuse_assertion() {
        let (game, _, _) = fixture();
        let wanted = frontier();
        let cold = game.settle_region_transition_cold(&wanted);
        let before = crate::diagnostics::work_counts();
        let actual = game.settle_region_transition(&wanted);
        assert_eq!(actual, cold);
        assert!(actual.active.contains(&RegionId(1)));
        let warm = crate::diagnostics::work_counts();
        assert_eq!(game.settle_region_transition(&wanted), cold);
        let after = crate::diagnostics::work_counts();
        assert!(
            warm.region_contribution_bucket_visits > before.region_contribution_bucket_visits,
            "first-red: ordinary settlement must actually use the contribution index"
        );
        assert_eq!(
            after.region_contribution_actor_rebuilds,
            warm.region_contribution_actor_rebuilds
        );
        assert_eq!(
            after.region_contribution_item_rebuilds,
            warm.region_contribution_item_rebuilds
        );
        assert!(after.region_contribution_bucket_visits > warm.region_contribution_bucket_visits);
    }
    #[test]
    fn contribution_target_only_mutation_and_candidate_rollback_match_cold() {
        let (mut game, owner, target) = fixture();
        game.admit_intention(
            owner,
            crate::Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: crate::supplies::SupplySelection::Carried,
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(
            game.preparation(owner).is_some(),
            "positive live target dependency"
        );
        let original = game.region_contributions();
        assert!(original.rows[&Owner::Actor(owner)].actors.contains(&target));
        let before_owner = game.actor_mutation_stamp(owner);
        let mut candidate = game.clone();
        candidate.actors.get_mut(&target).unwrap().location = at(3, 2);
        assert_eq!(candidate.actor_mutation_stamp(owner), before_owner);
        let changed = candidate.region_contributions();
        assert!(
            changed.rows[&Owner::Actor(owner)]
                .loaded
                .contains(&RegionId(3)),
            "unchanged owner must follow its changed target"
        );
        let wanted = frontier();
        // The direct indexed contribution union agrees with the unmodified live helper.
        assert_eq!(
            changed.rows[&Owner::Actor(owner)].loaded,
            candidate.live_regions(owner)
        );
        assert_eq!(
            candidate.settle_region_transition_indexed(&wanted),
            candidate.settle_region_transition_cold(&wanted)
        );
        drop(candidate);
        let retained = game.region_contributions();
        assert_eq!(
            retained.rows[&Owner::Actor(owner)].loaded,
            original.rows[&Owner::Actor(owner)].loaded
        );
        assert!(!retained.rows[&Owner::Actor(owner)]
            .loaded
            .contains(&RegionId(3)));
    }
    #[test]
    fn contribution_claimed_item_follows_carrier_without_item_mutation() {
        let (mut game, owner, _) = fixture();
        let mut spec = crate::ItemSpec::ordinary("bandage".into());
        spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let item = game
            .place_item_stack(10, at(1, 1), Some(owner), 1, spec)
            .unwrap();
        game.admit_intention(
            owner,
            crate::Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(owner),
                supplies: crate::supplies::SupplySelection::Explicit {
                    items: vec![crate::supplies::SelectedSupply {
                        cost_ordinal: 0,
                        item,
                    }],
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(
            !game.items[&item].holds.is_empty(),
            "positive live item claim"
        );
        let original = game.region_contributions();
        assert_eq!(original.rows[&Owner::Item(item)].source, Some(RegionId(1)));
        let stamp = game.item_mutation_stamp(item);
        let mut candidate = game.clone();
        candidate.actors.get_mut(&owner).unwrap().location = at(3, 2);
        assert_eq!(candidate.item_mutation_stamp(item), stamp);
        let changed = candidate.region_contributions();
        assert_eq!(changed.rows[&Owner::Item(item)].source, Some(RegionId(3)));
        assert_eq!(
            changed.rows[&Owner::Item(item)].loaded,
            BTreeSet::from([RegionId(3)])
        );
        let wanted = frontier();
        assert_eq!(
            candidate.settle_region_transition_indexed(&wanted),
            candidate.settle_region_transition_cold(&wanted)
        );
        assert_eq!(
            game.region_contributions().rows[&Owner::Item(item)].source,
            Some(RegionId(1))
        );
    }

    #[test]
    fn edge_empty_contributors_keep_shared_source_bucket_until_last_removal() {
        // Internal index invariant, not a claim that a published valid Game
        // currently produces unresolved item-claim owners.
        let mut index = RegionContributions::default();
        let source = RegionId(1);
        let row = Row {
            source: Some(source),
            ..Row::default()
        };
        let first = Owner::Item(ItemId(10));
        let second = Owner::Item(ItemId(11));
        index.insert(first, row.clone());
        index.insert(second, row);
        assert_eq!(index.rows.len(), 2);
        assert_eq!(index.buckets.len(), 1);
        let loaded = BTreeSet::from([source]);
        let requirements = index.requirements(&loaded, &loaded);
        assert!(
            requirements.active.is_empty() && requirements.loaded.is_empty(),
            "positive fixture: source owners exist without destination edges"
        );
        index.remove(first);
        assert!(index.rows.contains_key(&second));
        assert!(
            index.buckets.contains_key(&source),
            "a remaining edge-empty contributor still owns its source bucket"
        );
        index.remove(second);
        assert!(index.rows.is_empty());
        assert!(index.buckets.is_empty());
    }

    #[test]
    fn contribution_reference_counts_preserve_other_owners_and_load_only_edges() {
        let mut index = RegionContributions::default();
        let mut first = Row {
            source: Some(RegionId(1)),
            ..Row::default()
        };
        first.loaded.insert(RegionId(2));
        first.active.insert(RegionId(2));
        first.active_loaded.insert(RegionId(3));
        index.insert(Owner::Actor(ActorId(1)), first.clone());
        index.insert(Owner::Actor(ActorId(2)), first);
        index.remove(Owner::Actor(ActorId(1)));
        let loaded = BTreeSet::from([RegionId(1)]);
        let frozen = index.requirements(&BTreeSet::new(), &loaded);
        assert_eq!(frozen.loaded, BTreeSet::from([RegionId(2)]));
        assert!(frozen.active.is_empty());
        let active = index.requirements(&loaded, &loaded);
        assert_eq!(active.active, BTreeSet::from([RegionId(2)]));
        assert!(active.loaded.contains(&RegionId(3)));
        assert!(
            !active.active.contains(&RegionId(3)),
            "fall/view edges are load-only"
        );
    }
}
