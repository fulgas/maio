# Go live and share a stream

Use this to put a playlist on air and give listeners an address for their player.

1. Open the playlist and turn on **Live** in the strip under its band. At most `max_live` playlists are live at once (4 by default, in `maio.toml`).
2. Choose who can listen:
   - **Public**: the address names the playlist, `/live/{playlist}/stream.mp3`.
   - **Private**: the address carries a secret key, `/listen/{key}/stream.mp3`. **New link** makes a new key and stops the old one at once.
3. Copy an address with its copy button:
   - MP3, AAC or HLS for browsers and most players, on port 8080;
   - RTSP, SRT or RTMP for media tools, through MediaMTX on ports 8555, 8891 and 1936.
4. If listeners reach Maio by another name than `localhost`, make the addresses use that name: the MP3, AAC and HLS ones take the address in your browser, so open Maio by that name before copying them; for RTSP, SRT and RTMP, set `host` under `[relay]` in `maio.toml` and restart Maio.
5. Watch the strip: it shows the listener count, the song on air and the one after it. The MediaMTX icon in the header turns grey if MediaMTX stops answering.
6. Turn **Live** off to stop: the encoders stop and every listener is disconnected.

The addresses, the shuffle setting and what happens after a restart: [arc42 8.4](../architecture/08-crosscutting-concepts.md#84-live).
