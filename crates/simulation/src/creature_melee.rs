//! Selected melee reads the authoritative source cache; slot selection lives in derivation.
use crate::{ActorId, Game};
impl Game {
    /// Trusted source query; client disclosure still requires knowledge/authority.
    pub fn selected_melee_attack(&self, actor: ActorId) -> Option<&crate::attacks::MeleeAttack> {
        Some(
            &self
                .actors
                .get(&actor)?
                .combat
                .as_ref()?
                .creature()
                .derived()
                .selected_attack
                .attack,
        )
    }
}
