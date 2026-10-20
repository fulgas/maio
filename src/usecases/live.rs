//! Listening to live playlists and the MediaMTX relay (arc42 8.4): the stream routes, the relay's auth hook and the
//! relay's state go through here.

use std::{path::PathBuf, sync::Arc};

use crate::domain::{
    Playlist, PlaylistRef,
    live::{self, PathKind, StreamFormat},
    relay::{self, RelayAddresses, RelayStatus},
};
use crate::usecases::{
    Result,
    ports::{Listen, LiveListening, PlaylistRepository, Relay},
};

#[derive(Clone)]
pub struct Live {
    playlists: Arc<dyn PlaylistRepository>,
    listening: Arc<dyn LiveListening>,
    relay: Arc<dyn Relay>,
}

impl Live {
    pub fn new(playlists: Arc<dyn PlaylistRepository>, listening: Arc<dyn LiveListening>, relay: Arc<dyn Relay>) -> Self {
        Self { playlists, listening, relay }
    }

    fn live_playlists(&self) -> Result<Vec<Playlist>> {
        Ok(self.playlists.list()?.into_iter().filter(|p| p.live.on).collect())
    }

    /// The live playlist a stream path names.
    fn find(&self, kind: PathKind, name: &str) -> Option<String> {
        let all = self.playlists.list().ok()?;
        live::by_path(&all, kind, name).map(|p| p.id.clone())
    }

    /// A listener's stream of the live playlist the path names, and that playlist's id.
    pub fn listen(&self, kind: PathKind, name: &str, format: StreamFormat) -> Option<(String, Listen)> {
        let id = self.find(kind, name)?;
        let listen = self.listening.listen(&id, format)?;
        Some((id, listen))
    }

    pub fn now_title(&self, id: &str) -> Option<String> {
        self.listening.now_title(id)
    }

    /// The HLS folder of the live playlist the path names.
    pub fn hls_dir(&self, kind: PathKind, name: &str) -> Option<PathBuf> {
        self.find(kind, name).map(|id| self.listening.hls_dir(&id))
    }

    /// MediaMTX's auth hook: may this caller do this on this path?
    pub fn authorize(&self, user: &str, password: &str, action: &str, path: &str) -> bool {
        let Some(caller) = self.relay.caller(user, password) else { return false };
        let paths: Vec<String> = self.live_playlists().unwrap_or_default().iter().map(live::relay_path).collect();
        relay::allowed(caller, action, path, &paths)
    }

    /// MediaMTX and every live playlist on it; `None` without a relay.
    pub fn relay_status(&self) -> Result<Option<RelayStatus>> {
        let on_relay: Vec<_> =
            self.live_playlists()?.iter().map(|p| (PlaylistRef { id: p.id.clone(), name: p.name.clone() }, live::relay_path(p))).collect();
        Ok(self.relay.status(&on_relay))
    }

    /// Where relay listeners reach a playlist; `None` without a relay.
    pub fn relay_addresses(&self, p: &Playlist) -> Option<RelayAddresses> {
        self.relay.addresses(&live::relay_path(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Visibility;
    use crate::usecases::testing::{FakeListening, FakeRelay, MemPlaylists};

    fn playlist(id: &str, name: &str) -> Playlist {
        Playlist::new(id.into(), name, format!("key-{id}"), "2026-10-20T00:00:00Z".into()).unwrap()
    }

    fn live_with(relay: FakeRelay) -> (Live, Arc<FakeListening>) {
        let mut on = playlist("p1", "Late coding");
        on.live.on = true;
        let mut private = playlist("p2", "Night");
        private.live.on = true;
        private.live.visibility = Visibility::Private;
        private.live.key = "KEY".into();
        let off = playlist("p3", "Off air");
        let listening = Arc::new(FakeListening::default());
        let repo = Arc::new(MemPlaylists(std::sync::Mutex::new(vec![on, private, off])));
        (Live::new(repo, listening.clone(), Arc::new(relay)), listening)
    }

    #[test]
    fn the_auth_hook_lets_maio_do_anything_and_anyone_read_a_live_path() {
        let (live, _) = live_with(FakeRelay::on());
        assert!(live.authorize("maio", "secret", "publish", "live/p1"));
        assert!(!live.authorize("maio", "wrong", "read", "live/p1"));
        assert!(live.authorize("", "", "read", "live/p1"));
        assert!(live.authorize("", "", "read", "listen/KEY"));
        assert!(!live.authorize("", "", "read", "live/p3"), "off air");
        assert!(!live.authorize("", "", "publish", "live/p1"));
    }

    #[test]
    fn without_a_relay_the_auth_hook_refuses_everyone() {
        let (live, _) = live_with(FakeRelay::default());
        assert!(!live.authorize("", "", "read", "live/p1"));
        assert_eq!(live.relay_status().unwrap(), None);
    }

    #[test]
    fn the_relay_status_names_every_live_playlist_with_its_path() {
        let relay = FakeRelay::on();
        let (live, _) = live_with(relay);
        let status = live.relay_status().unwrap().expect("a relay");
        let names: Vec<_> = status.streams.iter().map(|s| s.playlist.name.as_str()).collect();
        assert_eq!(names, ["Late coding", "Night"]);
    }

    #[test]
    fn a_stream_path_reaches_only_a_live_playlist() {
        let (live, listening) = live_with(FakeRelay::default());
        assert_eq!(live.listen(PathKind::Live, "p1", StreamFormat::Mp3).map(|(id, _)| id).as_deref(), Some("p1"));
        assert_eq!(live.listen(PathKind::Listen, "KEY", StreamFormat::Aac).map(|(id, _)| id).as_deref(), Some("p2"));
        assert!(live.listen(PathKind::Live, "p3", StreamFormat::Mp3).is_none(), "off air");
        assert!(live.listen(PathKind::Live, "p2", StreamFormat::Mp3).is_none(), "a private playlist only by its key");
        assert_eq!(live.hls_dir(PathKind::Live, "p1"), Some(PathBuf::from("/hls/p1")));
        assert_eq!(*listening.asked.lock().unwrap(), ["p1 Mp3", "p2 Aac"]);
    }
}
