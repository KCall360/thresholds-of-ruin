//! What happened, as structured beats derived from consecutive views.
//!
//! Beats name things as the character knew them at the time, so a passage
//! can still name a figure that has since died or gone out of sight.
use std::collections::BTreeMap;

use tor_client_common::Palette;
use tor_protocol::*;

use super::scene::{whereabouts, Kind, Scene};

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
    Condition(String),
    /// One step of the character's own movement.
    Stepped(Direction),
    /// The character deliberately traversed its terminal objective endpoint.
    Escaped(String),
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
}

impl Chronicler {
    /// Learn the names in a view without recording anything.
    pub fn learn(&mut self, state: &StateView, palette: &Palette) {
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
                .cloned()
                .unwrap_or_else(|| "figure".into()),
        }
    }

    /// The beats between two consecutive observations, given the history
    /// event that came with the second.
    pub fn observe(
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
            match event {
                Event::EscapeStarted { .. } => beats.push(Beat::EscapeHold(
                    tor_client_common::narration::action(event, a),
                )),
                Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
                Event::Escaped { .. } => beats.push(Beat::Escaped(
                    tor_client_common::narration::action(event, a),
                )),
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
                        .map_or_else(|| "thing".into(), |g| crate::safe(&g.item.name)),
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

fn item_name(items: &[ItemView], id: ItemTarget) -> String {
    items
        .iter()
        .find(|i| i.id == id)
        .filter(|i| !i.name.trim().is_empty())
        .map_or_else(|| "thing".into(), |i| crate::safe(&i.name).to_lowercase())
}

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

    fn view() -> StateView {
        serde_json::from_value(serde_json::json!({
            "wizard_game":false,"revision":"1","observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1; 32]),"tick":"5","position":{"x":0,"y":0,"z":0},"ready":true,
            "places":[],"visible_cells":[{"key":"here","position":{"x":0,"y":0,"z":0},
                "wall":false,"material":"stone","lit":true,"place_hint":false,
                "stairs_up":false,"stairs_down":false,
                "door":{"id":tor_protocol::DoorTarget::from_digest([7; 32]),"name":"Oak Door","description":"","open":false,
                    "reachable":true,"approaches":[]}}],
            "ground_items":[],"inventory":[],
            "linked_holds":[],"visible_actors":[{"id":tor_protocol::ActorTarget::from_digest([2; 32]),"name":"ruin scout","description":"",
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
                event,
            },
        }
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
                "known_supply":null,"quantity":"1","class":"weapon","appearance":"item",
                "identified":true,"id":item,"name":"iron sword","description":""
            }))
            .unwrap(),
        );
        let slot = tor_client_common::test_support::equipment_slot("hand", "in hand", true);
        state.observation.interactions = Some(InteractionView {
            identification: None,
            completed: vec![],
            slots: vec![slot.clone()],
            preparation: None,
            inventory: vec![ItemInteractionView {
                item,
                slot: Some(slot),
                equipped_slot: Some(0),
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                charges: None,
            }],
        });
        state
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
}
