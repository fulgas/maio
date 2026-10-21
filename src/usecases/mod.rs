//! Use cases and the ports they need (arc42 5). Adapters implement the ports; the web layer calls the use cases.

mod account;
mod error;
mod making;
mod playlists;
pub mod ports;
mod songs;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use account::{Account, SignedIn};
pub use error::{Error, Result};
pub use making::{Accepted, MakeSettings, Making, MakingPorts, TextStatus};
pub use playlists::{LiveView, PlaylistSummary, PlaylistView, Playlists};
pub use songs::{DEFAULT_LIMIT, MAX_LIMIT, SongPage, SongView, Songs};
