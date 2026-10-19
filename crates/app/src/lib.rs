//! Use cases and the ports they need (SPEC 3). Adapters implement the ports; the web layer calls the use cases.

mod error;
mod making;
mod playlists;
pub mod ports;
mod songs;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use error::{Error, Result};
pub use playlists::{PlaylistSummary, PlaylistView, Playlists};
pub use songs::{DEFAULT_LIMIT, MAX_LIMIT, SongPage, SongView, Songs};
pub use making::{Accepted, MakeSettings, Making, MakingPorts};
