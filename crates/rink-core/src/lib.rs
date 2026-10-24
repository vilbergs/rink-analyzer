//! Everything about a play that isn't UI: the model, rink geometry,
//! interpolation (`sample`), spatial facts, position reviews, validation and
//! the file format. The GPUI app and the `rink` CLI are thin shells over this
//! crate.

pub mod facts;
pub mod io;
pub mod migrate;
pub mod model;
pub mod review;
pub mod rink;
pub mod sample;
pub mod spline;
pub mod templates;
pub mod validate;

pub use glam::Vec2;
pub use model::*;
pub use review::{Review, Suggestion, SuggestionError};
pub use rink::{RinkPreset, RinkSpec, Zone};
pub use sample::{sample, EntityState, PuckState, Snapshot};
pub use validate::{validate, Issue};
