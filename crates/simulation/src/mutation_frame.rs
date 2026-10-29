//! Shared derived dependency evidence. Entity ancestry is certified; global
//! inputs are explicit invalidation facts, not a blanket whole-Game proof.
use crate::actor_store::ActorMutation;
use crate::item_store::ItemMutation;
use crate::mutation::{MutationCursor, MutationStamp};
use crate::{ActorId, Game, ItemId};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use tor_world::{SharedIdentity, World};

#[derive(Clone)]
struct EntityPublicationCursor {
    actors: MutationCursor<ActorId, ActorMutation>,
    items: MutationCursor<ItemId, ItemMutation>,
}

#[derive(Clone)]
pub(crate) struct EntityMutationIdentity {
    entities: EntityPublicationCursor,
    world: SharedIdentity<World>,
    content: Weak<crate::creatures::Content>,
    spatial_registry: SharedIdentity<crate::spatial_scope::registry::Registry>,
    tick: u64,
    residency: crate::streaming::ResidencyWitness,
    light: crate::portable_light::EmitterSignature,
}

impl std::fmt::Debug for EntityMutationIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EntityMutationIdentity(..)")
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MutationGlobals {
    pub world_changed: bool,
    pub content_changed: bool,
    pub spatial_registry_changed: bool,
    pub time_changed: bool,
    pub residency_changed: bool,
    pub light_changed: bool,
}

pub(crate) struct EntityMutationFrame {
    pub from: EntityMutationIdentity,
    pub to: EntityMutationIdentity,
    pub actors: BTreeMap<ActorId, ActorMutation>,
    pub items: BTreeMap<ItemId, ItemMutation>,
    pub globals: MutationGlobals,
}

impl Game {
    fn entity_publication_cursor(&self) -> EntityPublicationCursor {
        EntityPublicationCursor {
            actors: self.actors.mutation_cursor(),
            items: self.items.mutation_cursor(),
        }
    }

    fn certifies_entity_publications_since(&self, cursor: &EntityPublicationCursor) -> bool {
        self.actors.certifies_since(&cursor.actors) && self.items.certifies_since(&cursor.items)
    }

    pub(crate) fn mutation_identity(&self) -> EntityMutationIdentity {
        EntityMutationIdentity {
            entities: self.entity_publication_cursor(),
            world: self.world.weak_identity(),
            content: Arc::downgrade(&self.content),
            spatial_registry: self.spatial_registry.weak_identity(),
            tick: self.tick,
            residency: self.lifecycle.dependency_witness(),
            light: self.emitter_signature(),
        }
    }

    /// Certifies only actor/item publication ancestry. Every consumer must also
    /// check its exact global, clock, policy and external-availability inputs.
    pub(crate) fn certifies_mutations_since(&self, identity: &EntityMutationIdentity) -> bool {
        self.certifies_entity_publications_since(&identity.entities)
    }

    pub(crate) fn mutation_frame_since(
        &self,
        identity: &EntityMutationIdentity,
    ) -> Option<EntityMutationFrame> {
        crate::diagnostics::mutation_frame();
        let Some(actors) = self.actors.changes_since(&identity.entities.actors) else {
            crate::diagnostics::mutation_frame_unknown();
            return None;
        };
        let Some(items) = self.items.changes_since(&identity.entities.items) else {
            crate::diagnostics::mutation_frame_unknown();
            return None;
        };
        let to = self.mutation_identity();
        let globals = MutationGlobals {
            world_changed: !identity.world.matches(&self.world),
            content_changed: !identity
                .content
                .upgrade()
                .is_some_and(|content| Arc::ptr_eq(&content, &self.content)),
            spatial_registry_changed: !identity.spatial_registry.matches(&self.spatial_registry),
            time_changed: self.tick != identity.tick,
            residency_changed: !identity.residency.matches(&self.lifecycle),
            light_changed: to.light != identity.light,
        };
        let frame = EntityMutationFrame {
            from: identity.clone(),
            to,
            actors,
            items,
            globals,
        };
        debug_assert!(self.certifies_mutations_since(&frame.from));
        Some(frame)
    }

    pub(crate) fn actor_mutation_stamp(&self, actor: ActorId) -> Option<MutationStamp> {
        self.actors.mutation_stamp(&actor)
    }

    #[cfg(test)]
    pub(crate) fn item_mutation_stamp(&self, item: ItemId) -> Option<MutationStamp> {
        self.items.mutation_stamp(&item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_identity_does_not_retain_global_payloads_and_detects_cow_changes() {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let world_shared = game.world.is_shared();
        let registry_shared = game.spatial_registry.is_shared();
        let content_owners = Arc::strong_count(&game.content);
        let identity = game.mutation_identity();
        assert_eq!(game.world.is_shared(), world_shared);
        assert_eq!(game.spatial_registry.is_shared(), registry_shared);
        assert_eq!(Arc::strong_count(&game.content), content_owners);
        let unchanged = game.mutation_frame_since(&identity).unwrap();
        assert!(!unchanged.globals.world_changed);
        assert!(!unchanged.globals.content_changed);
        assert!(!unchanged.globals.spatial_registry_changed);
        assert!(!unchanged.globals.residency_changed);
        // Mutable access dissociates the weak allocation identity even when
        // the value remains equal: a proof must conservatively go cold.
        let _ = std::ops::DerefMut::deref_mut(&mut game.world);
        let changed = game.mutation_frame_since(&identity).unwrap();
        assert!(changed.globals.world_changed);
        assert!(!changed.globals.spatial_registry_changed);
        assert!(changed.actors.is_empty());
        assert!(changed.items.is_empty());
    }
}
