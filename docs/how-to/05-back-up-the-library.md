# Back up the library

Use this to keep a copy of your songs, playlists and account. Everything Maio keeps is in the `data` folder beside `compose.yaml`, plus your two config files.

1. Stop Maio, so the library is not being written: `just stop`.
2. Copy these somewhere safe:
   - `data/`: `library.db` (songs, playlists, the account) and `songs/` (the song files);
   - `maio.toml` and `.env`. `.env` holds your secrets, so keep the copy private.
3. Start Maio again: `just stack`.

To restore, put the copied `data/`, `maio.toml` and `.env` back beside `compose.yaml` and start Maio. `data/tmp/` and `data/live/` can be left out: they hold downloads being made and the live streams' working files.

What each part of the data folder holds: [arc42 7.2](../architecture/07-deployment-view.md#72-the-data-volume).
