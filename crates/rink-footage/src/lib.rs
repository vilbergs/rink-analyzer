//! Local hockey footage ingestion and session storage.

mod extract;
pub mod identity;
mod mov;
pub mod session;
#[cfg(feature = "yolo")]
pub mod yolo;

pub use extract::{extract, SavedFrame, Selection};
