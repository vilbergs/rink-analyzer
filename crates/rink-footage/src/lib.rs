//! Local hockey footage ingestion and session storage.

mod extract;
mod mov;
pub mod session;

pub use extract::{extract, SavedFrame, Selection};
