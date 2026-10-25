//! Unilateral environmental skill checks share ordinary skill and edge rules.
//! There is no opponent or ability, and capture is pure: only publication spends
//! the exact available outgoing next-check sources selected for this attempt.
use crate::abilities::{CheckThreshold, PlannedCheck, RuleError};
use crate::attributes::{Skill, SkillCheck};
use crate::conditions::TokenReservation;
use crate::creatures::CreatureState;
use crate::{ActorId, Game, GameError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapturedHazardCheck {
    pub(crate) check: PlannedCheck,
    pub(crate) tokens: Vec<TokenReservation>,
}

pub(crate) fn capture_hazard_skill_check(
    creature: &CreatureState,
    actor: ActorId,
    skill: Skill,
    dc: i32,
    excluded: &[TokenReservation],
) -> Result<CapturedHazardCheck, GameError> {
    if actor.0 == 0 || actor.0 == u64::MAX || !(0..=crate::limits::NUMERIC).contains(&i64::from(dc))
    {
        return Err(GameError::Rule(RuleError::InvalidDefinition));
    }
    let rules = creature.build().ruleset();
    let tokens: Vec<_> = creature
        .conditions()
        .qualifying_tokens(rules, actor, false)
        .into_iter()
        .filter(|token| !excluded.contains(token))
        .collect();
    let derived = creature.derived();
    let edge = derived
        .hazard_check_edge(rules, actor, &tokens)
        .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?
        + derived.perception.skill_edge(skill);
    let bonus = SkillCheck {
        skill,
        binding: derived.binding(),
        modifier: 0,
        threshold: 0,
    }
    .bonus(derived.attributes, derived.skills);
    Ok(CapturedHazardCheck {
        check: PlannedCheck {
            actor: crate::rules::CheckActor::User,
            target_participates: false,
            bonus,
            threshold: CheckThreshold::Fixed {
                value: i64::from(dc),
            },
            edge,
        },
        tokens,
    })
}

impl Game {
    /// The caller owns the surrounding candidate transaction. RNG is published
    /// before lifecycle hooks, so a nested contact cannot replay or lose draws.
    pub(crate) fn resolve_hazard_skill_check_in_transaction(
        &mut self,
        actor: ActorId,
        skill: Skill,
        dc: i32,
    ) -> Result<bool, GameError> {
        self.settle_actor_time(actor)?;
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        if creature.health().dead() {
            return Ok(false);
        }
        let captured = capture_hazard_skill_check(&creature, actor, skill, dc, &[])?;
        let mut rng = self.rules_rng;
        let success = crate::abilities::resolve_escape_check(&captured.check, &mut rng);
        self.rules_rng = rng;
        if !captured.tokens.is_empty() {
            let mut holder = self.actor_outcome(actor)?;
            let combat = holder
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?;
            combat.reserve_tokens(&captured.tokens, actor)?;
            combat.consume_tokens(&captured.tokens, actor);
            self.apply_outcome(holder)?;
        }
        Ok(success)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_ids::ConditionId;
    use crate::rules::{AbilityStat, Modifier, ModifierOp, Scope, Stat};
    fn condition(id: &str) -> ConditionId {
        ConditionId::new(id).unwrap()
    }
    fn creature() -> CreatureState {
        CreatureState::new(crate::test_creatures::build(
            crate::test_creatures::species(),
        ))
        .unwrap()
    }
    fn fixture() -> (Game, ActorId) {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut game = Game::two_room(42, build.content().clone());
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "fixture".into(),
            },
            build,
        )
        .unwrap();
        (game, actor)
    }
    #[test]
    fn unilateral_capture_uses_only_ordinary_outgoing_edge_and_live_skill() {
        let mut species = crate::test_creatures::species();
        for (stat, value) in [
            (Stat::Edge, 2),
            (Stat::CheckEdge, 3),
            (Stat::IncomingEdge { defense: None }, 20),
            (
                Stat::Ability {
                    ability: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    stat: AbilityStat::CheckEdge,
                },
                30,
            ),
        ] {
            species.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add { stat, value },
            });
        }
        let mut creature = CreatureState::new(crate::test_creatures::build(species)).unwrap();
        let actor = ActorId(7);
        // Self-caused source-scoped fear would leak through a fabricated self
        // opponent, so this is an adversarial witness, not a harmless other foe.
        creature
            .apply_condition(&condition("fear"), actor, 200)
            .unwrap();
        let before = creature.clone();
        let captured =
            capture_hazard_skill_check(&creature, actor, Skill::Athletics, 12, &[]).unwrap();
        assert_eq!(captured.check.edge, 5);
        assert_eq!(captured.check.actor, crate::rules::CheckActor::User);
        assert!(!captured.check.target_participates);
        assert_eq!(
            captured.check.threshold,
            CheckThreshold::Fixed { value: 12 }
        );
        assert!(captured.tokens.is_empty());
        let derived = creature.derived();
        assert_eq!(
            captured.check.bonus,
            SkillCheck {
                skill: Skill::Athletics,
                binding: derived.binding(),
                modifier: 0,
                threshold: 0
            }
            .bonus(derived.attributes, derived.skills)
        );
        assert_eq!(
            creature, before,
            "capture cannot reserve or consume sources"
        );
    }
    #[test]
    fn capture_excludes_exact_primary_tokens_and_unavailable_or_incoming_sources() {
        let mut creature = creature();
        let actor = ActorId(7);
        for id in ["readied", "distracted", "exposed"] {
            creature
                .apply_condition(&condition(id), actor, 200)
                .unwrap();
        }
        let available =
            capture_hazard_skill_check(&creature, actor, Skill::Agility, 9, &[]).unwrap();
        assert_eq!(available.tokens.len(), 2);
        assert_eq!(available.check.edge, 0);
        let readied = available
            .tokens
            .iter()
            .find(|token| token.condition == condition("readied"))
            .unwrap()
            .clone();
        let distracted = available
            .tokens
            .iter()
            .find(|token| token.condition == condition("distracted"))
            .unwrap()
            .clone();
        let before = creature.clone();
        let without_primary = capture_hazard_skill_check(
            &creature,
            actor,
            Skill::Agility,
            9,
            std::slice::from_ref(&readied),
        )
        .unwrap();
        assert_eq!(without_primary.tokens, std::slice::from_ref(&distracted));
        assert_eq!(without_primary.check.edge, -1);
        assert_eq!(creature, before);
        creature
            .reserve_tokens(std::slice::from_ref(&distracted), actor)
            .unwrap();
        let reserved = creature.clone();
        let captured =
            capture_hazard_skill_check(&creature, actor, Skill::Agility, 9, &[]).unwrap();
        assert_eq!(captured.tokens, [readied]);
        assert_eq!(captured.check.edge, 1);
        assert_eq!(
            creature, reserved,
            "held tokens stay held; capture is not release"
        );
    }
    #[test]
    fn distinct_token_families_keep_exact_sources_and_duplicate_token_family_is_invalid() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut document = original.ruleset().document().clone();
        let mut second = document
            .conditions
            .iter()
            .find(|row| row.id == condition("readied"))
            .unwrap()
            .clone();
        second.id = condition("fixture_second_ready");
        let mut invalid = document.clone();
        invalid.conditions.push(second.clone());
        second.family = crate::content_ids::ConditionFamilyId::new("fixture_second_ready").unwrap();
        document.conditions.push(second);
        let rules = crate::rules::Ruleset::validate(document).unwrap();
        assert!(matches!(crate::rules::Ruleset::validate(invalid),
            Err(crate::rules::RulesetError::Bound { field }) if field == "token_family"));
        let content =
            crate::creatures::Content::new(rules, original.content().catalog().clone()).unwrap();
        let build = crate::creatures::CreatureBuild::from_recorded(
            content,
            original.species_id().clone(),
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.advancement().clone(),
            original.template_ids().to_vec(),
        )
        .unwrap();
        let mut creature = CreatureState::new(build).unwrap();
        let actor = ActorId(7);
        for id in ["readied", "fixture_second_ready"] {
            creature
                .apply_condition(&condition(id), actor, 200)
                .unwrap();
        }
        let captured =
            capture_hazard_skill_check(&creature, actor, Skill::Athletics, 12, &[]).unwrap();
        assert_eq!(
            captured.check.edge, 2,
            "each valid distinct token family contributes its ordinary sign"
        );
        assert_eq!(
            captured.tokens.len(),
            2,
            "edge folding cannot collapse exact spendable sources"
        );
        let other = capture_hazard_skill_check(
            &creature,
            actor,
            Skill::Athletics,
            12,
            &captured.tokens[..1],
        )
        .unwrap();
        assert_eq!(other.check.edge, 1);
        assert_eq!(other.tokens, captured.tokens[1..]);
    }
    #[test]
    fn actual_check_spends_outgoing_token_on_both_success_and_failure_only_once() {
        for dc in [0, 100] {
            let (mut game, actor) = fixture();
            for id in ["readied", "exposed"] {
                game.apply_condition(actor, &condition(id), actor, 200)
                    .unwrap();
            }
            let captured = capture_hazard_skill_check(
                &game.creature(actor).unwrap(),
                actor,
                Skill::Agility,
                dc,
                &[],
            )
            .unwrap();
            let mut rng = game.rules_rng;
            let expected = crate::abilities::resolve_escape_check(&captured.check, &mut rng);
            let at = game.actors[&actor].location;
            assert_eq!(
                game.resolve_hazard_skill_check_in_transaction(actor, Skill::Agility, dc)
                    .unwrap(),
                expected
            );
            assert_eq!(expected, dc == 0);
            assert_eq!(game.rules_rng, rng);
            let creature = game.creature(actor).unwrap();
            assert!(creature
                .conditions()
                .instances()
                .all(|entry| entry.condition != condition("readied")));
            assert!(creature
                .conditions()
                .instances()
                .any(|entry| entry.condition == condition("exposed")));
            assert_eq!(game.actors[&actor].location, at);
            let next =
                capture_hazard_skill_check(&creature, actor, Skill::Agility, dc, &[]).unwrap();
            assert!(next.tokens.is_empty());
            assert_eq!(next.check.edge, 0);
        }
    }
    #[test]
    fn hazard_difficulty_bounds_reject_negative_without_touching_sources() {
        let mut creature = creature();
        let actor = ActorId(7);
        creature
            .apply_condition(&condition("readied"), actor, 200)
            .unwrap();
        let before = creature.clone();
        for dc in [0, crate::limits::NUMERIC as i32] {
            assert!(
                capture_hazard_skill_check(&creature, actor, Skill::Athletics, dc, &[]).is_ok()
            );
        }
        for dc in [-1, crate::limits::NUMERIC as i32 + 1] {
            assert_eq!(
                capture_hazard_skill_check(&creature, actor, Skill::Athletics, dc, &[]),
                Err(GameError::Rule(RuleError::InvalidDefinition))
            );
        }
        assert_eq!(creature, before);
    }
    #[test]
    fn hazard_check_settles_expired_sources_before_capture() {
        let (mut game, actor) = fixture();
        game.apply_condition(actor, &condition("readied"), actor, 1)
            .unwrap();
        let baseline = creature();
        let ordinary =
            capture_hazard_skill_check(&baseline, actor, Skill::Agility, 12, &[]).unwrap();
        game.tick = 1; // The owning scheduler has reached, but not materialized, this boundary.
        let mut expected_rng = game.rules_rng;
        let expected = crate::abilities::resolve_escape_check(&ordinary.check, &mut expected_rng);
        assert_eq!(
            game.resolve_hazard_skill_check_in_transaction(actor, Skill::Agility, 12)
                .unwrap(),
            expected
        );
        assert_eq!(game.rules_rng, expected_rng);
        assert_eq!(game.actors[&actor].active_since, 1);
        assert!(game
            .creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .all(|entry| entry.condition != condition("readied")));
    }
}
