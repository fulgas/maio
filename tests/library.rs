//! The use cases over the real SQLite adapters: the rules hold end to end.

use std::sync::Arc;

use maio::adapters::{
    sqlite::{Db, SqlitePlaylists, SqliteSongs},
    system::{RandomIds, SystemClock},
};
use maio::usecases::{
    Playlists, Songs,
    ports::{SongRepository, SongSort},
    testing::MemFiles,
};
use maio::domain::{Error as Rule, testing::song};

#[test]
fn songs_and_playlists_follow_the_rules_over_sqlite() {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let (song_table, playlist_table) = (Arc::new(SqliteSongs(db.clone())), Arc::new(SqlitePlaylists(db)));
    let files = Arc::new(MemFiles::default());
    let songs = Songs::new(song_table.clone(), playlist_table.clone(), files.clone());
    let playlists = Playlists::new(playlist_table, song_table.clone(), Arc::new(RandomIds), Arc::new(SystemClock));
    for id in ["a", "b"] {
        song_table.insert(&song(id)).unwrap();
    }

    let late = playlists.create("Late").unwrap();
    playlists.add_songs(&late.id, &["b".into(), "a".into(), "b".into()], None).unwrap();
    assert_eq!(playlists.get(&late.id).unwrap().songs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);

    let refused = songs.delete("a").unwrap_err();
    assert_eq!(refused.rule().map(Rule::code), Some("song_in_playlist"));
    assert_eq!(songs.get("a").unwrap().playlists[0].name, "Late");

    playlists.remove_song(&late.id, "a").unwrap();
    songs.delete("a").unwrap();
    assert_eq!(*files.deleted.lock().unwrap(), ["songs/a.opus"]);

    songs.rename("b", "Night Bus").unwrap();
    let found = songs.search(Some("night"), SongSort::Newest, None, None).unwrap();
    assert_eq!(found.songs.iter().map(|v| v.song.id.as_str()).collect::<Vec<_>>(), ["b"]);

    assert_eq!(playlists.list().unwrap()[0].songs, 1);
    playlists.delete(&late.id).unwrap();
    songs.delete("b").unwrap();
}
