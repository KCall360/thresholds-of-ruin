//! What happened, as structured beats derived from ordered event-time facts.
//!
//! Beats name things as the character knew them at the time, so a passage
//! can still name a figure that has since died or gone out of sight.
use std::collections::BTreeMap;

use tor_client_common::Palette;
use tor_protocol::*;

use super::scene::whereabouts;
use super::scene::{Kind, Scene};

/// A figure as it was known when the beat happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Figure {
    pub id: ActorTarget,
    pub name: String,
}

/// Who did or suffered something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Who {
    Me,
    Figure(Figure),
    /// Someone the character couldn't see.
    Unseen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beat {
    Discovery(String),
    Condition(String),
    /// One step of the character's own movement.
    Stepped(Direction),
    /// The character deliberately traversed its terminal objective endpoint.
    Escaped(String),
    Crumbled {
        quantity: u64,
    },
    Took {
        name: String,
        quantity: u64,
    },
    Dropped {
        name: String,
        quantity: u64,
    },
    /// The character opened or closed a door.
    SetDoor {
        name: String,
        open: bool,
    },
    Waited,
    ItemBegan(String),
    ItemFinished(String),
    AbilityBegan(String),
    /// Generic linked-hold progress never carries the holder identity.
    EscapeHold(String),
    AbilityResolved {
        ability: ContentRef,
        attacker: Who,
        target: Who,
        outcome: AbilityOutcome,
        strike: Option<u16>,
        /// A plain attack rather than a technique.
        basic: bool,
        /// Which telling to use, so blows in a row read differently.
        variant: u32,
    },
    /// A figure's visible condition changed, short of death.
    Injured {
        figure: Figure,
        level: Injury,
    },
    SequenceStatus(String),
    /// A paid observation expressed entirely from its immutable public record.
    PublicPlan(String),
    /// The server refused execution while the disclosed intention stays queued.
    ExecutionRefused(String),
    /// The character's attack was interrupted.
    Interrupted,
    Died(Who),
    Appeared {
        figure: Figure,
        whereabouts: String,
    },
    /// A walk stopped because this came into view: "a copper token".
    Spotted {
        what: String,
        whereabouts: String,
    },
    Vanished(Figure),
    /// A door changed state, not by the character's hand.
    DoorChanged {
        name: String,
        whereabouts: String,
        open: bool,
    },
    Displaced,
    Impacted,
    Hp {
        from: u64,
        to: u64,
        max: u64,
    },
    Victory {
        terminal: bool,
    },
    Objective(ObjectiveKind),
    /// A journey ended.
    Journey {
        phase: TravelPhase,
    },
    Control(bool),
    /// A figure stands where a blocked journey would have gone.
    Barred(Figure),
    /// The state was replaced, as by a rewind or resynchronization.
    Resync,
}

/// Remembers names, so beats can name figures after they're gone.
#[derive(Clone, Debug, Default)]
pub struct Chronicler {
    names: BTreeMap<ActorTarget, String>,
    /// Blows told so far, so each is worded afresh.
    blows: u32,
    status: Option<OutcomeStatusFacts>,
}

impl Chronicler {
    /// Learn the names in a view without recording anything.
    pub fn learn(&mut self, state: &StateView, palette: &Palette) {
        if self.status.is_none() {
            self.status = state
                .observation
                .combat
                .as_ref()
                .map(OutcomeStatusFacts::from);
        }
        for r in Scene::new(state, palette).of(Kind::Figure) {
            if let super::scene::Key::Actor(id) = r.key {
                self.names.insert(id, r.name.clone());
            }
        }
    }

    fn who(&self, actor: Option<ActorTarget>, me: ActorTarget) -> Who {
        match actor {
            Some(id) if id == me => Who::Me,
            Some(id) => Who::Figure(self.figure(id)),
            None => Who::Unseen,
        }
    }

    fn figure(&self, id: ActorTarget) -> Figure {
        Figure {
            id,
            name: self
                .names
                .get(&id)
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "figure".into()),
        }
    }

    /// Consume event-time facts; never reconstruct action results from visuals.
    pub fn outcome(&mut self, facts: &OutcomeFacts) -> Vec<Beat> {
        for actor in &facts.actor_names {
            self.names.insert(actor.actor, crate::safe(&actor.name));
        }
        for fact in &facts.facts {
            if let OutcomeFact::Noticed { actor, name, .. }
            | OutcomeFact::LostSight { actor, name } = fact
            {
                self.names.insert(*actor, crate::safe(name));
            }
        }
        // Prose has always told combat, wounds, plans, then progress. Sort
        // typed facts directly; rendering never relies on positional pairing.
        let mut ordered: Vec<_> = facts.facts.iter().collect();
        ordered.sort_by_key(|fact| match fact {
            OutcomeFact::OwnAction { .. } => 0,
            OutcomeFact::ItemCompleted { .. } => 1,
            OutcomeFact::TrapDiscovered { .. } | OutcomeFact::DoorDiscovered => 2,
            OutcomeFact::Displaced | OutcomeFact::Impacted => 3,
            OutcomeFact::Combat { .. } => 4,
            OutcomeFact::Injured { .. } => 5,
            OutcomeFact::PublicPlan { .. } => 6,
            OutcomeFact::Status { .. } => 7,
            OutcomeFact::Objective { .. } => 8,
            OutcomeFact::Noticed { .. } => 9,
            OutcomeFact::LostSight { .. } => 10,
            OutcomeFact::DoorChanged { .. } => 11,
            OutcomeFact::ItemRefused { .. }
            | OutcomeFact::CurseBlocked { .. }
            | OutcomeFact::Pacing { .. } => 12,
        });
        let mut beats = Vec::new();
        for fact in ordered {
            match fact {
                OutcomeFact::Injured { actor, injury } => {
                    beats.push(Beat::Injured {
                        figure: self.figure(*actor),
                        level: *injury,
                    });
                    continue;
                }
                OutcomeFact::Objective { objective } => {
                    beats.push(Beat::Objective(*objective));
                    continue;
                }
                _ => {}
            }
            let Some(line) = tor_client_common::narration::outcome_line(facts, fact) else {
                continue;
            };
            match fact {
                OutcomeFact::OwnAction {
                    event,
                    name,
                    basic_attack,
                } => {
                    let name = name
                        .as_ref()
                        .map_or_else(|| "thing".into(), |name| crate::safe(&name.text));
                    match event {
                        Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
                        Event::Waited => beats.push(Beat::Waited),
                        Event::Taken { quantity, .. } => beats.push(Beat::Took {
                            name,
                            quantity: *quantity,
                        }),
                        Event::Dropped { quantity, .. } => beats.push(Beat::Dropped {
                            name,
                            quantity: *quantity,
                        }),
                        Event::Crumbled { quantity, .. } => beats.push(Beat::Crumbled {
                            quantity: *quantity,
                        }),
                        Event::DoorChanged { open, .. } => {
                            beats.push(Beat::SetDoor { name, open: *open })
                        }
                        Event::Escaped { .. } => beats.push(Beat::Escaped(line.text)),
                        Event::EscapeStarted { .. } => beats.push(Beat::EscapeHold(line.text)),
                        Event::ItemStarted { .. } => beats.push(Beat::ItemBegan(line.text)),
                        Event::AbilityStarted { .. } if *basic_attack => {
                            beats.push(Beat::AbilityBegan("You ready yourself to strike.".into()));
                        }
                        Event::AbilityStarted { .. } => beats.push(Beat::AbilityBegan(line.text)),
                        Event::PreparationPaused => {}
                    }
                }
                OutcomeFact::Combat {
                    event,
                    basic_attack,
                } => beats.push(match event {
                    CombatEventView::Ability {
                        caster,
                        target,
                        ability,
                        outcome,
                        strike,
                    } => {
                        self.blows = self.blows.wrapping_add(1);
                        Beat::AbilityResolved {
                            ability: ability.clone(),
                            attacker: self.who(*caster, facts.self_target),
                            target: self.who(*target, facts.self_target),
                            outcome: *outcome,
                            strike: *strike,
                            basic: *basic_attack,
                            variant: self.blows,
                        }
                    }
                    CombatEventView::Died { actor } => {
                        Beat::Died(self.who(Some(*actor), facts.self_target))
                    }
                    CombatEventView::Interrupted { .. } => Beat::Interrupted,
                    CombatEventView::Condition { condition } => {
                        Beat::Condition(condition.name.clone())
                    }
                    _ => Beat::EscapeHold(line.text),
                }),
                OutcomeFact::Status { status } => {
                    if self.status.as_ref().is_none_or(|old| {
                        old.sequence != status.sequence
                            || old.preparation_active != status.preparation_active
                    }) {
                        if let Some(line) =
                            tor_client_common::narration::sequence_status_facts(status)
                        {
                            beats.push(Beat::SequenceStatus(line));
                        }
                    }
                    if let Some(old) = &self.status {
                        if old.hp != status.hp {
                            beats.push(Beat::Hp {
                                from: old.hp,
                                to: status.hp,
                                max: status.max_hp,
                            });
                        }
                    }
                    if status.victory && self.status.as_ref().is_none_or(|old| !old.victory) {
                        beats.push(Beat::Victory {
                            terminal: status.terminal,
                        });
                    }
                    self.status = Some(status.clone());
                }
                OutcomeFact::ItemCompleted { .. } => beats.push(Beat::ItemFinished(line.text)),
                OutcomeFact::PublicPlan { .. } => beats.push(Beat::PublicPlan(line.text)),
                OutcomeFact::Displaced => beats.push(Beat::Displaced),
                OutcomeFact::Impacted => beats.push(Beat::Impacted),
                OutcomeFact::LostSight { actor, .. } => {
                    beats.push(Beat::Vanished(self.figure(*actor)))
                }
                OutcomeFact::Noticed {
                    actor, position, ..
                } => beats.push(Beat::Appeared {
                    figure: self.figure(*actor),
                    whereabouts: whereabouts(*position),
                }),
                OutcomeFact::DoorChanged { .. }
                | OutcomeFact::TrapDiscovered { .. }
                | OutcomeFact::DoorDiscovered => beats.push(Beat::Discovery(line.text)),
                OutcomeFact::ItemRefused { .. } | OutcomeFact::CurseBlocked { .. } => {
                    beats.push(Beat::Condition(line.text))
                }
                OutcomeFact::Pacing { .. } => {}
                OutcomeFact::Injured { .. } | OutcomeFact::Objective { .. } => unreachable!(),
            }
        }
        self.status = facts.status.clone();
        beats
    }

    /// The beats between two consecutive observations, given the history
    /// event that came with the second.
    #[cfg(test)]
    fn observe(
        &mut self,
        before: &StateView,
        after: &StateView,
        event: Option<&HistoryEntry>,
        palette: &Palette,
    ) -> Vec<Beat> {
        let (b, a) = (&before.observation, &after.observation);
        let then = Scene::new(before, palette);
        let now = Scene::new(after, palette);
        self.learn(before, palette);
        self.learn(after, palette);
        let mut beats = Vec::new();
        let mut own_door = None;
        if let Some(HistoryEntry {
            content: HistoryContent::Action { event, .. },
            ..
        }) = event
        {
            match event.as_ref() {
                Event::EscapeStarted { .. } => beats.push(Beat::EscapeHold(
                    tor_client_common::narration::action(event, a),
                )),
                Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
                Event::Escaped { .. } => beats.push(Beat::Escaped(
                    tor_client_common::narration::action(event, a),
                )),
                Event::Crumbled { quantity, .. } => beats.push(Beat::Crumbled {
                    quantity: *quantity,
                }),
                Event::Taken {
                    result, quantity, ..
                } => beats.push(Beat::Took {
                    name: item_name(&a.inventory, *result),
                    quantity: *quantity,
                }),
                Event::Dropped {
                    result, quantity, ..
                } => beats.push(Beat::Dropped {
                    name: a
                        .ground_items
                        .iter()
                        .find(|g| g.item.id == *result)
                        .map_or_else(
                            || "thing".into(),
                            |g| crate::safe(tor_client_common::items::ground_name(g)),
                        ),
                    quantity: *quantity,
                }),
                Event::DoorChanged { door, open } => {
                    own_door = Some(*door);
                    beats.push(Beat::SetDoor {
                        name: door_name(a, *door),
                        open: *open,
                    })
                }
                Event::Waited => beats.push(Beat::Waited),
                Event::ItemStarted { .. } => beats.push(Beat::ItemBegan(
                    tor_client_common::narration::action(event, a),
                )),
                Event::AbilityStarted { ability, .. } if basic(a, ability) => {
                    beats.push(Beat::AbilityBegan("You ready yourself to strike.".into()))
                }
                Event::AbilityStarted { .. } => beats.push(Beat::AbilityBegan(
                    tor_client_common::narration::action(event, a),
                )),
                Event::PreparationPaused => {}
            }
        }
        beats.extend(
            tor_client_common::narration::completed_items(b, a)
                .into_iter()
                .map(Beat::ItemFinished),
        );
        beats.extend(
            tor_client_common::narration::discovery_changes(b, a)
                .into_iter()
                .map(Beat::Discovery),
        );
        if a.tick != b.tick {
            if let Some(motion) = &a.motion {
                if motion.displaced {
                    beats.push(Beat::Displaced);
                }
                if motion.impacted {
                    beats.push(Beat::Impacted);
                }
            }
        }
        if let Some(combat) = &a.combat {
            let fresh = a.tick != b.tick
                || event.is_some()
                || b.combat.as_ref().is_none_or(|c| c.events != combat.events);
            if fresh {
                for e in &combat.events {
                    beats.push(match *e {
                        CombatEventView::Ability {
                            caster,
                            target,
                            ref ability,
                            outcome,
                            strike,
                        } => {
                            let attacker = self.who(caster, a.self_target);
                            let basic = attacker == Who::Me && basic(a, ability);
                            self.blows = self.blows.wrapping_add(1);
                            Beat::AbilityResolved {
                                ability: ability.clone(),
                                attacker,
                                target: self.who(target, a.self_target),
                                outcome,
                                strike,
                                basic,
                                variant: self.blows,
                            }
                        }
                        CombatEventView::UnseenAbility { .. }
                        | CombatEventView::AnonymousAbility { .. }
                        | CombatEventView::EscapeResolved { .. } => {
                            Beat::EscapeHold(tor_client_common::narration::combat_event(e, b, a))
                        }
                        CombatEventView::Condition { ref condition } => {
                            Beat::Condition(condition.name.clone())
                        }
                        CombatEventView::Interrupted { .. } => Beat::Interrupted,
                        CombatEventView::Died { actor } => {
                            Beat::Died(self.who(Some(actor), a.self_target))
                        }
                    });
                }
            }
            let old = b.combat.as_ref();
            // Wounds told as they show: "it staggers, near death".
            let dying: Vec<ActorTarget> = combat
                .events
                .iter()
                .filter_map(|e| match e {
                    CombatEventView::Died { actor } => Some(*actor),
                    _ => None,
                })
                .collect();
            if let Some(old) = old {
                for now in &combat.actors {
                    let was = old.actors.iter().find(|o| o.actor == now.actor);
                    if was.is_some_and(|w| w.injury != now.injury)
                        && !dying.contains(&now.actor)
                        && now.actor != a.self_target
                    {
                        beats.push(Beat::Injured {
                            figure: self.figure(now.actor),
                            level: now.injury,
                        });
                    }
                }
            }
            if let Some(line) = tor_client_common::public_plan::changed_line(
                old.and_then(|combat| combat.public_plan.as_ref()),
                combat.public_plan.as_ref(),
            ) {
                beats.push(Beat::PublicPlan(line));
            }
            if old.is_none_or(|c| {
                c.sequence != combat.sequence || c.preparation_active != combat.preparation_active
            }) {
                if let Some(status) = tor_client_common::narration::sequence_status(combat) {
                    beats.push(Beat::SequenceStatus(status));
                }
            }
            if old.is_none_or(|c| c.hp != combat.hp) {
                if let Some(old) = old {
                    beats.push(Beat::Hp {
                        from: old.hp,
                        to: combat.hp,
                        max: combat.max_hp,
                    });
                }
            }
            if combat.victory && !old.is_some_and(|c| c.victory) {
                beats.push(Beat::Victory {
                    terminal: combat.terminal,
                });
            }
            if let Some(objective) = combat.objective {
                if old.is_none_or(|c| c.objective.is_none()) {
                    beats.push(Beat::Objective(objective));
                }
            }
        }
        let figures = |scene: &Scene| -> BTreeMap<ActorTarget, String> {
            scene
                .of(Kind::Figure)
                .filter_map(|r| match r.key {
                    super::scene::Key::Actor(id) => Some((id, whereabouts(r.position?))),
                    _ => None,
                })
                .collect()
        };
        let (seen, seeing) = (figures(&then), figures(&now));
        for (id, place) in &seeing {
            if !seen.contains_key(id) {
                beats.push(Beat::Appeared {
                    figure: self.figure(*id),
                    whereabouts: place.clone(),
                });
            }
        }
        for id in seen.keys() {
            if !seeing.contains_key(id) {
                beats.push(Beat::Vanished(self.figure(*id)));
            }
        }
        let doors = |o: &Observation| -> BTreeMap<DoorTarget, (bool, Position)> {
            o.visible_cells
                .iter()
                .filter_map(|c| c.door.as_ref().map(|d| (d.id, (d.open, c.position))))
                .collect()
        };
        let old_doors = doors(b);
        for (id, (open, position)) in doors(a) {
            if Some(id) != own_door
                && old_doors
                    .get(&id)
                    .is_some_and(|(was_open, _)| *was_open != open)
            {
                beats.push(Beat::DoorChanged {
                    name: door_name(a, id),
                    whereabouts: whereabouts(position),
                    open,
                });
            }
        }
        beats
    }
}

/// Whether the character's own disclosed ability is a plain attack. Names
/// and IDs identify content; they never supply missing source-owned semantics.
#[cfg(test)]
fn basic(o: &Observation, ability: &ContentRef) -> bool {
    o.combat
        .as_ref()
        .and_then(|c| c.own_stats.as_ref())
        .is_some_and(|s| {
            s.abilities
                .iter()
                .any(|a| a.basic_attack && a.id == ability.id)
        })
}

#[cfg(test)]
fn item_name(items: &[ItemView], id: ItemTarget) -> String {
    items
        .iter()
        .find(|i| i.id == id)
        .filter(|i| !i.name.trim().is_empty())
        .map_or_else(|| "thing".into(), |i| crate::safe(&i.name).to_lowercase())
}

#[cfg(test)]
fn door_name(o: &Observation, id: DoorTarget) -> String {
    o.visible_cells
        .iter()
        .filter_map(|c| c.door.as_ref())
        .find(|d| d.id == id)
        .filter(|d| !d.name.trim().is_empty())
        .map_or_else(|| "door".into(), |d| crate::safe(&d.name).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_noticed_outcome_uses_generic_figure_without_a_new_picture() {
        let before = view();
        let actor = ActorTarget::from_digest([91; 32]);
        let mut chronicler = Chronicler::default();
        for name in ["guard", "", "   "] {
            let facts = OutcomeFacts {
                self_target: before.observation.self_target,
                status: None,
                actor_names: Vec::new(),
                facts: vec![OutcomeFact::Noticed {
                    actor,
                    name: name.into(),
                    position: Position { x: 1, y: 0, z: 0 },
                }],
            };
            let beats = chronicler.outcome(&facts);
            assert!(
                matches!(beats.as_slice(), [Beat::Appeared { figure, whereabouts }]
                if figure.id == actor
                    && figure.name == (if name.trim().is_empty() { "figure" } else { name })
                    && whereabouts == "to the east")
            );
        }
    }

    #[test]
    fn ordered_outcomes_narrate_without_refreshing_the_visual_view() {
        let before = view();
        let mut after = before.clone();
        after.observation.tick += 1;
        after.observation.combat.as_mut().unwrap().hp = 40;
        let event = Event::Moved {
            direction: Direction::North,
        };
        let facts = OutcomeFacts::capture(&before.observation, &after.observation, Some(&event));
        let mut chronicler = Chronicler::default();
        chronicler.learn(&before, &Palette::default());
        let beats = chronicler.outcome(&facts);
        assert!(beats.contains(&Beat::Stepped(Direction::North)));
        assert!(beats.contains(&Beat::Hp {
            from: 50,
            to: 40,
            max: 50
        }));
        assert_eq!(before.observation.combat.as_ref().unwrap().hp, 50);
    }

    #[test]
    fn outcome_keeps_legacy_combat_wounds_plan_progress_and_objective_order() {
        let mut before = view();
        let subject = before.observation.visible_actors[0].id;
        before
            .observation
            .combat
            .as_mut()
            .unwrap()
            .actors
            .push(CombatActorView {
                actor: subject,
                hostile: true,
                allied: false,
                injury: Injury::Healthy,
            });
        let mut after = before.clone();
        after.observation.tick += 1;
        let me = after.observation.self_target;
        let tick = after.observation.tick;
        let combat = after.observation.combat.as_mut().unwrap();
        combat.events.push(CombatEventView::Ability {
            caster: Some(me),
            target: Some(subject),
            ability: ContentRef {
                id: "test.strike".into(),
                name: "Strike".into(),
            },
            outcome: AbilityOutcome::Applied,
            strike: Some(1),
        });
        combat.actors[0].injury = Injury::Wounded;
        combat.public_plan = Some(PublicPlanObservationView {
            observed_at: tick,
            subject_name: "remembered guard".into(),
            state: PublicPlanStateView::Preparing {
                ability: None,
                aim: ObservedAimView::Unavailable,
                remaining_work: 200,
            },
        });
        combat.sequence = Some(SequenceView {
            completed: 1,
            total: 3,
            next_strike_remaining: 5,
            cancel_recovery_remaining: 35,
        });
        combat.preparation_active = true;
        combat.hp = 40;
        combat.victory = true;
        combat.objective = Some(ObjectiveKind::RetrieveAndReturn);
        let facts = OutcomeFacts::capture(&before.observation, &after.observation, None);
        let expected = Chronicler::default().observe(&before, &after, None, &Palette::default());
        let mut current = Chronicler::default();
        current.learn(&before, &Palette::default());
        let actual = current.outcome(&facts);
        assert_eq!(actual, expected);
        assert!(matches!(
            actual.as_slice(),
            [
                Beat::AbilityResolved { .. },
                Beat::Injured { .. },
                Beat::PublicPlan(_),
                Beat::SequenceStatus(_),
                Beat::Hp { .. },
                Beat::Victory { .. },
                Beat::Objective(_),
            ]
        ));
    }

    fn view() -> StateView {
        serde_json::from_value(serde_json::json!({
            "snapshot":"chronicle-fixture","wizard_game":false,"revision":"1","observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1; 32]),"tick":"5","position":{"x":0,"y":0,"z":0},"ready":true,
            "places":[],"visible_cells":[{"key":"here","position":{"x":0,"y":0,"z":0},
                "wall":false,"material":"stone","lit":true,"place_hint":false,
                "trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,
                "door":{"id":tor_protocol::DoorTarget::from_digest([7; 32]),"name":"Oak Door","description":"","open":false,
                    "reachable":true,"approaches":[]}}],
            "ground_items":[],"inventory":[],
            "charts":[],"linked_holds":[],"visible_actors":[{"id":tor_protocol::ActorTarget::from_digest([2; 32]),"name":"ruin scout","description":"",
                "position":{"x":2,"y":0,"z":0}}],
            "combat":{"public_plan":null,"hp":"50","max_hp":"50","sequence":null,"preparation_remaining":null,
                "preparation_active":false,"recovery_remaining":"0","actors":[],
                "events":[],"objective":null,"victory":false,"dead":false,
                "terminal":false}}}))
        .unwrap()
    }

    fn action(event: Event) -> HistoryEntry {
        HistoryEntry {
            id: EntryId("e".into()),
            branch: BranchId("main".into()),
            actor: ActorId(1),
            tick: 6,
            author: Author::Backend {
                component: "test".into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Action {
                action: Action::Wait,
                event: Box::new(event),
            },
        }
    }

    #[test]
    fn anonymous_effect_feedback_never_becomes_a_named_ability_beat() {
        let before = view();
        let mut after = before.clone();
        after.observation.tick += 1;
        let target = after.observation.visible_actors[0].id;
        after.observation.combat.as_mut().unwrap().events =
            vec![CombatEventView::AnonymousAbility {
                caster: Some(after.observation.self_target),
                target: Some(target),
                outcome: AbilityOutcome::Unaffected,
            }];
        let beats = Chronicler::default().observe(&before, &after, None, &Palette::default());
        assert!(beats.contains(&Beat::EscapeHold(
            "You used an unidentified effect on the ruin scout.".into()
        )));
        assert!(!beats
            .iter()
            .any(|beat| matches!(beat, Beat::AbilityResolved { .. })));
    }

    #[test]
    fn ability_starts_and_effects_become_distinct_narrative_beats() {
        let before = view();
        let mut after = view();
        after.observation.tick = 6;
        let target = after.observation.visible_actors[0].id;
        after.observation.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
            strike: None,
            caster: Some(after.observation.self_target),
            target: Some(target),
            ability: tor_client_common::test_support::content("fear"),
            outcome: AbilityOutcome::Applied,
        }];
        let entry = action(Event::AbilityStarted {
            ability: tor_client_common::test_support::content("fear"),
            target,
        });
        let beats =
            Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
        assert!(beats.contains(&Beat::AbilityBegan("You prepare to use Fear.".into())));
        assert!(beats
            .iter()
            .any(|beat| matches!(beat,Beat::AbilityResolved{ability,..} if ability.id=="fear")));
    }

    fn armed_view() -> StateView {
        let mut state = view();
        state.observation.combat.as_mut().unwrap().own_stats =
            Some(tor_client_common::test_support::stats());
        let item = ItemTarget::from_digest([8; 32]);
        state.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "known_currency":null,"known_supply":null,"quantity":"1","class":"weapon","appearance":"item",
                "identified":true,"id":item,"name":"iron sword","description":""
            }))
            .unwrap(),
        );
        let slot = tor_client_common::test_support::equipment_slot("hand", "in hand", true);
        state.observation.interactions = Some(InteractionView {
            refused: vec![],
            curse_blocked: vec![],
            identification: None,
            completed: vec![],
            slots: vec![slot.clone()],
            preparation: None,
            inventory: vec![ItemInteractionView {
                item,
                slot: Some(slot),
                equipped_slot: Some(0),
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
            }],
        });
        state
    }

    #[test]
    fn outcome_preparation_prose_uses_disclosed_basic_flag_not_ability_name() {
        for basic_attack in [false, true] {
            let mut before = armed_view();
            let mut disclosed = tor_client_common::test_support::ability("custom.technique");
            disclosed.name = "Renamed Technique".into();
            disclosed.basic_attack = basic_attack;
            before
                .observation
                .combat
                .as_mut()
                .unwrap()
                .own_stats
                .as_mut()
                .unwrap()
                .abilities = vec![disclosed];
            let mut after = before.clone();
            after.observation.tick += 1;
            let event = Event::AbilityStarted {
                ability: ContentRef {
                    id: "custom.technique".into(),
                    name: "Renamed Technique".into(),
                },
                target: after.observation.visible_actors[0].id,
            };
            let entry = action(event.clone());
            let expected =
                Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
            let facts =
                OutcomeFacts::capture(&before.observation, &after.observation, Some(&event));
            let actual = Chronicler::default().outcome(&facts);
            assert_eq!(actual, expected);
            assert_eq!(
                actual,
                vec![Beat::AbilityBegan(
                    if basic_attack {
                        "You ready yourself to strike."
                    } else {
                        "You prepare to use Renamed Technique."
                    }
                    .into()
                )]
            );
        }
    }

    #[test]
    fn attack_names_and_ids_do_not_override_own_source_semantics() {
        for (id, name) in [("technique", "Attack"), ("basic_melee", "Rattling Blow")] {
            let before = armed_view();
            let mut after = before.clone();
            after.observation.tick += 1;
            let ability = ContentRef {
                id: id.into(),
                name: name.into(),
            };
            let combat = after.observation.combat.as_mut().unwrap();
            let mut disclosed = tor_client_common::test_support::ability(id);
            disclosed.name = name.into();
            disclosed.basic_attack = false;
            combat.own_stats.as_mut().unwrap().abilities = vec![disclosed];
            combat.events = vec![CombatEventView::Ability {
                caster: Some(after.observation.self_target),
                target: Some(after.observation.visible_actors[0].id),
                ability: ability.clone(),
                outcome: AbilityOutcome::Applied,
                strike: None,
            }];
            let entry = action(Event::AbilityStarted {
                ability: ability.clone(),
                target: after.observation.visible_actors[0].id,
            });
            let beats =
                Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
            assert!(
                beats.contains(&Beat::AbilityBegan(format!("You prepare to use {name}."))),
                "{beats:?}"
            );
            assert!(
                beats.iter().any(|beat| matches!(beat,
                    Beat::AbilityResolved { basic: false, ability: named, .. } if *named == ability
                )),
                "{beats:?}"
            );
        }
    }

    #[test]
    fn a_basic_flag_classifies_only_the_disclosed_source_without_binding_its_weapon() {
        for (caster_is_me, disclose_stats, expected_basic) in [
            (true, true, true),
            (true, false, false),
            (false, true, false),
            (false, false, false),
        ] {
            let before = armed_view();
            let mut after = before.clone();
            after.observation.tick += 1;
            let caster = if caster_is_me {
                after.observation.self_target
            } else {
                after.observation.visible_actors[0].id
            };
            let combat = after.observation.combat.as_mut().unwrap();
            if !disclose_stats {
                combat.own_stats = None;
            }
            combat.events = vec![CombatEventView::Ability {
                caster: Some(caster),
                target: Some(after.observation.self_target),
                ability: ContentRef {
                    id: "basic_melee".into(),
                    name: "Attack".into(),
                },
                outcome: AbilityOutcome::Applied,
                strike: None,
            }];
            let started = caster_is_me.then(|| {
                action(Event::AbilityStarted {
                    ability: ContentRef {
                        id: "basic_melee".into(),
                        name: "Attack".into(),
                    },
                    target: after.observation.self_target,
                })
            });
            let beats = Chronicler::default().observe(
                &before,
                &after,
                started.as_ref(),
                &Palette::default(),
            );
            if expected_basic {
                assert!(beats.contains(&Beat::AbilityBegan("You ready yourself to strike.".into())));
            }
            assert!(
                beats.iter().any(|beat| matches!(beat,
                    Beat::AbilityResolved { basic, .. }
                        if *basic == expected_basic
                )),
                "caster_is_me={caster_is_me}, disclose_stats={disclose_stats}: {beats:?}"
            );
        }
    }

    #[test]
    fn a_deadly_blow_names_the_dead_after_they_vanish() {
        let before = view();
        let mut after = view();
        after.observation.tick = 6;
        after.observation.visible_actors.clear();
        let combat = after.observation.combat.as_mut().unwrap();
        combat.hp = 44;
        combat.events = vec![
            CombatEventView::Ability {
                strike: None,
                ability: tor_client_common::test_support::content("basic_melee"),
                caster: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
                target: Some(tor_protocol::ActorTarget::from_digest([2; 32])),
                outcome: AbilityOutcome::Applied,
            },
            CombatEventView::Died {
                actor: tor_protocol::ActorTarget::from_digest([2; 32]),
            },
        ];
        let scout = Figure {
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
            name: "ruin scout".into(),
        };
        assert_eq!(
            Chronicler::default().observe(&before, &after, None, &Palette::default()),
            [
                Beat::AbilityResolved {
                    strike: None,
                    ability: tor_client_common::test_support::content("basic_melee"),
                    basic: false,
                    variant: 1,
                    attacker: Who::Me,
                    target: Who::Figure(scout.clone()),
                    outcome: AbilityOutcome::Applied,
                },
                Beat::Died(Who::Figure(scout.clone())),
                Beat::Hp {
                    from: 50,
                    to: 44,
                    max: 50,
                },
                Beat::Vanished(scout),
            ]
        );
    }

    #[test]
    fn own_door_actions_are_not_also_told_as_changes() {
        let before = view();
        let mut after = view();
        after.observation.tick = 6;
        after.observation.visible_cells[0]
            .door
            .as_mut()
            .unwrap()
            .open = true;
        let mut chronicler = Chronicler::default();
        let palette = Palette::default();
        assert_eq!(
            chronicler.observe(
                &before,
                &after,
                Some(&action(Event::DoorChanged {
                    door: tor_protocol::DoorTarget::from_digest([7; 32]),
                    open: true
                })),
                &palette
            ),
            [Beat::SetDoor {
                name: "oak door".into(),
                open: true
            }]
        );
        assert_eq!(
            chronicler.observe(&before, &after, None, &palette),
            [Beat::DoorChanged {
                name: "oak door".into(),
                whereabouts: "at your feet".into(),
                open: true
            }]
        );
        // An unchanged view tells nothing new.
        assert!(chronicler
            .observe(&after, &after, None, &palette)
            .is_empty());
    }
    #[test]
    fn dropped_object_narration_captures_cosmetic_name_without_true_sibling() {
        let before = view();
        let mut after = before.clone();
        after.observation.tick += 1;
        let id = ItemTarget::from_digest([82; 32]);
        after.observation.ground_items.push(GroundItemView {
            presentation: Some(ObjectPresentation {
                name: "silver flask".into(),
                asset: None,
                kind: ObjectPresentationKind::Item {
                    class: ItemClass::Potion,
                },
            }),
            item: ItemView {
                corpse: None,
                class: ItemClass::Misc,
                quantity: 1,
                appearance: "object".into(),
                identified: true,
                description: String::new(),
                id,
                name: "object".into(),
                asset: None,
                known_supply: None,
                known_currency: None,
            },
            position: after.observation.position,
            reachable: true,
            supply_reachable: false,
        });
        let event = Event::Dropped {
            item: id,
            result: id,
            quantity: 1,
        };
        assert_eq!(
            tor_client_common::narration::action(&event, &after.observation),
            "You drop 1 x silver flask."
        );
        let entry = action(event);
        let beats =
            Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
        assert!(beats.contains(&Beat::Dropped {
            name: "silver flask".into(),
            quantity: 1
        }));
    }
}
