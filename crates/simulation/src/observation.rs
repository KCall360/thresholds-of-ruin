use std::collections::BTreeSet;
use tor_world::{Direction, Location, Position, Region, RegionId};

use crate::{ActorId, Game, GameError, ItemId, ItemLocation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SceneWitness {
    world: tor_world::PerceptionSnapshot,
    location: Location,
    orientation: u8,
    body: tor_world::Shared<crate::BodySpec>,
    disclosed: bool,
    sight: bool,
}

/// Derived actor scenes for observations and streaming pins. Shared across
/// candidate clones, bounded, omitted from saves and ignored by simulation equality.
#[derive(Clone, Debug)]
struct CachedScene {
    witness: SceneWitness,
    cells: Vec<tor_world::SightCell>,
    sight: Vec<tor_world::SightCell>,
    // Lazy: streaming and observations do not pay for an unused supply index.
    locations: Option<BTreeSet<Location>>,
}
impl CachedScene {
    fn contains(&mut self, at: Location) -> bool {
        self.locations
            .get_or_insert_with(|| {
                crate::diagnostics::scene_membership_cells(self.cells.len());
                self.cells.iter().map(|cell| cell.location).collect()
            })
            .contains(&at)
    }
}
type SceneEntries = std::collections::BTreeMap<ActorId, CachedScene>;
#[derive(Clone, Debug, Default)]
pub(crate) struct ActorScenes(std::sync::Arc<std::sync::Mutex<SceneEntries>>);

impl PartialEq for ActorScenes {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ActorScenes {}

/// Manhattan sight range in cells, measured from the eye cell.
pub(crate) const SIGHT_RANGE: u8 = 16;

/// Where the scene places the landing of the abstract stair the actor stands
/// on: straight up or down, just beyond any physically visible offset, so it
/// never collides with a real cell. Travel learns stair links from it.
pub(crate) fn stair_landing_offset(body: &crate::BodySpec, direction: Direction) -> Position {
    let [x, y, z] = body.eye;
    let beyond = i32::from(SIGHT_RANGE) + x.abs() + y.abs() + z.abs() + 1;
    Position {
        x: 0,
        y: 0,
        z: if direction == Direction::Down {
            -beyond
        } else {
            beyond
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyView {
    pub id: crate::content_ids::SupplyId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseAppearance {
    pub actor: ActorId,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub corpse: Option<CorpseAppearance>,
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub known_supply: Option<SupplyView>,
    pub known_currency: Option<crate::CurrencyKind>,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundItemView {
    pub corpse: Option<CorpseAppearance>,
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub known_supply: Option<SupplyView>,
    pub known_currency: Option<crate::CurrencyKind>,
    /// Disclosed physical contact, independent of private holds.
    pub supply_reachable: bool,
    pub location: Location,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorView {
    pub disclosure: crate::perception::ActorDisclosure,
    /// Empty when nothing names it.
    pub name: String,
    pub id: ActorId,
    pub location: Location,
    pub asset: Option<String>,
}

/// Name projection after the caller establishes visible-actor eligibility.
/// Both ordinary perception and paid observations use this observer-aware seam.
/// Empty means unnamed, which clients present using their generic figure label.
pub(crate) fn projected_actor_name<'a>(
    _observer: &crate::Actor,
    subject: &'a crate::Actor,
) -> &'a str {
    let name = subject
        .combat
        .as_ref()
        .map_or("", |combat| combat.identity.name.as_str());
    if name.trim().is_empty() {
        ""
    } else {
        name
    }
}

/// An observable exit, without the unexplored destination's identity or contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitView {
    pub location: Location,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapView {
    pub id: u64,
    pub family: Option<tor_world::TrapFamily>,
    pub reachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellView {
    /// Illumination of this disclosed occurrence, including dark awareness.
    pub lit: bool,
    pub frame: u8,
    pub door: Option<tor_world::Door>,
    pub trap: Option<TrapView>,
    pub door_reachable: bool,
    pub door_secret: bool,
    pub door_approaches: Vec<Location>,
    pub material: &'static str,
    pub location: Location,
    pub wall: bool,
    pub place_hint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionView {
    pub velocity: [i64; 3],
    pub displaced: bool,
    pub impacted: bool,
}

/// Disclosed facts for one actor, separate from the authoritative game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub linked_holds: Vec<crate::linked_hold::LinkedHoldObservation>,
    pub interactions: Option<crate::interactions::InteractionView>,
    pub combat: Option<crate::combat::CombatView>,
    pub motion: Option<MotionView>,
    pub actor: ActorId,
    pub tick: u64,
    pub location: Location,
    pub region: Region,
    /// The observer's own asset, for views of itself from another angle.
    pub asset: Option<String>,
    pub visible_cells: Vec<CellView>,
    pub ground_items: Vec<GroundItemView>,
    pub apparent_objects: Vec<crate::perception::ApparentObjectView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub exits: Vec<ExitView>,
    pub known_places: Vec<KnownPlace>,
}

/// One decision's disclosure plus the original illuminated eye-relative cells.
/// Tactile awareness and projected stair landings never become sight targeting.
pub(crate) struct DecisionSnapshot {
    pub view: Observation,
    pub sight: Vec<tor_world::SightCell>,
}

#[derive(Clone, Copy)]
enum ScenePurpose {
    Decision,
    Ordinary,
}

impl Game {
    pub(crate) fn decision_snapshot(&self, id: ActorId) -> Result<DecisionSnapshot, GameError> {
        let mut sight = Vec::new();
        let disclosed = self.compute_scene_for(
            id,
            ScenePurpose::Decision,
            &mut |cells: &[tor_world::SightCell]| sight.extend_from_slice(cells),
        )?;
        let view = self.observe_in_scene(id, &disclosed, &sight)?;
        self.cache_scene(id, &disclosed, &sight)?;
        Ok(DecisionSnapshot { view, sight })
    }

    /// A free read using bounded exact three-dimensional sight through rotated
    /// portals, with ambient lighting and body-local awareness.
    /// Neither querying nor seeing through a portal counts as visiting a place.
    pub fn observe(&self, id: ActorId) -> Result<Observation, GameError> {
        self.observe_scene(id).map(|(view, _)| view)
    }

    /// Construct disclosure and its exact projected scene together. Callers can
    /// reuse the scene without repeating sight or door-approach queries.
    pub fn observe_scene(
        &self,
        id: ActorId,
    ) -> Result<(Observation, Vec<tor_world::SightCell>), GameError> {
        // Residency planning may already have resolved this exact geometry
        // and actor chart. Entity/readiness disclosure is still rebuilt below.
        let (scene, sight) = self.scene_snapshot(id)?;
        let view = self.observe_in_scene(id, &scene, &sight)?;
        Ok((view, scene))
    }

    fn observe_in_scene(
        &self,
        id: ActorId,
        scene: &[tor_world::SightCell],
        sight: &[tor_world::SightCell],
    ) -> Result<Observation, GameError> {
        crate::diagnostics::observation();
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let cells = scene
            .iter()
            .map(|cell| cell.location)
            .collect::<BTreeSet<_>>();
        let visible = |location: Location| cells.contains(&location);
        let eyes: BTreeSet<_> = sight.iter().map(|cell| cell.location).collect();
        let perceived = self.perception_projection(id, scene, sight)?;
        let door_base = |location: Location| {
            self.disclosed_door(id, location)
                .is_some_and(|door| self.world.door_location(door.id) == Some(location))
        };
        // A location can occur in several frames and at several distances;
        // each (location, frame) pair is disclosed once. Floors and ceilings
        // are ordinary seen solid cells, derived by clients.
        let occurrences: BTreeSet<_> = scene.iter().map(|c| (c.location, c.rotation)).collect();
        let trap_contacts = std::cell::OnceCell::new();
        let mut ground_items = Vec::new();
        let mut inventory = Vec::new();
        let mut item_interactions = Vec::new();
        for (item_id, item) in self.items.perceived(id, &cells) {
            crate::diagnostics::item_view(item.spec.concealed);
            let identified = if item.corpse.is_some() {
                actor.knowledge.contains(&item.spec.identity)
                    || match item.location {
                        ItemLocation::Carried(owner) => owner == id,
                        ItemLocation::Ground(location) => eyes.contains(&location),
                    }
            } else {
                !item.spec.concealed || actor.knowledge.contains(&item.spec.identity)
            };
            let anonymous_corpse = item.corpse.is_some() && !identified;
            let name = if identified {
                &item.spec.name
            } else if anonymous_corpse {
                "corpse"
            } else {
                &item.spec.appearance
            };

            let known_supply = identified
                .then(|| item.spec.supply.as_ref())
                .flatten()
                .and_then(|supply| self.content.rules.supply(supply))
                .map(|supply| SupplyView {
                    id: supply.id.clone(),
                    name: supply.name.clone(),
                });
            match item.location {
                ItemLocation::Ground(location) if visible(location) => {
                    ground_items.push(GroundItemView {
                        corpse: identified.then(|| item.corpse.clone()).flatten(),
                        class: item.spec.class,
                        description: item_description(name),
                        id: item_id,
                        name: name.to_owned(),
                        quantity: item.quantity,
                        appearance: if anonymous_corpse {
                            "corpse".into()
                        } else {
                            item.spec.appearance.clone()
                        },
                        identified,
                        supply_reachable: known_supply.is_some()
                            && self.touches_supply(id, location),
                        known_supply,
                        known_currency: identified.then_some(item.spec.currency).flatten(),
                        location,
                        asset: (!anonymous_corpse)
                            .then(|| item.spec.asset.clone())
                            .flatten(),
                    });
                }
                ItemLocation::Carried(owner) if owner == id => {
                    if item.spec.equipment.is_some()
                        || item.spec.projectile.is_some()
                        || item.spec.consumable.is_some()
                        || item.spec.edible.is_some()
                        || item.spec.readable.is_some()
                        || item.spec.device.is_some()
                    {
                        item_interactions.push(crate::interactions::ItemInteractionView {
                            item: item_id,
                            slot: item
                                .spec
                                .equipment
                                .as_ref()
                                .map(|equipment| {
                                    self.content
                                        .rules
                                        .equipment_slot(&equipment.slot)
                                        .map(crate::EquipmentSlotView::from)
                                        .ok_or(GameError::Rule(
                                            crate::abilities::RuleError::InvalidDefinition,
                                        ))
                                })
                                .transpose()?,
                            equipped_slot: actor
                                .equipment
                                .iter()
                                .find_map(|(slot, item)| (*item == item_id).then_some(*slot)),
                            known_equipment: self.known_equipment_view(id, item_id),
                            known_projectile_bonus: self.known_projectile_bonus_view(id, item_id),
                            known_cursed: actor.known_equipment.curse(item_id),
                            known_protected: actor.known_equipment.protection(item_id),
                            drinkable: item.spec.consumable.is_some(),
                            readable: item.spec.readable.is_some(),
                            zappable: item.spec.device.is_some(),
                            charges: actor.known_charges.observed(item_id, item.charges),
                            edible: item.spec.edible.is_some(),
                        });
                    }
                    inventory.push(ItemView {
                        corpse: identified.then(|| item.corpse.clone()).flatten(),
                        class: item.spec.class,
                        description: item_description(name),
                        id: item_id,
                        name: name.to_owned(),
                        quantity: item.quantity,
                        appearance: if anonymous_corpse {
                            "corpse".into()
                        } else {
                            item.spec.appearance.clone()
                        },
                        identified,
                        known_supply,
                        known_currency: identified.then_some(item.spec.currency).flatten(),
                        asset: (!anonymous_corpse)
                            .then(|| item.spec.asset.clone())
                            .flatten(),
                    });
                }
                _ => {}
            }
        }
        let completed: Vec<_> = self
            .event_log
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                crate::combat::CombatEvent::ItemCompleted { actor, work, .. }
                    if *actor == id && self.owns_event_receipt(id, index) =>
                {
                    Some(work.clone())
                }
                _ => None,
            })
            .collect();
        let curse_blocked: Vec<_> = self
            .event_log
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                crate::combat::CombatEvent::ItemCurseBlocked { actor, item, .. }
                    if *actor == id && self.owns_event_receipt(id, index) =>
                {
                    Some(*item)
                }
                _ => None,
            })
            .collect();
        let slots = actor
            .anatomy
            .slots
            .iter()
            .map(|id| {
                self.content
                    .rules
                    .equipment_slot(id)
                    .map(crate::EquipmentSlotView::from)
                    .ok_or(GameError::Rule(
                        crate::abilities::RuleError::InvalidDefinition,
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Observation {
            linked_holds: self.linked_holds(id),
            interactions: (!actor.anatomy.slots.is_empty()
                || actor.pending.is_some()
                || actor.identification.is_some()
                || !item_interactions.is_empty()
                || !completed.is_empty()
                || !curse_blocked.is_empty())
            .then(|| crate::interactions::InteractionView {
                identification: self.identification_view(id),
                completed,
                curse_blocked,
                slots,
                preparation: actor.pending.as_ref().map(|progress| {
                    crate::interactions::PreparationView {
                        work: progress.plan.work(),
                        remaining: if progress.active {
                            progress.remaining.saturating_sub(
                                self.actor_clock(id).saturating_sub(progress.started),
                            )
                        } else {
                            progress.remaining
                        },
                        active: progress.active,
                    }
                }),
                inventory: item_interactions,
            }),
            combat: self.combat_view(id, &perceived),
            motion: self.motion_view_active(id).then_some(MotionView {
                velocity: actor.motion.velocity,
                displaced: self.physics.displaced.contains(&id),
                impacted: self
                    .physics
                    .impacts
                    .iter()
                    .any(|e| e.entity == crate::PhysicsEntity::Actor(id)),
            }),
            actor: id,
            tick: self.tick,
            location: actor.location,
            asset: actor.asset.clone(),
            region: self
                .world
                .region(actor.location.region)
                .ok_or(GameError::InvalidLocation)?
                .clone(),
            ground_items,
            apparent_objects: perceived
                .iter()
                .filter_map(|(subject, perceived)| {
                    if perceived.disclosure != crate::perception::ActorDisclosure::ApparentObject
                        || !self.actors[subject].alive()
                    {
                        return None;
                    }
                    let state = &self.actors[subject];
                    visible(state.location).then(|| crate::perception::ApparentObjectView {
                        subject: *subject,
                        location: state.location,
                        appearance: state.guise.appearance().expect("classified guise").clone(),
                    })
                })
                .collect(),
            visible_cells: occurrences
                .iter()
                .map(|&(location, frame)| CellView {
                    lit: self.world.is_lit(location) == Some(true),
                    frame,
                    door: self.disclosed_door(id, location),
                    door_secret: self.disclosed_door(id, location).is_some()
                        && self.world.secret_door(location).is_some(),
                    trap: self
                        .world
                        .trap(location)
                        .filter(|trap| {
                            self.feature_known(id, tor_world::ConcealedFeatureId::Trap(trap.id))
                        })
                        .map(|trap| TrapView {
                            id: trap.id,
                            family: self
                                .feature_family_known(
                                    id,
                                    tor_world::ConcealedFeatureId::Trap(trap.id),
                                )
                                .then(|| trap.kind.family()),
                            reachable: trap_contacts
                                .get_or_init(|| self.trap_contacts_in_scene(id, scene))
                                .contains(&location),
                        }),
                    // A tall door is one door: only its base cell is offered
                    // for interaction, so it isn't reachable twice.
                    door_reachable: door_base(location)
                        && self.door_reachable_from(actor.location, location),
                    door_approaches: if door_base(location) {
                        self.disclosed_door_approaches(scene, location)
                    } else {
                        vec![]
                    },
                    material: match self.world.terrain(location) {
                        _ if self.concealed_door_wall(id, location) => "stone",
                        Some(tor_world::Terrain::Solid(material)) => material.name(),
                        _ if self.world.is_chamber(location.region) => "",
                        _ => "stone", // Cosmetic fallback for raw diagnostic regions.
                    },
                    location,
                    wall: self.world.is_wall(location) || self.concealed_door_wall(id, location),
                    place_hint: self.world.has_place_hint(location),
                })
                .collect(),
            inventory,
            visible_actors: perceived
                .iter()
                .filter(|(other, perceived)| {
                    self.actors[other].alive() && perceived.disclosure.reference().is_some()
                })
                .flat_map(|(other_id, perceived)| {
                    let other_id = *other_id;
                    let other = &self.actors[&other_id];
                    let disclosure = perceived.disclosure;
                    perceived
                        .body
                        .iter()
                        .copied()
                        .filter(move |(at, _)| {
                            visible(*at) && (other_id != id || *at != actor.location)
                        })
                        .map(move |(location, _)| ActorView {
                            disclosure,
                            name: if disclosure == crate::perception::ActorDisclosure::Creature {
                                projected_actor_name(actor, other).to_owned()
                            } else {
                                String::new()
                            },
                            id: other_id,
                            location,
                            asset: (disclosure == crate::perception::ActorDisclosure::Creature)
                                .then(|| other.asset.clone())
                                .flatten(),
                        })
                })
                .collect(),
            exits: cells
                .iter()
                .flat_map(|&location| {
                    [
                        Direction::North,
                        Direction::East,
                        Direction::South,
                        Direction::West,
                        Direction::Up,
                        Direction::Down,
                    ]
                    .into_iter()
                    .filter(move |direction| {
                        self.world.passage(location, *direction).is_some()
                            || self.world.is_stair(location, *direction)
                            || self.is_terminal_exit(id, location, *direction)
                    })
                    .map(move |direction| ExitView {
                        location,
                        direction,
                    })
                })
                .filter(|exit| {
                    !self.world.is_wall(exit.location)
                        && (!matches!(exit.direction, Direction::Up | Direction::Down)
                            || self.world.is_stair(exit.location, exit.direction)
                            || self.is_terminal_exit(id, exit.location, exit.direction))
                })
                .collect(),
            known_places: actor
                .visited
                .iter()
                .map(|&id| KnownPlace {
                    id,
                    name: self
                        .world
                        .known_region(id)
                        .expect("visited region is known")
                        .name
                        .clone(),
                })
                .collect(),
        })
    }

    /// Closed undiscovered entrances disclose the same solid appearance as the wall.
    /// Opening remains one shared physical world change for every observer.
    fn concealed_door_wall(&self, actor: ActorId, location: Location) -> bool {
        self.world.secret_door(location).is_some()
            && self.world.door(location).is_some_and(|door| {
                !door.open
                    && !self.feature_known(actor, tor_world::ConcealedFeatureId::Door(door.id))
            })
    }

    pub(crate) fn disclosed_door(
        &self,
        actor: ActorId,
        location: Location,
    ) -> Option<tor_world::Door> {
        (!self.concealed_door_wall(actor, location))
            .then(|| self.world.door(location))
            .flatten()
    }

    fn trap_contacts_in_scene(
        &self,
        actor: ActorId,
        scene: &[tor_world::SightCell],
    ) -> BTreeSet<Location> {
        let mut contacts = self.melee_contact_locations_in_scene(actor, scene, None);
        if let Some(owner) = self.actors.get(&actor) {
            if let Some(body) = self.body_cells(owner.location, owner.orientation, &owner.body) {
                contacts.extend(body.into_iter().map(|(location, _)| location));
            }
        }
        contacts
    }

    pub(crate) fn trap_reachable(&self, actor: ActorId, location: Location) -> bool {
        self.scene_snapshot(actor).is_ok_and(|(scene, _)| {
            scene.iter().any(|cell| cell.location == location)
                && self
                    .trap_contacts_in_scene(actor, &scene)
                    .contains(&location)
        })
    }

    fn disclosed_door_approaches(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
    ) -> Vec<Location> {
        self.door_approaches_among(scene, door, true)
    }

    /// With `filtered`, only cells beside an occurrence of the door are
    /// considered. Tests compare that against considering every cell.
    fn door_approaches_among(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
        filtered: bool,
    ) -> Vec<Location> {
        let mut approaches = BTreeSet::new();
        // An approach needs the door seen one horizontal step away, so only
        // cells beside one of its occurrences can qualify. Checking that first
        // skips only candidates the final test below would reject.
        let doors: Vec<Position> = scene
            .iter()
            .filter(|cell| cell.location == door)
            .map(|cell| cell.offset)
            .collect();
        for from in scene {
            let beside = |x: i32, y: i32| {
                let target = Position {
                    x: from.offset.x + x,
                    y: from.offset.y + y,
                    z: from.offset.z,
                };
                doors.contains(&target)
            };
            if filtered && !(-1..=1).any(|x| (-1..=1).any(|y| (x, y) != (0, 0) && beside(x, y)))
                || !self.world.walkable(from.location)
            {
                continue;
            }
            for direction in Direction::HORIZONTAL {
                let (dx, dy, _) = direction.delta();
                if filtered && !beside(dx, dy) {
                    continue;
                }
                let local = direction.rotated(from.rotation);
                let reach = if let Some((a, b)) = direction.components() {
                    self.world.diagonal_reach(from.location, local, |side| {
                        !self.occupied(side)
                            && [a, b].into_iter().any(|first| {
                                let Some((next, turns)) = self
                                    .world
                                    .movement_neighbor(from.location, first.rotated(from.rotation))
                                else {
                                    return false;
                                };
                                let (sx, sy, _) = first.delta();
                                next == side
                                    && scene.iter().any(|seen| {
                                        seen.location == side
                                            && seen.rotation
                                                == tor_world::compose_rotation(from.rotation, turns)
                                            && seen.offset
                                                == Position {
                                                    x: from.offset.x + sx,
                                                    y: from.offset.y + sy,
                                                    z: from.offset.z,
                                                }
                                    })
                            })
                    })
                } else {
                    self.reach(from.location, local)
                };
                let Some((to, turns)) = reach else {
                    continue;
                };
                if to != door {
                    continue;
                }
                let rotation = tor_world::compose_rotation(from.rotation, turns);
                if scene.iter().any(|target| {
                    target.location == door
                        && target.rotation == rotation
                        && target.offset
                            == Position {
                                x: from.offset.x + dx,
                                y: from.offset.y + dy,
                                z: from.offset.z,
                            }
                }) {
                    approaches.insert(from.location);
                }
            }
        }
        approaches.into_iter().collect()
    }

    /// Backend-resolved view occurrences. A location may be seen at several
    /// offsets. Sight starts at the centre of the body's eye cell; offsets are
    /// relative to the actor's reference cell in its body frame.
    pub fn scene(&self, id: ActorId) -> Result<Vec<tor_world::SightCell>, GameError> {
        let mut sight = Vec::new();
        let scene = self.compute_scene(id, &mut |cells: &[tor_world::SightCell]| {
            sight.extend_from_slice(cells)
        })?;
        self.cache_scene(id, &scene, &sight)?;
        Ok(scene)
    }

    fn cache_scene(
        &self,
        id: ActorId,
        scene: &[tor_world::SightCell],
        sight: &[tor_world::SightCell],
    ) -> Result<(), GameError> {
        let witness = self.scene_witness(id)?;
        crate::diagnostics::scene_cells_copied(scene.len());
        self.insert_scene(
            id,
            CachedScene {
                witness,
                cells: scene.to_vec(),
                sight: sight.to_vec(),
                locations: None,
            },
        );
        Ok(())
    }

    fn insert_scene(&self, id: ActorId, scene: CachedScene) {
        let mut cache = self.lifecycle.scenes.0.lock().expect("actor scene cache");
        if cache.len() >= 512 && !cache.contains_key(&id) {
            cache.pop_first();
        }
        cache.insert(id, scene);
    }

    pub(crate) fn scene_witness(&self, id: ActorId) -> Result<SceneWitness, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        Ok(SceneWitness {
            world: self.world.perception_snapshot(),
            location: actor.location,
            orientation: actor.orientation,
            body: actor.body.clone(),
            disclosed: self.alive(id) || self.combat.selected == Some(id),
            sight: actor.perception().sight,
        })
    }

    /// Geometry and exact eye sight share one cache witness and one computation.
    pub(crate) fn scene_snapshot(
        &self,
        id: ActorId,
    ) -> Result<(Vec<tor_world::SightCell>, Vec<tor_world::SightCell>), GameError> {
        let witness = self.scene_witness(id)?;
        if let Some(cached) = self
            .lifecycle
            .scenes
            .0
            .lock()
            .expect("actor scene cache")
            .get(&id)
        {
            if cached.witness == witness {
                return Ok((cached.cells.clone(), cached.sight.clone()));
            }
        }
        let mut sight = Vec::new();
        let scene = self.compute_scene(id, &mut |cells: &[tor_world::SightCell]| {
            sight.extend_from_slice(cells)
        })?;
        self.cache_scene(id, &scene, &sight)?;
        Ok((scene, sight))
    }

    /// Streaming consumes the same result as observations, without requesting
    /// another actor scene when all perception inputs remain unchanged.
    pub(crate) fn streaming_scene(
        &self,
        id: ActorId,
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        let witness = self.scene_witness(id)?;
        if let Some(cached) = self
            .lifecycle
            .scenes
            .0
            .lock()
            .expect("actor scene cache")
            .get(&id)
        {
            if cached.witness == witness {
                crate::diagnostics::scene_cells_copied(cached.cells.len());
                return Ok(cached.cells.clone());
            }
        }
        self.scene(id)
    }

    /// Ordinary disclosed-location membership. Physical contact is a separate gate.
    pub(crate) fn scene_contains(&self, id: ActorId, at: Location) -> Result<bool, GameError> {
        crate::diagnostics::scene_membership_lookup();
        let witness = self.scene_witness(id)?;
        if let Some(cached) = self
            .lifecycle
            .scenes
            .0
            .lock()
            .expect("actor scene cache")
            .get_mut(&id)
        {
            if cached.witness == witness {
                return Ok(cached.contains(at));
            }
        }
        // Do not copy the projected array merely to ask a membership question.
        // A rejected candidate may have populated this shared cache with a
        // different witness; replace that derived entry from this Game's inputs.
        let mut sight = Vec::new();
        let cells = self.compute_scene(id, &mut |cells: &[tor_world::SightCell]| {
            sight.extend_from_slice(cells)
        })?;
        let mut cached = CachedScene {
            witness,
            cells,
            sight,
            locations: None,
        };
        let contains = cached.contains(at);
        self.insert_scene(id, cached);
        Ok(contains)
    }

    fn compute_scene(
        &self,
        id: ActorId,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        self.compute_scene_for(id, ScenePurpose::Ordinary, sight_observer)
    }

    fn compute_scene_for(
        &self,
        id: ActorId,
        purpose: ScenePurpose,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        self.compute_scene_at(
            id,
            actor.location,
            actor.orientation,
            purpose,
            sight_observer,
        )
    }

    pub(crate) fn scene_snapshot_at_placement(
        &self,
        id: ActorId,
        placement: crate::abilities::Placement,
    ) -> Result<(Vec<tor_world::SightCell>, Vec<tor_world::SightCell>), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if actor.location == placement.location && actor.orientation == placement.orientation {
            return self.scene_snapshot(id);
        }
        let mut sight = Vec::new();
        let scene = self.compute_scene_at(
            id,
            placement.location,
            placement.orientation,
            ScenePurpose::Ordinary,
            &mut |cells: &[tor_world::SightCell]| sight.extend_from_slice(cells),
        )?;
        Ok((scene, sight))
    }
    fn compute_scene_at(
        &self,
        id: ActorId,
        location: Location,
        orientation: u8,
        purpose: ScenePurpose,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        if !self.alive(id) && self.combat.selected != Some(id) && self.actors.contains_key(&id) {
            return Ok(Vec::new());
        }
        crate::diagnostics::scene();
        match purpose {
            ScenePurpose::Decision => crate::diagnostics::decision_scene(),
            ScenePurpose::Ordinary => crate::diagnostics::ordinary_scene(),
        }
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let body = &actor.body;
        let Some((eye, eye_frame)) = self.eye_at(location, orientation, body) else {
            return Ok(Vec::new());
        };
        // The body frame is carried across any portal inside the body, so
        // eye-frame offsets are body-frame offsets from the eye cell.
        let [ex, ey, ez] = body.eye;
        let mut scene = if actor.perception().sight {
            self.world
                .illuminated_eye_scene(eye, eye_frame, SIGHT_RANGE)
        } else {
            Vec::new()
        };
        sight_observer(&scene);
        for cell in &mut scene {
            cell.offset.x += ex;
            cell.offset.y += ey;
            cell.offset.z += ez;
        }
        let mut scene =
            merge_perception(self.local_awareness_at(location, orientation, body), scene);
        // Abstract stair links are traversal, not geometry: standing on one
        // discloses its landing as a separate occurrence beyond physical sight.
        for direction in [Direction::Up, Direction::Down] {
            if self.world.is_stair(location, direction) {
                let Some(to) = self.world.link_destination(location, direction) else {
                    continue;
                };
                if !self.world.contains(to) {
                    continue;
                }
                scene.push(tor_world::SightCell {
                    physical: false,
                    location: to,
                    rotation: tor_world::compose_rotation(
                        orientation,
                        self.world.crossing_rotation(location, direction),
                    ),
                    offset: stair_landing_offset(body, direction),
                    wall: self.world.is_wall(to),
                });
            }
        }
        scene.sort_by_key(|c| c.offset);
        Ok(scene)
    }

    pub(crate) fn local_awareness(&self, id: ActorId) -> Vec<tor_world::SightCell> {
        let Some(actor) = self.actors.get(&id) else {
            return Vec::new();
        };
        self.local_awareness_at(actor.location, actor.orientation, &actor.body)
    }

    pub(crate) fn local_awareness_at(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
    ) -> Vec<tor_world::SightCell> {
        let mut scene = Vec::new();
        if let Some(occupied) = self.body_cells(location, orientation, body) {
            let mut occupied: Vec<_> = body.cells.iter().copied().zip(occupied).collect();
            occupied.sort_by_key(|(offset, _)| *offset);
            // Authored cell order must not let a neighbour projection replace
            // another occupied cell at a join with different reverse geometry.
            for &(offset, (location, frame)) in &occupied {
                scene.push(tor_world::SightCell {
                    physical: true,
                    location,
                    rotation: frame,
                    offset: Position {
                        x: offset[0],
                        y: offset[1],
                        z: offset[2],
                    },
                    wall: self.world.is_wall(location),
                });
            }
            for (offset, (location, frame)) in occupied {
                for mut cell in self.world.neighborhood_scene(location, frame) {
                    cell.offset.x += offset[0];
                    cell.offset.y += offset[1];
                    cell.offset.z += offset[2];
                    scene.push(cell);
                }
            }
        }
        scene
    }

    /// The loaded actor's eye cell and frame, if its body resolves.
    pub(crate) fn eye(&self, id: ActorId) -> Option<(Location, u8)> {
        let actor = self.actors.get(&id)?;
        self.eye_at(actor.location, actor.orientation, &actor.body)
    }

    fn eye_at(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
    ) -> Option<(Location, u8)> {
        let eye_index = body.cells.iter().position(|cell| *cell == body.eye)?;
        self.body_cells(location, orientation, body)
            .map(|cells| cells[eye_index])
    }
}

/// Local awareness takes precedence over eye projection in the body chart.
/// Stable ordering keeps occupied cells (first in local awareness) authoritative.
pub(crate) fn merge_perception(
    mut local: Vec<tor_world::SightCell>,
    distant: Vec<tor_world::SightCell>,
) -> Vec<tor_world::SightCell> {
    local.extend(distant);
    local.sort_by_key(|cell| cell.offset);
    local.dedup_by_key(|cell| cell.offset);
    local
}

/// Initial authored appearance catalog. These cosmetic stubs do not add item rules.
/// Empty when nothing is authored; clients decide how to say so.
fn item_description(name: &str) -> String {
    match name {
        "copper token" => "A small copper disc, stamped with a worn spiral.",
        "silver token" => "A small silver disc, stamped with a worn spiral.",
        "iron token" => "A small iron disc, stamped with a worn spiral.",
        "stone tablet" => "A weathered slab of stone. Shallow marks run across its surface.",
        _ => "",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Extent, Passage, World};

    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }

    fn membership_fixture() -> (Game, ActorId) {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        (game, actor)
    }

    #[test]
    fn scene_build_counters_cover_cold_snapshots_placements_and_decisions() {
        let (game, actor) = membership_fixture();
        let before = crate::diagnostics::work_counts();
        game.scene_snapshot(actor).unwrap();
        let cold = crate::diagnostics::work_counts();
        assert_eq!(cold.scenes - before.scenes, 1);
        assert_eq!(cold.ordinary_scenes - before.ordinary_scenes, 1);
        assert_eq!(cold.decision_scenes, before.decision_scenes);
        game.scene_snapshot(actor).unwrap();
        let warm = crate::diagnostics::work_counts();
        assert_eq!(warm.scenes, cold.scenes);
        assert_eq!(warm.ordinary_scenes, cold.ordinary_scenes);
        game.scene_snapshot_at_placement(
            actor,
            crate::abilities::Placement {
                location: at(1, 1, 0),
                orientation: 0,
            },
        )
        .unwrap();
        let placed = crate::diagnostics::work_counts();
        assert_eq!(placed.scenes - warm.scenes, 1);
        assert_eq!(placed.ordinary_scenes - warm.ordinary_scenes, 1);
        game.decision_snapshot(actor).unwrap();
        let decided = crate::diagnostics::work_counts();
        assert_eq!(decided.scenes - placed.scenes, 1);
        assert_eq!(decided.decision_scenes - placed.decision_scenes, 1);
        assert_eq!(decided.ordinary_scenes, placed.ordinary_scenes);
        assert_eq!(
            decided.scenes - before.scenes,
            decided.ordinary_scenes - before.ordinary_scenes + decided.decision_scenes
                - before.decision_scenes
        );
    }

    #[test]
    fn phase5_scene_membership_warm_ground_fragments_do_not_copy_or_scan_the_scene() {
        let (mut game, actor) = membership_fixture();
        for id in 10..42 {
            game.place_item_stack(
                id,
                at(1, 1, 0),
                None,
                1,
                crate::ItemSpec::ordinary(format!("fragment {id}")),
            )
            .unwrap();
        }
        assert!(game.scene_contains(actor, at(1, 1, 0)).unwrap());
        let before = crate::diagnostics::work_counts();
        for id in 10..42 {
            assert!(game.supply_access(
                actor,
                &game.items[&ItemId(id)],
                &crate::rules::SupplyAccess::CarriedOrGroundTouch
            ));
        }
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.scene_membership_lookups - before.scene_membership_lookups,
            32
        );
        assert_eq!(after.scenes, before.scenes);
        assert_eq!(
            after.scene_cells_copied, before.scene_cells_copied,
            "warm fragment checks must not clone the disclosed scene"
        );
        assert_eq!(
            after.scene_membership_cells, before.scene_membership_cells,
            "warm fragment checks must not scan the disclosed scene"
        );
    }

    #[test]
    fn phase5_scene_membership_cold_index_is_built_once_and_is_read_only() {
        let (game, actor) = membership_fixture();
        let expected = game.compute_scene(actor, &mut |_| {}).unwrap();
        game.lifecycle.scenes.0.lock().unwrap().clear();
        let original = game.clone();
        let before = crate::diagnostics::work_counts();
        assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
        let cold = crate::diagnostics::work_counts();
        assert_eq!(cold.scenes - before.scenes, 1);
        assert_eq!(
            cold.scene_membership_cells - before.scene_membership_cells,
            expected.len()
        );
        for _ in 0..32 {
            assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
        }
        let warm = crate::diagnostics::work_counts();
        assert_eq!(warm.scenes, cold.scenes);
        assert_eq!(warm.scene_membership_cells, cold.scene_membership_cells);
        assert_eq!(warm.scene_cells_copied, cold.scene_cells_copied);
        assert_eq!(game, original);
    }

    fn check_membership(game: &Game, actor: ActorId) {
        let scene = game.compute_scene(actor, &mut |_| {}).unwrap();
        for location in
            scene
                .iter()
                .map(|cell| cell.location)
                .chain([at(1, 2, 0), at(2, 0, 1), at(99, 0, 0)])
        {
            assert_eq!(
                game.scene_contains(actor, location).unwrap(),
                scene.iter().any(|cell| cell.location == location)
            );
        }
    }

    #[test]
    fn phase5_scene_membership_tracks_witness_changes_and_candidate_rollback() {
        let (mut game, actor) = membership_fixture();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        check_membership(&game, actor);
        let mut candidate = game.clone();
        candidate.set_cell_light(at(1, 2, 0), false).unwrap();
        check_membership(&candidate, actor);
        check_membership(&game, actor);
        candidate.set_wall(at(1, 1, 0), true).unwrap();
        check_membership(&candidate, actor);
        candidate.set_wall(at(1, 1, 0), false).unwrap();
        candidate.teleport(actor, at(2, 0, 1)).unwrap();
        check_membership(&candidate, actor);
        candidate.actors.get_mut(&actor).unwrap().orientation = 1;
        check_membership(&candidate, actor);
        candidate.actors.get_mut(&actor).unwrap().orientation = 0;
        candidate.teleport(actor, at(1, 0, 0)).unwrap();
        candidate
            .set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0]],
                    eye: [1, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
        check_membership(&candidate, actor);
        candidate
            .apply_damage(
                actor,
                &crate::damage::DamageSpec::new(
                    vec![crate::damage::DamageComponent::fixed(
                        crate::combat::DamageType::Impact,
                        None,
                        1000,
                    )],
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(!candidate.alive(actor));
        check_membership(&candidate, actor);
        candidate.combat.selected = Some(actor);
        check_membership(&candidate, actor);
        candidate.combat.selected = None;
        check_membership(&candidate, actor);
        check_membership(&game, actor);
        // Eviction cannot turn the derived index into authority.
        game.lifecycle.scenes.0.lock().unwrap().clear();
        check_membership(&game, actor);
    }

    #[test]
    fn phase5_scene_membership_deduplicates_occurrences_without_granting_stair_touch() {
        let (mut game, actor) = membership_fixture();
        let landing = at(2, 0, 1);
        game.register_named_anchors(
            RegionId(2),
            std::collections::BTreeMap::from([("landing".into(), landing.position)]),
        )
        .unwrap();
        game.connect_named_stair(
            at(1, 0, 0),
            Direction::Up,
            tor_world::NamedAnchor {
                region: RegionId(2),
                name: "landing".into(),
            },
        )
        .unwrap();
        let scene = game.scene(actor).unwrap();
        assert!(scene.iter().any(|cell| cell.location == landing));
        assert!(game.scene_contains(actor, landing).unwrap());
        assert!(!game.touches_supply(actor, landing));
        let item = game
            .place_item_stack(
                10,
                landing,
                None,
                1,
                crate::ItemSpec::ordinary("stair supply".into()),
            )
            .unwrap();
        assert!(!game.supply_access(
            actor,
            &game.items[&item],
            &crate::rules::SupplyAccess::CarriedOrGroundTouch
        ));
        // Cache membership depends on physical locations, never occurrence offsets.
        // Explicitly exercise duplicate projected occurrences at distinct offsets.
        let mut repeated = scene.clone();
        let mut duplicate = repeated[0];
        duplicate.offset.x += 100;
        repeated.push(duplicate);
        game.cache_scene(actor, &repeated, &[]).unwrap();
        for cell in &repeated {
            assert!(game.scene_contains(actor, cell.location).unwrap());
        }
        assert_eq!(game.streaming_scene(actor).unwrap(), repeated);
    }

    #[test]
    fn phase5_scene_membership_index_respects_cache_capacity_and_eviction() {
        let (game, actor) = membership_fixture();
        assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
        let cached = game.lifecycle.scenes.0.lock().unwrap()[&actor].clone();
        assert!(cached.locations.is_some());
        // Fill only this derived cache to its bound; no actors or claims are
        // synthesized in authoritative state.
        {
            let mut cache = game.lifecycle.scenes.0.lock().unwrap();
            cache.clear();
            for id in 2..=513 {
                cache.insert(ActorId(id), cached.clone());
            }
        }
        let original = game.clone();
        let before = crate::diagnostics::work_counts();
        assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
        assert_eq!(crate::diagnostics::work_counts().scenes, before.scenes + 1);
        {
            let cache = game.lifecycle.scenes.0.lock().unwrap();
            assert_eq!(cache.len(), 512);
            assert!(cache.contains_key(&actor));
            assert!(!cache.contains_key(&ActorId(2)));
        }
        check_membership(&game, actor);
        assert_eq!(game, original);
    }

    #[test]
    fn observation_reuses_the_exact_scene_already_resolved_for_streaming() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let scene = game.streaming_scene(actor).unwrap();
        let before = crate::diagnostics::work_counts();
        assert_eq!(game.observe_scene(actor).unwrap().1, scene);
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.scenes, before.scenes,
            "unchanged perception was projected twice"
        );
        assert_eq!(after.observations, before.observations + 1);
        let mut changed = game.clone();
        changed.set_cell_light(at(1, 2, 0), false).unwrap();
        let dark = changed.observe_scene(actor).unwrap().1;
        assert_ne!(dark, scene);
        assert_eq!(dark, changed.compute_scene(actor, &mut |_| {}).unwrap());
        assert_eq!(game.observe_scene(actor).unwrap().1, scene);
    }

    #[test]
    fn observation_scene_cache_tracks_actor_chart_body_and_death_disclosure() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        let check = |game: &Game| {
            assert_eq!(
                game.observe_scene(actor).unwrap().1,
                game.compute_scene(actor, &mut |_| {}).unwrap()
            );
        };
        check(&game);
        game.teleport(actor, at(2, 0, 1)).unwrap();
        check(&game);
        game.actors.get_mut(&actor).unwrap().orientation = 1;
        check(&game);
        game.actors.get_mut(&actor).unwrap().orientation = 0;
        game.teleport(actor, at(1, 0, 0)).unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [1, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        check(&game);
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Impact,
                    None,
                    1000,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!game.alive(actor));
        assert!(game.observe_scene(actor).unwrap().1.is_empty());
        check(&game);
        game.combat.selected = Some(actor);
        assert!(!game.observe_scene(actor).unwrap().1.is_empty());
        check(&game);
        game.combat.selected = None;
        check(&game);
    }

    #[test]
    fn streaming_reuses_only_scenes_with_identical_perception_inputs() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let initial = game.scene(actor).unwrap();
        let calls = crate::diagnostics::work_counts().scenes;
        assert_eq!(game.streaming_scene(actor).unwrap(), initial);
        assert_eq!(crate::diagnostics::work_counts().scenes, calls);
        let mut candidate = game.clone();
        let target = at(1, 2, 0);
        assert!(initial.iter().any(|cell| cell.location == target));
        candidate.set_cell_light(target, false).unwrap();
        let dark = candidate.streaming_scene(actor).unwrap();
        assert!(!dark.iter().any(|cell| cell.location == target));
        assert_eq!(crate::diagnostics::work_counts().scenes, calls + 1);
        assert_eq!(game.streaming_scene(actor).unwrap(), initial);
        candidate.set_cell_light(target, true).unwrap();
        candidate.set_wall(at(1, 1, 0), true).unwrap();
        assert!(!candidate
            .streaming_scene(actor)
            .unwrap()
            .iter()
            .any(|cell| cell.location == target));
        candidate.set_wall(at(1, 1, 0), false).unwrap();
        candidate
            .set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0]],
                    eye: [0, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
        candidate.set_cell_light(target, false).unwrap();
        // Extending the body puts the dark target in its immediate neighbourhood.
        assert!(candidate
            .streaming_scene(actor)
            .unwrap()
            .iter()
            .any(|cell| cell.location == target));
        assert_eq!(
            candidate.streaming_scene(actor).unwrap(),
            candidate.compute_scene(actor, &mut |_| {}).unwrap()
        );
    }

    #[test]
    fn door_approaches_ignore_only_cells_away_from_the_door() {
        let (mut compared, mut found) = (0, 0);
        for seed in 1..=24u64 {
            let mut state = 0x9E37_79B9_7F4A_7C15 ^ seed;
            let mut below = |n: u64| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state % n
            };
            let rooms = (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: String::new(),
                    bounds: Extent::new(7, 7, 1).unwrap(),
                })
                .collect();
            let mut world = World::new(rooms, vec![]).unwrap();
            // A straight join and a rotated one, as in the latency fixture.
            for (from, direction, to, back, turns) in [
                (
                    at(1, 6, 3),
                    Direction::East,
                    at(2, 0, 3),
                    Direction::West,
                    0,
                ),
                (
                    at(1, 3, 0),
                    Direction::North,
                    at(2, 0, 5),
                    Direction::West,
                    1,
                ),
            ] {
                world
                    .connect(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        turns,
                    )
                    .unwrap();
                world
                    .connect(
                        Passage {
                            from: to,
                            direction: back,
                            to: from,
                        },
                        (4 - turns) % 4,
                    )
                    .unwrap();
            }
            let cell = |below: &mut dyn FnMut(u64) -> u64| {
                at(1 + below(2), below(7) as i32, below(7) as i32)
            };
            for _ in 0..12 {
                let _ = world.set_wall(cell(&mut below), true);
            }
            let mut game = Game::new(world, seed, crate::test_creatures::content());
            for _ in 0..8 {
                let _ = game.place_door(cell(&mut below), below(2) == 0, 1);
            }
            for _ in 0..4 {
                let _ = game.spawn_actor(cell(&mut below), NonZeroU64::new(100).unwrap());
            }
            for _ in 0..16 {
                let eye = cell(&mut below);
                let scene = game.world.eye_scene(eye, below(4) as u8, SIGHT_RANGE);
                let doors: BTreeSet<_> = scene
                    .iter()
                    .map(|c| c.location)
                    .filter(|&at| game.world.door(at).is_some())
                    .collect();
                for door in doors {
                    let approaches = game.door_approaches_among(&scene, door, true);
                    assert_eq!(
                        approaches,
                        game.door_approaches_among(&scene, door, false),
                        "seed {seed}: eye {eye:?}, door {door:?}"
                    );
                    compared += 1;
                    found += approaches.len();
                }
            }
        }
        assert!(
            compared > 200 && found > 200,
            "{compared} doors, {found} approaches"
        );
    }
}
