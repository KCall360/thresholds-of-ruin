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
    pub id: ActorId,
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
    /// One step of the character's own movement.
    Stepped(Direction),
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
    AttackBegan,
    Blow {
        attacker: Who,
        target: Who,
        outcome: AttackOutcome,
    },
    /// The character's attack was interrupted.
    Interrupted,
    Died(Who),
    Appeared {
        figure: Figure,
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
        from: u32,
        to: u32,
        max: u32,
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
    names: BTreeMap<ActorId, String>,
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

    fn who(&self, actor: Option<ActorId>, me: ActorId) -> Who {
        match actor {
            Some(id) if id == me => Who::Me,
            Some(id) => Who::Figure(self.figure(id)),
            None => Who::Unseen,
        }
    }

    fn figure(&self, id: ActorId) -> Figure {
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
                Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
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
                Event::AttackStarted { .. } => beats.push(Beat::AttackBegan),
                Event::PreparationPaused => {}
            }
        }
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
                        CombatEventView::Attack {
                            attacker,
                            target,
                            outcome,
                        } => Beat::Blow {
                            attacker: self.who(attacker, a.actor),
                            target: self.who(target, a.actor),
                            outcome,
                        },
                        CombatEventView::Interrupted { .. } => Beat::Interrupted,
                        CombatEventView::Died { actor } => {
                            Beat::Died(self.who(Some(actor), a.actor))
                        }
                    });
                }
            }
            let old = b.combat.as_ref();
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
        let figures = |scene: &Scene| -> BTreeMap<ActorId, String> {
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
        let doors = |o: &Observation| -> BTreeMap<u64, (bool, Position)> {
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

fn item_name(items: &[ItemView], id: u64) -> String {
    items
        .iter()
        .find(|i| i.id == id)
        .filter(|i| !i.name.trim().is_empty())
        .map_or_else(|| "thing".into(), |i| crate::safe(&i.name).to_lowercase())
}

fn door_name(o: &Observation, id: u64) -> String {
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
            "wizard_game":false,"revision":1,"observation":{
            "actor":1,"tick":5,"position":{"x":0,"y":0,"z":0},"ready":true,
            "places":[],"visible_cells":[{"key":"here","position":{"x":0,"y":0,"z":0},
                "wall":false,"material":"stone","place_hint":false,
                "stairs_up":false,"stairs_down":false,
                "door":{"id":7,"name":"Oak Door","description":"","open":false,
                    "reachable":true,"approaches":[]}}],
            "ground_items":[],"inventory":[],
            "visible_actors":[{"id":2,"name":"ruin scout","description":"",
                "position":{"x":2,"y":0,"z":0}}],
            "combat":{"hp":50,"max_hp":50,"preparation_remaining":null,
                "preparation_active":false,"recovery_remaining":0,"actors":[],
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
    fn a_deadly_blow_names_the_dead_after_they_vanish() {
        let before = view();
        let mut after = view();
        after.observation.tick = 6;
        after.observation.visible_actors.clear();
        let combat = after.observation.combat.as_mut().unwrap();
        combat.hp = 44;
        combat.events = vec![
            CombatEventView::Attack {
                attacker: Some(ActorId(1)),
                target: Some(ActorId(2)),
                outcome: AttackOutcome::Hit,
            },
            CombatEventView::Died { actor: ActorId(2) },
        ];
        let scout = Figure {
            id: ActorId(2),
            name: "ruin scout".into(),
        };
        assert_eq!(
            Chronicler::default().observe(&before, &after, None, &Palette::default()),
            [
                Beat::Blow {
                    attacker: Who::Me,
                    target: Who::Figure(scout.clone()),
                    outcome: AttackOutcome::Hit,
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
                    door: 7,
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
