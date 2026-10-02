//! Disclosed entity representation and scope extraction from StateView.
//!
//! Part of the resolver (`scope`, `matcher`, `context`), which is built and
//! tested but not yet used by the game: `adventure::Dialogue` still resolves
//! names, pronouns and clarification answers itself. See
//! section 3.3 of docs/if-parser-architecture.md.

use tor_client_common::surfaces;
use tor_protocol::{ActorId, Direction, Position, StateView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceType {
    Floor,
    Wall,
    Ceiling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entity {
    Item {
        id: u64,
        name: String,
        description: String,
        quantity: u64,
        reachable: bool,
        carried: bool,
    },
    Actor {
        id: ActorId,
        name: String,
        description: String,
        reachable: bool,
        is_self: bool,
    },
    Door {
        id: u64,
        name: String,
        description: String,
        open: bool,
        reachable: bool,
        approaches: Vec<String>,
    },
    Surface {
        surface_type: SurfaceType,
        materials: Vec<String>,
    },
    Exit {
        direction: Direction,
        destination_key: String,
        label: String,
    },
}

impl Entity {
    pub fn name(&self) -> &str {
        match self {
            Entity::Item { name, .. } => name.as_str(),
            Entity::Actor { name, is_self, .. } => {
                if *is_self {
                    "myself"
                } else if name.is_empty() {
                    "figure"
                } else {
                    name.as_str()
                }
            }
            Entity::Door { name, .. } => name.as_str(),
            Entity::Surface { surface_type, .. } => match surface_type {
                SurfaceType::Floor => "floor",
                SurfaceType::Wall => "wall",
                SurfaceType::Ceiling => "ceiling",
            },
            Entity::Exit { label, .. } => label.as_str(),
        }
    }

    pub fn is_item(&self) -> bool {
        matches!(self, Entity::Item { .. })
    }

    pub fn is_carried(&self) -> bool {
        matches!(self, Entity::Item { carried: true, .. })
    }

    pub fn is_door(&self) -> bool {
        matches!(self, Entity::Door { .. })
    }

    pub fn is_actor(&self) -> bool {
        matches!(self, Entity::Actor { .. })
    }
}

/// The set of all disclosed game entities currently in perceptual scope.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub entities: Vec<Entity>,
}

impl Scope {
    /// Extracts all entities in scope from the given StateView.
    pub fn from_state(state: &StateView) -> Self {
        let mut entities = Vec::new();
        let obs = &state.observation;

        // 1. Carried items in inventory
        for item in &obs.inventory {
            entities.push(Entity::Item {
                id: item.id,
                name: item.name.clone(),
                description: item.description.clone(),
                quantity: item.quantity,
                reachable: true,
                carried: true,
            });
        }

        // 2. Visible ground items
        for ground in &obs.ground_items {
            entities.push(Entity::Item {
                id: ground.item.id,
                name: ground.item.name.clone(),
                description: ground.item.description.clone(),
                quantity: ground.item.quantity,
                reachable: ground.reachable,
                carried: false,
            });
        }

        // 3. Visible actors
        for actor in &obs.visible_actors {
            let is_self = actor.id == obs.actor;
            let reachable = actor.position == Position { x: 0, y: 0, z: 0 };
            entities.push(Entity::Actor {
                id: actor.id,
                name: actor.name.clone(),
                description: actor.description.clone(),
                reachable,
                is_self,
            });
        }

        // 4. Visible doors
        let mut seen_doors = std::collections::BTreeSet::new();
        for cell in &obs.visible_cells {
            if let Some(door) = &cell.door {
                if seen_doors.insert(door.id) {
                    entities.push(Entity::Door {
                        id: door.id,
                        name: door.name.clone(),
                        description: door.description.clone(),
                        open: door.open,
                        reachable: door.reachable,
                        approaches: door.approaches.clone(),
                    });
                }
            }
        }

        // 5. Surfaces (floors, walls, ceilings)
        let roles = surfaces::roles(&obs.visible_cells);
        if !roles.floors.is_empty() {
            entities.push(Entity::Surface {
                surface_type: SurfaceType::Floor,
                materials: roles.floors.into_iter().map(String::from).collect(),
            });
        }
        if !roles.walls.is_empty() {
            entities.push(Entity::Surface {
                surface_type: SurfaceType::Wall,
                materials: roles.walls.into_iter().map(String::from).collect(),
            });
        }
        if !roles.ceilings.is_empty() {
            entities.push(Entity::Surface {
                surface_type: SurfaceType::Ceiling,
                materials: roles.ceilings.into_iter().map(String::from).collect(),
            });
        }

        Self { entities }
    }
}
