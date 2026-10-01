//! NetHack interaction model shared by a window. Pixels, the font, and the socket
//! stay in the ASCII client. This crate does not own a frontend trait.

mod chart;
mod column;
mod config;
mod glyphs;
mod log;

pub use chart::{chart_shift, shift_origin};
pub use column::{column_glyph, map_columns, ColumnGlyph, DrawnColumn};
pub use config::{load_config, parse_config, BumpAttacks, Click, SessionConfig};
pub use glyphs::{resolve, Glyph, Structure, REMEMBERED_COLOR};
pub use log::{wrap, MessageLog, MORE, SCROLLBACK_CAP, SKIPPED, UNACKED_CAP};
