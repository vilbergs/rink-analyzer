//! Everything about a play that isn't UI: the model, rink geometry,
//! interpolation (`sample`), validation and the file format. The GPUI app and
//! the MCP server are thin shells over this crate.

pub mod io;
pub mod model;
pub mod rink;
pub mod sample;
pub mod spline;
pub mod templates;
pub mod validate;

pub use glam::Vec2;
pub use model::*;
pub use rink::{RinkPreset, RinkSpec, Zone};
pub use sample::{sample, EntityState, PuckState, Snapshot};
pub use validate::{validate, Issue};
