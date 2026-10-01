//! NetHack interaction model shared by a window. Pixels, the font, and the socket
//! stay in the ASCII client. This crate does not own a frontend trait.

mod chart;
mod column;
mod command;
mod config;
mod glyphs;
mod letters;
mod log;
mod queue;

pub use chart::{chart_shift, shift_origin};
pub use column::{column_glyph, map_columns, ColumnGlyph, DrawnColumn};
pub use command::{
    adjacent, bounded_repeat, bump, creature_at, decide_autopickup, door_at, door_command,
    drawn_creature, feet_items, fight, gone_sentence, look_at, offset, other_actors,
    pickup_sentence, repeat_intent, repeat_interrupted, take_quantity, to_action, AutoPickup,
    AutoQuery, Resolved, BAD_QUANTITY, BUFFER_FULL, CANCEL_TRAVEL_FIRST, HELP_LINE, ILLEGAL_TARGET,
    NOTHING_HERE, NOTHING_TO_FIGHT, NO_DOOR, REPEAT_CAP, RUN_CAP, UNAVAILABLE,
};
pub use config::{load_config, parse_config, BumpAttacks, Click, SessionConfig};
pub use glyphs::{resolve, Glyph, Structure, REMEMBERED_COLOR};
pub use letters::{temporary_letter, InventoryLetters};
pub use log::{wrap, MessageLog, MORE, SCROLLBACK_CAP, SKIPPED, UNACKED_CAP};
pub use queue::{classify_try_send, Intent, IntentQueue, SendFate, TrySend, QUEUE_CAP};
