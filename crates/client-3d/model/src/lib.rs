//! Presentation and input policy for the independently maintained 3D client.
//! No Bevy, native window, filesystem or simulation dependency belongs here.
mod advancement;
mod appearance;
mod commands;
mod equipment;
mod frame;
mod interaction;
mod inventory;
mod map;
mod observation;
mod routing;

pub use advancement::{
    catalog_command, AdvancementAction, AdvancementBrowser, AdvancementOption, AdvancementView,
};
pub use appearance::{
    actor_model, creature_model, item_model, object_model, wearing, CreatureModel, ItemModel,
    Wearing,
};
pub use commands::{
    offset, request_from, AbilityTarget, Cmd, Input, ProjectileStep, MAX_PLACE_NAME,
};
pub use equipment::Equipment;
pub use frame::{FrameChange, SceneFrame};
pub use interaction::{
    describe, describe_focus, direction, known_cells, near_actor, Focus, Intent,
};
pub use inventory::{InventoryBrowser, InventoryFilter, MAX_INVENTORY_SEARCH};
pub use map::{map_destinations, MapDestination, MapSelection};
pub use observation::{
    happenings, observed_harm, view, Ability, Actor, Carried, GroundItem, Happening, Terrain, View,
};

pub use routing::{approach_cells, attack_at_hand, step_toward};
