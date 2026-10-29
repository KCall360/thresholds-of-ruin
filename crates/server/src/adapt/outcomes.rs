//! Event-time narration inputs, projected without building a display picture.
//!
//! Scene iteration deliberately matches `observation`: portal copies and cosmetic
//! names keep their canonical order. The caller applies attachment-role policy
//! before capturing outcomes, just as it does for a complete observation.
use super::*;

pub(crate) fn outcome_observation(
    view: &s::Observation,
    scene: &[w::SightCell],
    salt: &str,
    ready: bool,
    targets: &crate::wire_adapter::TargetScope,
) -> p::OutcomeObservation {
    let disclosed: std::collections::BTreeMap<_, _> = view
        .visible_cells
        .iter()
        .map(|visible| ((visible.location, visible.frame), visible))
        .collect();
    let mut visible_cells = Vec::new();
    let mut ground_items = Vec::new();
    let mut visible_actors = Vec::new();
    for cell in scene {
        let Some(&visible) = disclosed.get(&(cell.location, cell.rotation)) else {
            continue;
        };
        visible_cells.push(p::OutcomeCell {
            key: cell_key(salt, view.actor.0, cell.location),
            wall: visible.wall,
            door: visible.door.map(|door| p::OutcomeDoor {
                id: targets.door(door.id),
                open: door.open,
                name: door_appearance(visible.door_secret).0.into(),
            }),
            trap: visible.trap.as_ref().map(|trap| p::OutcomeTrap {
                id: targets.trap(trap.id),
                family: trap.family.map(trap_family),
            }),
        });
        for item in view
            .ground_items
            .iter()
            .filter(|item| item.location == cell.location)
        {
            ground_items.push(p::OutcomeGroundName {
                id: targets.item(item.id),
                name: ground_name(item.presentation.as_ref(), &item.name),
            });
        }
        for object in view
            .apparent_objects
            .iter()
            .filter(|object| object.location == cell.location)
        {
            ground_items.push(p::OutcomeGroundName {
                id: targets.apparent_object(object.subject),
                name: ground_name(object.presentation.as_ref(), &object.appearance.name),
            });
        }
        for actor in view
            .visible_actors
            .iter()
            .filter(|actor| actor.location == cell.location)
        {
            visible_actors.push(p::OutcomeActorName {
                id: targets.actor_reference(
                    actor.id,
                    actor
                        .disclosure
                        .reference()
                        .expect("selectable actor disclosure"),
                ),
                name: actor
                    .presentation
                    .as_ref()
                    .map_or_else(|| actor.name.clone(), |value| value.name.to_string()),
            });
        }
        if cell.location == view.location && cell.offset != (w::Position { x: 0, y: 0, z: 0 }) {
            visible_actors.push(p::OutcomeActorName {
                id: targets.actor(view.actor),
                name: String::new(),
            });
        }
    }
    p::OutcomeObservation {
        self_target: targets.actor(view.actor),
        tick: view.tick,
        ready,
        visible_cells,
        visible_actors,
        ground_items,
        inventory: view
            .inventory
            .iter()
            .map(|item| p::OutcomeItemName {
                id: targets.item(item.id),
                name: item.name.clone(),
            })
            .collect(),
        motion: view.motion.as_ref().map(|motion| p::OutcomeMotion {
            displaced: motion.displaced,
            impacted: motion.impacted,
        }),
        interactions: view
            .interactions
            .as_ref()
            .map(|interaction| p::OutcomeInteractions {
                completed: interaction
                    .completed
                    .iter()
                    .map(|work| {
                        crate::wire_adapter::encode_action(
                            &recorded_action(work.action()).expect("completed item action"),
                            targets,
                        )
                    })
                    .collect(),
                refused: interaction
                    .refused
                    .iter()
                    .map(|refusal| p::ItemRefusalView {
                        action: crate::wire_adapter::encode_action(
                            &recorded_action(refusal.work.action()).expect("refused item action"),
                            targets,
                        ),
                        reason: match refusal.reason {
                            s::ItemRefusal::TooWeak => p::ItemRefusalReason::TooWeak,
                        },
                    })
                    .collect(),
                curse_blocked: interaction
                    .curse_blocked
                    .iter()
                    .map(|item| targets.item(*item))
                    .collect(),
            }),
        combat: view.combat.as_ref().map(|combat| p::OutcomeCombat {
            status: p::OutcomeStatusFacts {
                hp: combat.hp,
                max_hp: combat.max_hp,
                dead: combat.dead,
                victory: combat.victory,
                terminal: combat.terminal,
                sequence: combat.sequence.as_ref().map(|sequence| p::SequenceView {
                    completed: sequence.completed,
                    total: sequence.total,
                    next_strike_remaining: sequence.next_strike_remaining,
                    cancel_recovery_remaining: sequence.cancel_recovery_remaining,
                }),
                preparation_remaining: combat.preparation_remaining,
                preparation_active: combat.preparation_active,
                recovery_remaining: combat.recovery_remaining,
            },
            abilities: combat
                .own_stats
                .as_ref()
                .map(|facts| {
                    facts
                        .abilities()
                        .iter()
                        .filter_map(|id| facts.rules().ability(id))
                        .map(|ability| p::OutcomeAbility {
                            id: ability.id.to_string(),
                            basic_attack: ability.basic_attack,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            public_plan: combat
                .public_plan
                .as_deref()
                .map(|record| crate::public_plan_view::view(record, salt, view.actor)),
            actors: combat
                .actors
                .iter()
                .map(|&(id, hostile, allied, injury)| p::CombatActorView {
                    actor: targets.actor(id),
                    hostile,
                    allied,
                    injury: injury_view(injury),
                })
                .collect(),
            events: combat
                .events
                .iter()
                .cloned()
                .map(|event| combat_event_view(event, targets))
                .collect(),
            objective: combat.objective.map(|objective| match objective {
                s::combat::ObjectiveKind::RetrieveAndReturn => p::ObjectiveKind::RetrieveAndReturn,
                s::combat::ObjectiveKind::ReachExit => p::ObjectiveKind::ReachExit,
                s::combat::ObjectiveKind::RetrieveAndExit => p::ObjectiveKind::RetrieveAndExit,
            }),
        }),
    }
}

fn ground_name(
    presentation: Option<&s::appearance_catalog::ObjectPresentation>,
    item_name: &str,
) -> p::OutcomeName {
    match presentation {
        Some(value) if value.name.trim().is_empty() => p::OutcomeName {
            text: String::new(),
            fallback: match value.kind {
                s::appearance_catalog::ObjectPresentationKind::Item { .. } => {
                    p::OutcomeNameFallback::Object
                }
                s::appearance_catalog::ObjectPresentationKind::Stairs => {
                    p::OutcomeNameFallback::Stairs
                }
            },
        },
        Some(value) => p::OutcomeName {
            text: value.name.to_string(),
            fallback: p::OutcomeNameFallback::Item,
        },
        None => p::OutcomeName {
            text: item_name.into(),
            fallback: p::OutcomeNameFallback::Item,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_narration_inputs_match_full_projection_with_cosmetics_and_receipts() {
        let mut game = s::Game::two_room_in_stone(42, crate::engine::diagnostic_content().unwrap());
        let actor = game
            .spawn_actor(
                w::Location {
                    region: w::RegionId(1),
                    position: w::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let (mut view, scene) = game.observe_scene(actor).unwrap();
        let targets = crate::wire_adapter::TargetScope::new(uuid::Uuid::from_u128(7), actor);
        let assert_projection = |view: &s::Observation, ready| {
            let compact = outcome_observation(view, &scene, "fixture-salt", ready, &targets);
            let full = observation(
                view.clone(),
                &scene,
                "fixture-salt",
                ready,
                &|_| [None, None, None],
                &targets,
            );
            assert_eq!(compact, p::OutcomeObservation::from(&full));
        };
        assert_projection(&view, true);
        let subject = s::ActorId(900);
        view.visible_actors.push(s::ActorView {
            id: subject,
            location: view.location,
            disclosure: s::perception::ActorDisclosure::Creature,
            name: "actual creature".into(),
            asset: None,
            worn: Vec::new(),
            presentation: Some(s::appearance_catalog::CreaturePresentation {
                name: "apparent creature".into(),
                asset: None,
                seed: 3,
            }),
        });
        view.apparent_objects
            .push(s::perception::ApparentObjectView {
                subject,
                location: view.location,
                appearance: s::perception::GuiseAppearance {
                    name: "actual object".into(),
                    description: "not narrated".into(),
                    class: s::ItemClass::Misc,
                    asset: None,
                },
                presentation: Some(s::appearance_catalog::ObjectPresentation {
                    name: "".into(),
                    asset: None,
                    kind: s::appearance_catalog::ObjectPresentationKind::Stairs,
                }),
            });
        view.combat = Some(s::combat::CombatView {
            own_stats: None,
            public_plan: None,
            hp: 7,
            max_hp: 10,
            preparation_remaining: Some(20),
            preparation_active: true,
            recovery_remaining: 30,
            sequence: None,
            actors: vec![(subject, true, false, s::combat::Injury::Wounded)],
            events: vec![s::combat::DisclosedCombatEvent::Ability {
                caster: Some(actor),
                caster_reference: Some(s::perception::ActorReferenceKind::Creature),
                target: Some(subject),
                target_reference: Some(s::perception::ActorReferenceKind::Creature),
                id: s::content_ids::AbilityId::new("fixture_strike").unwrap(),
                name: "Fixture Strike".into(),
                outcome: s::abilities::OutcomeStatus::Miss,
                strike: Some(1),
            }],
            objective: Some(s::combat::ObjectiveKind::RetrieveAndExit),
            exit: Some(view.location),
            victory: false,
            dead: false,
            terminal: false,
        });
        assert_projection(&view, false);
    }
}
