//! One deterministic selected-weapon rule, ordered by equipped slot.
use crate::{ActorId, Game};
impl Game {
    /// Trusted source query; client disclosure still requires knowledge/authority.
    pub fn selected_melee_attack(&self, actor: ActorId) -> Option<&crate::attacks::MeleeAttack> {
        let actor_state = self.actors.get(&actor)?;
        actor_state
            .equipment
            .values()
            .rev()
            .find_map(|item| {
                self.items
                    .get(item)?
                    .spec
                    .equipment
                    .as_ref()?
                    .attack
                    .as_ref()
            })
            .or_else(|| {
                self.actors
                    .get(&actor)
                    .and_then(|actor| actor.combat.as_ref())
                    .map(|combat| &combat.creature().build().species().melee)
            })
    }
}
