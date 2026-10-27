//! A renderer-independent projection of disclosed client state.
use crate::{
    actor_model, creature_model, item_model, object_model, wearing, CreatureModel, ItemModel,
    Wearing,
};
use tor_client_common::{ClientState, Palette};
use tor_protocol::*;

/// One creature in view.
#[derive(Clone)]
pub struct Actor {
    pub appearance_seed: Option<AppearanceSeed>,
    pub model: CreatureModel,
    /// Disclosed visible equipment; empty when none is disclosed.
    pub wearing: Wearing,
    /// The same equipment as names, for describing the creature.
    pub attire: tor_client_common::narration::Attire,
    pub id: ActorTarget,
    pub name: String,
    pub description: String,
    pub position: Position,
    pub hostile: bool,
    pub injury: Option<Injury>,
    pub is_self: bool,
}

/// Everything the window draws, in the observer frame of this moment.
#[derive(Clone)]
pub struct View {
    pub shown: ReplyContext,
    pub context: StreamContext,
    pub branch: BranchId,
    pub feet: Position,
    pub visible: Vec<CellView>,
    /// Palette-resolved sampled cell art; never used for navigation or collision.
    pub object_models: std::collections::BTreeMap<Position, ItemModel>,
    /// Remembered cells aligned to this frame that are not currently seen.
    pub memory: Vec<RememberedMapCell>,
    pub actors: Vec<Actor>,
    pub items: Vec<GroundItem>,
    pub inventory: Vec<Carried>,
    /// Holds on the character it may try to escape, labelled by holder.
    pub holds: Vec<tor_client_common::holds::HoldChoice>,
    /// Granted abilities beyond the basic attack, in the server's order.
    pub abilities: Vec<Ability>,
    /// Carried items the server's pending identification choice accepts;
    /// `None` when no choice is waiting.
    pub identify: Option<Vec<ItemTarget>>,
    pub narration: Vec<String>,
    /// The shared status rows: attributes and place, then health, pools,
    /// gold, hunger, time, holds and combat state.
    pub status: [String; 2],
    pub hp: Option<(u64, u64)>,
    pub has_control: bool,
    /// Whether a new action would be admitted now (no journey or work under way).
    pub can_act: bool,
    /// The latest retained history, oldest first, as "T:tick  what happened".
    pub history: Vec<String>,
    /// The detection charts disclosed so far, in catalog order.
    pub chart_kinds: Vec<tor_protocol::chart_pages::ChartKind>,
    /// Places the character knows, with where each name came from.
    pub places: Vec<PlaceView>,
    /// Whether the server offers to resume or cancel work now.
    pub can_resume: bool,
    pub can_cancel: bool,
    pub tick: u64,
    /// Changes whenever the server's state does; events belong to one revision.
    pub revision: u64,
    /// What the action this view follows did.
    pub events: Vec<Happening>,
}

/// Something seen to happen, from the combat events the server disclosed.
#[derive(Clone, Debug)]
pub enum Happening {
    Ability {
        caster: Option<ActorTarget>,
        target: Option<ActorTarget>,
        id: String,
        name: String,
        outcome: AbilityOutcome,
        /// Ordinal of the blow within a strike sequence.
        strike: Option<u16>,
    },
    Died(ActorTarget),
    /// Health or injury actually worsened between two comparable views.
    Hurt(ActorTarget),
}

/// An item lying in view.
#[derive(Clone)]
pub struct GroundItem {
    pub corpse_seed: Option<AppearanceSeed>,
    pub model: ItemModel,
    pub corpse_model: CreatureModel,
    pub id: ItemTarget,
    pub name: String,
    pub class: ItemClass,
    pub quantity: u64,
    pub description: String,
    pub position: Position,
    pub reachable: bool,
}

/// A granted ability as the player chooses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ability {
    pub id: String,
    /// Its name with cooldown and supply needs, as the shared label reads.
    pub label: String,
    pub target: AbilityTargetKind,
}

/// An item the character carries, with what can be done with it now.
#[derive(Clone)]
pub struct Carried {
    pub model: ItemModel,
    pub id: ItemTarget,
    pub name: String,
    pub class: ItemClass,
    pub quantity: u64,
    pub description: String,
    /// How it is worn or held, when equipped.
    pub equipped: Option<String>,
    pub can_equip: bool,
    pub drinkable: bool,
    pub portable_light: Option<PortableLightView>,
    pub edible: bool,
    pub readable: bool,
    /// How a zappable device is aimed, when it can be zapped now.
    pub device: Option<DeviceActivation>,
}

/// How much retained history the history screen shows.
pub const HISTORY_LINES: usize = 40;

pub fn view(state: &ClientState, palette: &Palette) -> View {
    let o = &state.state().observation;
    let hostile = |id| {
        o.combat
            .as_ref()
            .is_some_and(|c| c.actors.iter().any(|a| a.actor == id && a.hostile))
    };
    let mut actors: Vec<Actor> = o
        .visible_actors
        .iter()
        .map(|a| Actor {
            appearance_seed: a.appearance_seed,
            model: actor_model(palette, a),
            wearing: wearing(palette, &a.worn),
            attire: tor_client_common::narration::Attire::of(&a.worn),
            id: a.id,
            name: a.name.clone(),
            description: a.description.clone(),
            position: a.position,
            hostile: hostile(a.id),
            injury: o
                .combat
                .as_ref()
                .and_then(|c| c.actors.iter().find(|row| row.actor == a.id))
                .map(|row| row.injury),
            is_self: a.id == o.self_target,
        })
        .collect();
    // The observer's own body need not be listed among what it sees.
    // A dead observer's body becomes its corpse, so it is no longer added.
    let dead = o.combat.as_ref().is_some_and(|c| c.dead);
    if !dead {
        let primary = actors
            .iter()
            .position(|a| a.is_self && a.position == o.position);
        let own = if let Some(index) = primary {
            actors.remove(index)
        } else {
            Actor {
                appearance_seed: o.self_appearance.as_ref().map(|appearance| appearance.seed),
                model: creature_model(
                    palette,
                    o.self_appearance
                        .as_ref()
                        .and_then(|appearance| appearance.asset.as_deref()),
                ),
                wearing: o
                    .self_appearance
                    .as_ref()
                    .map_or_else(Wearing::default, |appearance| {
                        wearing(palette, &appearance.worn)
                    }),
                attire: o
                    .self_appearance
                    .as_ref()
                    .map_or_else(Default::default, |appearance| {
                        tor_client_common::narration::Attire::of(&appearance.worn)
                    }),
                id: o.self_target,
                name: String::new(),
                description: String::new(),
                position: o.position,
                hostile: false,
                injury: None,
                is_self: true,
            }
        };
        // An upper body cell or another projection of this actor does not
        // replace its known location. Reconciliation uses this primary first.
        actors.insert(0, own);
    }
    let seen: std::collections::HashSet<Position> =
        o.visible_cells.iter().map(|c| c.position).collect();
    let memory: Vec<_> = state
        .map_memory()
        .filter(|c| !seen.contains(&c.position))
        .map(|c| c.appearance())
        .collect();
    let object_models = o
        .visible_cells
        .iter()
        .filter_map(|cell| {
            cell.object_presentation
                .as_ref()
                .map(|p| (cell.position, object_model(palette, p)))
        })
        .chain(memory.iter().filter_map(|cell| {
            cell.object_presentation
                .as_ref()
                .map(|p| (cell.position, object_model(palette, p)))
        }))
        .collect();
    View {
        object_models,
        shown: state.reply_context(),
        context: state.context().clone(),
        branch: state.branch().clone(),
        feet: o.position,
        visible: o.visible_cells.clone(),
        memory,
        actors,
        items: o
            .ground_items
            .iter()
            .map(|g| GroundItem {
                corpse_seed: g
                    .presentation
                    .is_none()
                    .then(|| g.item.corpse.as_ref().map(|appearance| appearance.seed))
                    .flatten(),
                model: g.presentation.as_ref().map_or_else(
                    || item_model(palette, &g.item),
                    |p| object_model(palette, p),
                ),
                corpse_model: if g.presentation.is_some() {
                    CreatureModel::Placeholder
                } else {
                    creature_model(
                        palette,
                        g.item
                            .corpse
                            .as_ref()
                            .and_then(|appearance| appearance.asset.as_deref()),
                    )
                },
                id: g.item.id,
                name: tor_client_common::items::ground_name(g).to_owned(),
                class: g.item.class,
                quantity: g.item.quantity,
                description: if g.presentation.is_some() {
                    String::new()
                } else {
                    g.item.description.clone()
                },
                position: g.position,
                reachable: g.reachable,
            })
            .collect(),
        inventory: o
            .inventory
            .iter()
            .map(|item| {
                let interactions = o.interactions.as_ref();
                let affordance =
                    interactions.and_then(|i| i.inventory.iter().find(|a| a.item == item.id));
                let equipped = affordance.and_then(|a| a.equipped_slot).map(|index| {
                    interactions
                        .and_then(|i| i.slots.get(index as usize))
                        .map_or_else(|| "in use".to_string(), |slot| slot.equipped_phrase.clone())
                });
                Carried {
                    model: item_model(palette, item),
                    id: item.id,
                    name: item.name.clone(),
                    class: item.class,
                    quantity: item.quantity,
                    description: item.description.clone(),
                    can_equip: affordance
                        .is_some_and(|a| a.slot.is_some() && a.equipped_slot.is_none()),
                    equipped,
                    drinkable: affordance.is_some_and(|a| a.drinkable),
                    portable_light: affordance.and_then(|a| a.portable_light.clone()),
                    edible: affordance.is_some_and(|a| a.edible),
                    readable: affordance.is_some_and(|a| a.readable),
                    device: affordance
                        .filter(|a| a.zappable)
                        .and_then(|a| a.device_activation),
                }
            })
            .collect(),
        holds: tor_client_common::holds::hold_choices(o),
        abilities: tor_client_common::abilities::choices(o)
            .into_iter()
            .map(|ability| Ability {
                label: tor_client_common::abilities::ability_label(&ability),
                id: ability.id,
                target: ability.target_kind,
            })
            .collect(),
        identify: o
            .interactions
            .as_ref()
            .and_then(|i| i.identification.as_ref())
            .map(|pending| {
                o.inventory
                    .iter()
                    .filter(|item| {
                        pending.items.contains(&item.id) && pending.classes.contains(&item.class)
                    })
                    .map(|item| item.id)
                    .collect()
            }),
        narration: state.narration().to_vec(),
        status: tor_client_common::status::lines(state, &[]),
        hp: o.combat.as_ref().map(|c| (c.hp, c.max_hp)),
        has_control: state.has_control(),
        can_act: state.can_admit_intention(),
        history: {
            let entries = state.history();
            entries[entries.len().saturating_sub(HISTORY_LINES)..]
                .iter()
                .map(|entry| {
                    format!(
                        "T:{}  {}",
                        entry.tick,
                        tor_client_common::history::text(entry)
                    )
                })
                .collect()
        },
        chart_kinds: o.charts.iter().map(|summary| summary.kind).collect(),
        places: o.places.clone(),
        can_resume: state.can_resume_intention(),
        can_cancel: state.can_cancel_intention(),
        tick: o.tick,
        revision: state.state().revision,
        events: happenings(o),
    }
}

/// Combat effects disclosed with this observation; never inferred from models.
pub fn happenings(observation: &Observation) -> Vec<Happening> {
    observation
        .combat
        .iter()
        .flat_map(|c| &c.events)
        .filter_map(|e| match e {
            CombatEventView::Ability {
                caster,
                target,
                ability,
                outcome,
                strike,
            } => Some(Happening::Ability {
                caster: *caster,
                target: *target,
                id: ability.id.clone(),
                name: ability.name.clone(),
                outcome: *outcome,
                strike: *strike,
            }),
            CombatEventView::Died { actor } => Some(Happening::Died(*actor)),
            _ => None,
        })
        .collect()
}

impl Happening {
    /// Whether the server reports that the effect applied, without implying damage.
    pub fn is_applied(&self) -> bool {
        matches!(
            self,
            Self::Ability {
                outcome: AbilityOutcome::Applied,
                ..
            }
        )
    }
}

/// Disclosed worsening only: a newly visible injury is not a new wound.
pub fn observed_harm(before: &View, after: &View) -> Vec<ActorTarget> {
    if before.context != after.context
        || before.branch != after.branch
        || before.shown.actor != after.shown.actor
    {
        return vec![];
    }
    let rank = |injury| match injury {
        Injury::Healthy => 0,
        Injury::Wounded => 1,
        Injury::BadlyWounded => 2,
        Injury::NearDeath => 3,
    };
    let mut hurt = std::collections::BTreeSet::new();
    for actor in &after.actors {
        if actor.is_self {
            if matches!((before.hp, after.hp), (Some((old, _)), Some((now, _))) if now < old) {
                hurt.insert(actor.id);
            }
        } else if let Some(now) = actor.injury {
            if before.actors.iter().any(|old| {
                old.id == actor.id && old.injury.is_some_and(|old| rank(now) > rank(old))
            }) {
                hurt.insert(actor.id);
            }
        }
    }
    hurt.into_iter().collect()
}

/// A terrain appearance for drawing and exploration. This carries no entity
/// targets, door affordances or occupants. Remembered terrain is always dim.
pub struct Terrain<'a> {
    pub object_presentation: Option<&'a ObjectPresentation>,
    pub object_model: Option<ItemModel>,
    pub key: &'a str,
    pub position: Position,
    pub wall: bool,
    pub stairs_up: bool,
    pub stairs_down: bool,
    pub door: Option<bool>,
    /// A discovered trap, by family once that is known.
    pub trap: Option<Option<TrapFamily>>,
    pub seen: bool,
    pub lit: bool,
}
impl View {
    pub fn terrain(&self) -> impl Iterator<Item = Terrain<'_>> {
        self.visible
            .iter()
            .map(|c| Terrain {
                object_presentation: c.object_presentation.as_ref(),
                object_model: self.object_models.get(&c.position).copied(),
                key: &c.key,
                position: c.position,
                wall: c.wall,
                stairs_up: c.stairs_up,
                stairs_down: c.stairs_down,
                door: c.door.as_ref().map(|d| d.open),
                trap: c.trap.as_ref().map(|t| t.family),
                seen: true,
                lit: c.lit,
            })
            .chain(self.memory.iter().map(|c| Terrain {
                object_presentation: c.object_presentation.as_ref(),
                object_model: self.object_models.get(&c.position).copied(),
                key: &c.key,
                position: c.position,
                wall: c.wall,
                stairs_up: c.stairs_up,
                stairs_down: c.stairs_down,
                door: c.door.as_ref().map(|d| d.open),
                trap: c.trap.as_ref().map(|t| t.family),
                seen: false,
                lit: false,
            }))
    }
}
