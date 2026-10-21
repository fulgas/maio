# 12. Glossary

| Term | Meaning |
|------|---------|
| **Song** | One generation from one prompt, kept in the library until you delete it (arc42 8.2). |
| **Prompt** | What a song should sound like, in your words; at most 500 characters. |
| **Take** | Each Make is a new take: a new random seed unless you copied one. |
| **Seed** | The number that, with the same settings, makes the same song again. |
| **Library** | All your songs. |
| **Playlist** | A named, ordered collection of songs (arc42 8.3). |
| **Live** | A playlist's switch that streams it, looping, to anyone with its address (arc42 8.4). |
| **On air** | The song a live playlist is playing now; **next** is the one picked to follow. |
| **Listen key**, **private link** | The 32 random characters in a private playlist's `/listen/{key}` address. |
| **Live edge** | Where a stream is now; joining a stream starts there, not where you paused. |
| **Engine**, **music engine** | A program that makes music; here ACE-Step 1.5 through `acestep-api` (arc42 3.3). |
| **Text engine** | A language model that writes lyrics, titles and descriptions (arc42 3.4). |
| **Job** | One song being made: waiting, making, done, failed or cancelled. |
| **Relay** | MediaMTX, which carries live playlists over RTSP, SRT and RTMP. |
| **Reader** | Someone pulling a live playlist from MediaMTX. |
| **Listener** | Someone pulling a live playlist over MP3, AAC or HLS, or a reader; the live strip counts both. |
| **Station** | Inside Maio, the playout and encoders of one live playlist. |
| **Shelf** | The row of playlist tiles on the Songs screen; also the name of the web app's design. |
| **Band** | The coloured block at the top of a playlist's page. |
| **Live strip** | The controls under a live playlist's band: switch, listeners, visibility, stream shuffle, new link, addresses. |
| **Player bar** | The mustard bar at the bottom that plays songs and live streams. |
| **Shuffle play** | The band's one-shot: the playlist from a random song with the player's shuffle on. |
| **Port** | A trait the use cases own; **adapter**: what implements it (arc42 5.2). |
| **Hexagon** | How Maio's code is shaped (arc42 5): a core of rules and use cases, with ports on every side where adapters plug in. Also called ports and adapters. |
| **Port** | A trait the core owns and needs, such as `SongRepository` or `MusicEngine` (arc42 5.2). |
| **Adapter** | Code that connects a port to the world: SQLite, ACE-Step over HTTP, ffmpeg, the web server. |
| **Driving adapter** | An adapter that calls into the core: the web server and, through the API, the web app. |
| **Driven adapter** | An adapter the core calls through a port: storage, engines, audio, live streams, the clock. |
