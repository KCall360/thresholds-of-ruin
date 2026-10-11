//! Natural and equipped melee through the same check/damage resolution bundle.
use crate::attributes::SkillCheck;
use crate::damage::{AttackCheck, DamageSpec, Protection};
use crate::dice::Edge;
use crate::grants::{Grant, Selector};
use crate::{ActorId, Game, GameError};

impl Game {
    pub(crate) fn creature_melee_damage(&self, actor: ActorId) -> Result<DamageSpec, GameError> {
        self.creature_melee_damage_with_impact_bonus(actor, 0)
    }

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
                self.creature(actor)
                    .map(|creature| &creature.derived().melee)
            })
    }

    pub(crate) fn creature_melee_damage_with_impact_bonus(
        &self,
        actor: ActorId,
        impact_bonus: u32,
    ) -> Result<DamageSpec, GameError> {
        let attack = self
            .selected_melee_attack(actor)
            .ok_or(GameError::InvalidLocation)?;
        self.creature(actor)
            .ok_or(GameError::InvalidLocation)?
            .derived()
            .damage_for_melee(attack, impact_bonus)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub(crate) fn resolve_creature_melee(
        &mut self,
        actor: ActorId,
        target: ActorId,
        defense: i32,
        impact_bonus: u32,
    ) -> (bool, u32) {
        let attack = self
            .selected_melee_attack(actor)
            .expect("validated selected melee source");
        let creature = self
            .creature(actor)
            .expect("validated creature melee source");
        let attributes = creature.derived().attributes;
        let skills = creature.derived().skills;
        let binding = creature.build().binding();
        let check = AttackCheck {
            check: SkillCheck {
                skill: attack.skill(),
                binding,
                modifier: attack.bonus(),
                threshold: defense,
            },
            attributes,
            skills,
        };
        let edge = Edge::from_counts(0, creature.fear().disadvantages_against(target));
        let damage = creature
            .derived()
            .damage_for_melee(attack, impact_bonus)
            .expect("melee validated before execution");
        let protection = self.damage_protection(target);
        let outcome = self.combat.resolve_creature_attack(
            check,
            edge,
            &damage,
            &protection,
            self.combat_diagnostics
                .as_mut()
                .and_then(|diagnostics| diagnostics.active.as_mut()),
        );
        let applied = outcome
            .damage
            .map_or(0, |damage| self.apply_injury(target, damage.total));
        (outcome.check.success, applied)
    }

    pub(crate) fn damage_protection(&self, target: ActorId) -> Protection {
        let combat = self.effective_combat(target).expect("live melee target");
        let descriptors = [
            crate::grants::Descriptor::Fire,
            crate::grants::Descriptor::Cold,
            crate::grants::Descriptor::Fear,
            crate::grants::Descriptor::MindAffecting,
        ];
        let descriptor_grants = self.creature(target).into_iter().flat_map(|creature| {
            descriptors.into_iter().flat_map(move |descriptor| {
                let selector = Selector::Descriptor(descriptor);
                let protection = &creature.derived().protection;
                protection
                    .has_immunity(selector)
                    .then_some(Grant::Immunity(selector))
                    .into_iter()
                    .chain(
                        (protection.reduction(selector) > 0)
                            .then_some(Grant::Reduction(selector, protection.reduction(selector))),
                    )
            })
        });
        Protection::from_grants(
            combat
                .immunities
                .iter()
                .map(|&category| Grant::Immunity(Selector::Category(category)))
                .chain(combat.reductions.iter().map(|(&category, &amount)| {
                    Grant::Reduction(Selector::Category(category), amount.min(1_000_000))
                }))
                .chain(descriptor_grants),
        )
        .expect("bounded category protection")
    }
}
