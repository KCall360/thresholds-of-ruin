//! Source-composed action timing, independent of attributes and elapsed clocks.
use serde::{Deserialize, Serialize};

/// A captured action-duration rate. Equal sources do not stack; opposing rates
/// cancel. Normal is the neutral source and effective value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDuration {
    #[default]
    Normal,
    Haste,
    Slow,
}

impl<'de> Deserialize<'de> for ActionDuration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "normal" => Ok(Self::Normal),
            "haste" => Ok(Self::Haste),
            "slow" => Ok(Self::Slow),
            _ => Err(serde::de::Error::custom("invalid action duration")),
        }
    }
}

impl ActionDuration {
    pub(crate) fn from_modifiers<'a>(
        values: impl Iterator<Item = &'a crate::rules::Modifier>,
    ) -> Self {
        use crate::rules::{ModifierOp, Scope};
        let (mut haste, mut slow) = (false, false);
        for modifier in values.filter(|modifier| modifier.scope == Scope::Always) {
            if let ModifierOp::ActionDuration { value } = modifier.op {
                match value {
                    Self::Normal => {}
                    Self::Haste => haste = true,
                    Self::Slow => slow = true,
                }
            }
        }
        match (haste, slow) {
            (true, false) => Self::Haste,
            (false, true) => Self::Slow,
            _ => Self::Normal,
        }
    }

    /// Scale an already-computed action duration, preserving zero-time work and
    /// rounding positive fractions upward. Ordinary actions use the u64 domain.
    pub fn apply(self, duration: u64) -> Option<u64> {
        match self {
            Self::Normal => Some(duration),
            Self::Haste => Some(duration.div_ceil(2)),
            Self::Slow => duration.checked_mul(2),
        }
    }

    /// Prepared work has a separate structural bound in addition to arithmetic.
    pub fn apply_bounded(self, duration: u64) -> Result<u64, crate::creatures::BuildError> {
        self.apply(duration)
            .ok_or(crate::creatures::BuildError::Arithmetic)
            .and_then(|value| {
                if value <= crate::limits::NUMERIC as u64 {
                    Ok(value)
                } else {
                    Err(crate::creatures::BuildError::DerivedLimit)
                }
            })
    }
}

impl crate::Actor {
    pub(crate) fn action_duration(&self) -> ActionDuration {
        self.combat
            .as_ref()
            .map_or(ActionDuration::Normal, |combat| {
                combat.creature().derived().action_duration
            })
    }

    pub(crate) fn action_duration_ticks(&self, duration: u64) -> Result<u64, crate::GameError> {
        self.action_duration()
            .apply(duration)
            .ok_or(crate::GameError::Schedule(
                crate::ScheduleError::TimeExhausted,
            ))
    }

    pub(crate) fn movement_duration(
        &self,
        direction: tor_world::Direction,
    ) -> Result<u64, crate::GameError> {
        self.action_duration_ticks(crate::movement_cost(self.turn_ticks.get(), direction)?)
    }
}

#[cfg(test)]
mod tests {
    use super::ActionDuration;
    use crate::rules::{Modifier, ModifierOp, Scope};

    fn fixture(rate: ActionDuration) -> crate::Game {
        let mut species = crate::test_creatures::species();
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::ActionDuration { value: rate },
        });
        let build = crate::test_creatures::build(species);
        let world = tor_world::World::new(
            vec![tor_world::Region {
                id: tor_world::RegionId(1),
                name: "Timing".into(),
                bounds: tor_world::Extent::new(7, 5, 1).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let mut game = crate::Game::new(world, 42, build.content().clone());
        for x in [2, 5] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 2, z: 0 },
                    },
                    std::num::NonZeroU64::new(101).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }

    #[test]
    fn ordinary_actions_capture_scaled_cost_after_diagonal_and_transfer_rounding() {
        use crate::{Action, ActorId};
        for (rate, turn, half_turn, diagonal) in [
            (ActionDuration::Normal, 101, 51, 143),
            (ActionDuration::Haste, 51, 26, 72),
            (ActionDuration::Slow, 202, 102, 286),
        ] {
            for family in 0..6 {
                let mut game = fixture(rate);
                let actor = ActorId(1);
                let at = game.actors[&actor].location;
                let (action, expected) = match family {
                    0 => (Action::Wait, turn),
                    1 => (Action::Move(tor_world::Direction::North), turn),
                    2 => (Action::Move(tor_world::Direction::NorthEast), diagonal),
                    3 => {
                        let item = game.place_item(at, "token".into()).unwrap();
                        (
                            Action::Take {
                                item,
                                quantity: None,
                            },
                            half_turn,
                        )
                    }
                    4 => {
                        game.place_item_stack(
                            10,
                            at,
                            Some(actor),
                            1,
                            crate::ItemSpec::ordinary("token".into()),
                        )
                        .unwrap();
                        (
                            Action::Drop {
                                item: crate::ItemId(10),
                                quantity: None,
                            },
                            half_turn,
                        )
                    }
                    _ => {
                        let location = tor_world::Location {
                            position: tor_world::Position { x: 3, y: 2, z: 0 },
                            ..at
                        };
                        let door = game
                            .place_door(location, false, game.door_clearance(location))
                            .unwrap();
                        (Action::SetDoor { door, open: true }, turn)
                    }
                };
                game.act(actor, action).unwrap();
                assert_eq!(game.actors[&actor].ready_at, expected, "{rate:?}/{family}");
                assert_eq!(game.actors[&actor].turn_ticks.get(), 101);
                let mut shared = crate::checkpoint::SharedState::default();
                let restored = crate::Game::restore_checkpoint(
                    game.checkpoint(&mut shared),
                    &shared,
                    game.content().clone(),
                )
                .unwrap();
                assert_eq!(restored, game);
            }
        }
    }

    #[test]
    fn item_plans_scale_preparation_but_condition_turns_keep_original_clock() {
        use crate::{ActorId, ItemId, Work};
        for rate in [ActionDuration::Haste, ActionDuration::Slow] {
            for family in 0..5 {
                let mut game = fixture(rate);
                let actor = ActorId(1);
                let at = game.actors[&actor].location;
                let item = ItemId(10);
                let mut spec = crate::ItemSpec::ordinary("timed item".into());
                let effect = crate::rules::Effect::ApplyConditionForTurns {
                    condition: crate::content_ids::ConditionId::new("braced").unwrap(),
                    duration: crate::rules::AmountExpr::Fixed { amount: 3 },
                };
                let (work, turns) = match family {
                    0 | 1 => {
                        let anatomy = game.anatomy(actor).unwrap();
                        let slot = anatomy
                            .slots
                            .iter()
                            .position(|slot| slot.as_str() == "body_armor")
                            .unwrap();
                        let kind = anatomy.slots[slot].clone();
                        let timing = game.content.rules.equipment_slot(&kind).unwrap();
                        let turns = if family == 0 {
                            timing.equip_turns
                        } else {
                            timing.unequip_turns
                        };
                        spec.class = crate::ItemClass::Armor;
                        spec.equipment = Some(crate::EquipmentSpec {
                            periodic_relocation: None,
                            potency_modifiers: Vec::new(),
                            slot: kind,
                            attack: None,
                            modifiers: vec![],
                            weapon_profile: None,
                            armor: None,
                        });
                        (
                            if family == 0 {
                                Work::Equip {
                                    item,
                                    slot: crate::EquipmentSlotId(slot as u16),
                                }
                            } else {
                                Work::Unequip { item }
                            },
                            turns,
                        )
                    }
                    2 => {
                        spec.class = crate::ItemClass::Potion;
                        spec.consumable = Some(crate::ConsumableSpec {
                            descriptors: vec![],
                            discovery: crate::ItemDiscovery::SelfEffect,
                            effects: vec![effect],
                        });
                        (
                            Work::Drink { item },
                            game.content.rules.constants().item_timing.consume_turns,
                        )
                    }
                    3 => {
                        spec.class = crate::ItemClass::Scroll;
                        spec.readable = Some(crate::ReadableSpec {
                            control: None,
                            descriptors: vec![],
                            effects: vec![effect],
                            identify: None,
                            discovery: crate::ItemDiscovery::SelfEffect,
                        });
                        (
                            Work::Read { item },
                            game.content.rules.constants().item_timing.consume_turns,
                        )
                    }
                    _ => {
                        spec.class = crate::ItemClass::Food;
                        spec.edible = Some(crate::EdibleSpec {
                            nutrition: crate::rules::AmountExpr::Fixed { amount: 3 },
                        });
                        (
                            Work::Eat { item },
                            game.content.rules.constants().item_timing.consume_turns,
                        )
                    }
                };
                game.place_item_stack(10, at, Some(actor), 1, spec).unwrap();
                if family == 1 {
                    let slot = game
                        .anatomy(actor)
                        .unwrap()
                        .slots
                        .iter()
                        .position(|slot| slot.as_str() == "body_armor")
                        .unwrap();
                    game.equip_authored(actor, item, crate::EquipmentSlotId(slot as u16))
                        .unwrap();
                }
                let plan = game.plan_work(actor, work).unwrap();
                assert_eq!(plan.preparation(), rate.apply(101 * turns).unwrap());
                assert_eq!(plan.recovery(), 0);
                if family == 2 || family == 3 {
                    assert!(matches!(
                        plan.on_success(),
                        [crate::abilities::PlannedEffect::ApplyConditionForTurns {
                            turn_ticks: 101,
                            ..
                        }]
                    ));
                }
                let restored: crate::abilities::Plan =
                    serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
                assert_eq!(restored, plan);
                assert!(restored.valid(game.content.rules()));
            }
        }
    }

    #[test]
    fn action_rates_do_not_scale_condition_cooldown_nutrition_or_recovery_clocks() {
        let actor = crate::ActorId(1);
        for rate in [ActionDuration::Haste, ActionDuration::Slow] {
            let mut changed = fixture(rate).creature(actor).unwrap().into_owned();
            let mut normal = fixture(ActionDuration::Normal)
                .creature(actor)
                .unwrap()
                .into_owned();
            for creature in [&mut changed, &mut normal] {
                creature
                    .start_cooldown(
                        &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                        307,
                    )
                    .unwrap();
                creature
                    .apply_condition(
                        &crate::content_ids::ConditionId::new("braced").unwrap(),
                        actor,
                        203,
                    )
                    .unwrap();
                creature.advance_active(37);
            }
            assert_eq!(changed.cooldowns(), normal.cooldowns());
            assert_eq!(changed.conditions(), normal.conditions());
            assert_eq!(changed.nutrition(), normal.nutrition());
            assert_eq!(changed.health_recovery(), normal.health_recovery());
            assert_eq!(changed.costs(), normal.costs());
        }
    }

    #[test]
    fn self_step_freezes_scaled_duration_and_rejects_slow_schedule_overflow() {
        let actor = crate::ActorId(1);
        for (rate, expected) in [(ActionDuration::Haste, 72), (ActionDuration::Slow, 286)] {
            let game = fixture(rate);
            let step = crate::abilities::StepCommitment::capture(
                &game,
                actor,
                crate::abilities::StepDirection::NorthEast,
            )
            .unwrap();
            assert_eq!(step.duration, expected);
            let decoded: crate::abilities::StepCommitment =
                serde_json::from_value(serde_json::to_value(step).unwrap()).unwrap();
            assert_eq!(decoded, step);
        }
        let mut game = fixture(ActionDuration::Slow);
        game.actors.get_mut(&actor).unwrap().turn_ticks =
            std::num::NonZeroU64::new(u64::MAX).unwrap();
        let before = game.clone();
        assert!(game.act(actor, crate::Action::Wait).is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn rates_preserve_zero_positive_minimum_and_exact_arithmetic_bounds() {
        use ActionDuration::{Haste, Normal, Slow};
        for rate in [Normal, Haste, Slow] {
            assert_eq!(rate.apply(0), Some(0));
        }
        assert_eq!(Haste.apply(1), Some(1));
        assert_eq!(Haste.apply(3), Some(2));
        assert_eq!(Haste.apply(u64::MAX), Some(u64::MAX / 2 + 1));
        assert_eq!(Slow.apply(u64::MAX / 2), Some(u64::MAX - 1));
        assert_eq!(Slow.apply(u64::MAX / 2 + 1), None);
        assert_eq!(Slow.apply_bounded(500_000), Ok(1_000_000));
        assert_eq!(
            Slow.apply_bounded(500_001),
            Err(crate::creatures::BuildError::DerivedLimit)
        );
    }

    #[test]
    fn composition_is_order_independent_idempotent_and_opposing_rates_cancel() {
        use ActionDuration::{Haste, Normal, Slow};
        for (rates, expected) in [
            (vec![], Normal),
            (vec![Normal], Normal),
            (vec![Haste, Haste], Haste),
            (vec![Slow, Slow], Slow),
            (vec![Haste, Normal, Slow, Haste], Normal),
            (vec![Slow, Haste], Normal),
        ] {
            let modifiers = rates
                .into_iter()
                .map(|value| Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::ActionDuration { value },
                })
                .collect::<Vec<_>>();
            assert_eq!(ActionDuration::from_modifiers(modifiers.iter()), expected);
        }
    }

    #[test]
    fn action_duration_requires_exact_string_vocabulary() {
        for value in ["normal", "haste", "slow"] {
            let encoded = serde_json::json!(value);
            let decoded: ActionDuration = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
        }
        for value in [
            serde_json::json!({"haste": null}),
            serde_json::json!({"slow": {}}),
            serde_json::json!(["haste"]),
            serde_json::json!(null),
            serde_json::json!("fast"),
        ] {
            assert!(serde_json::from_value::<ActionDuration>(value).is_err());
        }
    }

    #[test]
    fn authoring_requires_closed_named_modifier_and_global_scope() {
        let valid = serde_json::json!({
            "scope": "always", "op": {"kind": "action_duration", "value": "haste"}
        });
        let modifier = serde_json::from_value::<Modifier>(valid.clone()).unwrap();
        assert!(crate::rules::validate_modifier_shapes(&[modifier]).is_ok());
        for scope in ["versus_source", "while_no_sensed_hostile"] {
            let mut invalid = valid.clone();
            invalid["scope"] = scope.into();
            let modifier = serde_json::from_value::<Modifier>(invalid).unwrap();
            assert!(crate::rules::validate_modifier_shapes(&[modifier]).is_err());
        }
        for op in [
            serde_json::json!({"kind": "action_duration"}),
            serde_json::json!({"kind": "action_duration", "value": "fast"}),
            serde_json::json!({"kind": "action_duration", "value": {"haste": null}}),
            serde_json::json!({"kind": "action_duration", "value": "haste", "extra": 1}),
            serde_json::json!(["action_duration", "haste"]),
        ] {
            let mut invalid = valid.clone();
            invalid["op"] = op;
            assert!(serde_json::from_value::<Modifier>(invalid).is_err());
        }
    }

    #[test]
    fn slowdown_is_harmful_even_when_attached_to_a_support_condition() {
        for (value, allowed) in [(ActionDuration::Haste, true), (ActionDuration::Slow, false)] {
            let mut document = crate::test_creatures::ruleset().document().clone();
            document
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "braced")
                .unwrap()
                .modifiers = vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::ActionDuration { value },
            }];
            assert_eq!(crate::rules::Ruleset::validate(document).is_ok(), allowed);
        }
    }
    fn advancement_rate_fixture(rate: ActionDuration, recovery: u64) -> crate::Game {
        use crate::attributes::{Attributes, ByAttribute, ManaBinding, Skill};
        use crate::creatures::{Content, CreatureBuild, ScenarioCatalog};
        use crate::progression::{HdLedger, HdSource};
        let mut species = crate::test_creatures::species();
        species.default_attributes = Attributes::new(ByAttribute {
            intellect: 1,
            ..ByAttribute::default()
        })
        .unwrap();
        let species_id = species.id.clone();
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.growth.recovery = recovery;
        document.advancement.purchase_recovery = recovery;
        document.expertises.push(crate::expertise::ExpertiseDef {
            id: crate::content_ids::ExpertiseId::new("fixture_action_expertise").unwrap(),
            name: "Timing test expertise".into(),
            category: crate::expertise::ExpertiseCategory::Equipment,
        });
        document.conditions.push(crate::rules::ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: crate::content_ids::ConditionId::new("fixture_action_rate").unwrap(),
            family: crate::content_ids::ConditionFamilyId::new("fixture_action_rate").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            name: "Timing test condition".into(),
            modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::ActionDuration { value: rate },
            }],
            stacking: crate::rules::Stacking::Keep,
            source_scoped: false,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
            timing: crate::rules::ConditionTiming::UntilRemoved,
            restrictions: vec![],
            preparation: crate::rules::ConditionPreparation::Preserve,
        });
        let content = Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            ScenarioCatalog {
                species: std::collections::BTreeMap::from([(species_id.clone(), species)]),
                templates: Default::default(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            species_id,
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
        for x in [1, 2] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(101).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("owner {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game.apply_persistent_condition(
            crate::ActorId(1),
            &crate::content_ids::ConditionId::new("fixture_action_rate").unwrap(),
            crate::ActorId(1),
        )
        .unwrap();
        game
    }

    fn check_advancement_recovery(expertise: bool) {
        use crate::{Action, ActorId};
        let actor = ActorId(1);
        for recovery in [23, 0] {
            for (rate, positive_expected) in [
                (ActionDuration::Normal, 23),
                (ActionDuration::Haste, 12),
                (ActionDuration::Slow, 46),
            ] {
                let mut game = advancement_rate_fixture(rate, recovery);
                let before = game.clone();
                let creature = game.creature(actor).unwrap().into_owned();
                let basis = crate::player_advancement::AdvancementBasis {
                    actor,
                    branch: "original-branch".into(),
                    stream: "original-stream".into(),
                    reset: 7,
                    build_epoch: creature.build_epoch(),
                };
                let (action, expected_build) = if expertise {
                    let choice = crate::player_advancement::IntellectExpertiseOperation {
                        threshold: 1,
                        expertise: crate::content_ids::ExpertiseId::new("fixture_action_expertise")
                            .unwrap(),
                    };
                    let preview = game
                        .preview_player_intellect_expertise(actor, &basis, &choice)
                        .unwrap();
                    (
                        Action::ChooseIntellectExpertise { basis, choice },
                        preview.build().clone(),
                    )
                } else {
                    let source = crate::earned_growth::GrowthSource::Racial {};
                    let preview = game
                        .preview_player_growth_source(actor, &basis, &source)
                        .unwrap();
                    (
                        Action::ChooseGrowthSource { basis, source },
                        preview.build().clone(),
                    )
                };
                assert_eq!(
                    game, before,
                    "timing and advancement previews remain read-only"
                );
                let reopened_action: Action =
                    serde_json::from_value(serde_json::to_value(&action).unwrap()).unwrap();
                assert_eq!(reopened_action, action, "rate is not a new choice operand");
                game.act(actor, reopened_action).unwrap();
                let expected = if recovery == 0 { 0 } else { positive_expected };
                assert_eq!(game.tick(), 0);
                assert_eq!(
                    game.preparation_timing(actor).unwrap().recovery_remaining,
                    expected,
                    "{rate:?} must scale authored recovery {recovery}"
                );
                assert_eq!(game.actors[&actor].turn_ticks.get(), 101);
                let after = game.creature(actor).unwrap();
                assert_eq!(after.build(), &expected_build);
                assert_eq!(
                    after.build().unspent_points(),
                    creature.build().unspent_points()
                );
                assert_eq!(after.derived().attributes, creature.derived().attributes);
                assert_eq!(after.derived().skills, creature.derived().skills);
                assert_eq!(after.derived().defenses, creature.derived().defenses);
                assert_eq!(after.costs(), creature.costs());
                assert_eq!(after.cooldowns(), creature.cooldowns());
                assert_eq!(after.conditions(), creature.conditions());
                assert_eq!(after.nutrition(), creature.nutrition());
                assert_eq!(after.health_recovery(), creature.health_recovery());
                // A live rate change cannot rewrite already-paid recovery.
                game.apply(crate::abilities::Outcome {
                    resistance: None,
                    actor,
                    target: actor,
                    ability: None,
                    intention: None,
                    result: crate::abilities::OutcomeStatus::Applied,
                    check: None,
                    participation: crate::abilities::AttemptedParticipation::target(),
                    target_rider_participated: false,
                    effects: vec![crate::abilities::ResolvedEffect::RemoveCondition {
                        id: crate::content_ids::ConditionId::new("fixture_action_rate").unwrap(),
                    }],
                })
                .unwrap();
                assert_eq!(
                    game.creature(actor).unwrap().derived().action_duration,
                    ActionDuration::Normal
                );
                assert_eq!(
                    game.preparation_timing(actor).unwrap().recovery_remaining,
                    expected
                );
                let mut shared = crate::checkpoint::SharedState::default();
                let checkpoint = game.checkpoint(&mut shared);
                let bytes = serde_json::to_vec(&(checkpoint, shared)).unwrap();
                let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
                let mut reopened =
                    crate::Game::restore_checkpoint(checkpoint, &shared, game.content().clone())
                        .unwrap();
                assert_eq!(reopened, game);
                assert_eq!(
                    reopened
                        .preparation_timing(actor)
                        .unwrap()
                        .recovery_remaining,
                    expected
                );
                if expected > 0 {
                    for state in [&mut game, &mut reopened] {
                        state.act(ActorId(2), Action::Wait).unwrap();
                        assert_eq!(state.tick(), expected);
                        assert_eq!(state.next_actor(), Some(actor));
                        assert_eq!(
                            state.preparation_timing(actor).unwrap().recovery_remaining,
                            0
                        );
                    }
                    assert_eq!(reopened, game);
                }
            }
        }
    }

    #[test]
    fn growth_choice_recovery_scales_once_and_survives_rate_removal_and_reopen() {
        check_advancement_recovery(false);
    }

    #[test]
    fn intellect_expertise_recovery_scales_once_and_survives_rate_removal_and_reopen() {
        check_advancement_recovery(true);
    }
}
