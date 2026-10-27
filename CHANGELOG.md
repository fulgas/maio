# Changelog

Every release of Maio, made from its Conventional Commits (see docs/project-rules.md).

## [0.2.0](https://github.com/fulgas/maio/compare/v0.1.1...v0.2.0) - 2026-10-27

### Features

- *(ui)* pick any song length from 0:15 to 10:00 with a slider ([#23](https://github.com/fulgas/maio/issues/23) [#23](https://github.com/fulgas/maio/issues/23) )
- *(ui)* pick any tempo from 40 to 220 bpm with a slider, or leave it on Auto ([#24](https://github.com/fulgas/maio/issues/24) [#24](https://github.com/fulgas/maio/issues/24) )
- *(ui)* pick every song setting in a settings panel, each one on Auto until you choose ([#28](https://github.com/fulgas/maio/issues/28) [#28](https://github.com/fulgas/maio/issues/28) )
- *(account)* show or hide a password while typing it, with the eye at the end of the field ([#30](https://github.com/fulgas/maio/issues/30) [#30](https://github.com/fulgas/maio/issues/30) )

### Fixes

- *(ui)* on a phone a playlist's name fits its band, its songs keep their titles, and the header has two tidy rows
- *(ui)* a song row names the playlists that hold it, its mood and key stay on one line, and the live strip fits a phone
- *(ui)* Make waits for a ready music engine, and says why it is off ([#21](https://github.com/fulgas/maio/issues/21) [#21](https://github.com/fulgas/maio/issues/21) )
- *(live)* RTSP, SRT and RTMP addresses can't be copied while MediaMTX is not answering ([#22](https://github.com/fulgas/maio/issues/22) [#22](https://github.com/fulgas/maio/issues/22) )
- *(ui)* settings stacked under the prompt stay at most 480 px wide, so values stay near their names

## [0.1.1](https://github.com/fulgas/maio/releases/tag/v0.1.1) - 2026-10-26

### Features

- v0, the radio
- v1's documentation and core library
- *(songs)* making songs
- *(api)* the server and the JSON API
- *(ui)* the web app
- *(live)* live streaming
- *(engine)* ACE-Step's own API, the engine panel, tracing, downloads and song details
- *(playlists)* playlists as rows, a shelf that never scrolls, an optional ACE-Step API key
- *(relay)* the MediaMTX panel; live playlists resume after a restart
- *(player)* listen live in the player; the player bar only when it holds something; Shuffle play
- *(ui)* Add songs is a table of the songs not yet in the playlist; the engine and MediaMTX are header icons
- *(ui)* the name says what it is: Music + AI + Output
- *(ui)* a favicon, the wordmark's "ai" in mustard
- *(docs-site)* a Diátaxis compass leads to the four kinds of documentation
- *(docs-site)* the compass is the home page; the C4 explorer moves to /explore/
- *(engine)* busy answers and failed polls back off exponentially
- *(config)* the example config has hip-hop, funk, folk, chillhop and reggae
- *(web)* changes reach the web app as events, and a stopped engine shows within seconds
- *(web)* every event carries the new state, in the order it happened
- *(ui)* each part of the page changes only by its own events, with nothing fetched
- *(docs-site)* every page links to the repository on GitHub from the header

### Fixes

- *(ui)* accessibility review fixes; rename and delete on Playlists rows
- *(docs-site)* the header shows the version from Cargo.toml, not "v1 · running"
- *(docs)* arc42 3.4 lists the fields a text engine really has
- *(ui)* the songs search shows its focus ring around the whole pill
- *(docs-site)* the reading column widens to fit its tables, text with it
- *(engine)* a failed song says which engine is not answering and what to do
- *(ui)* every job's bar sits in the same place, whatever its text says
- *(engine)* a full ACE-Step queue is retried, not a failed song
- *(ui)* a down engine says to check its server, wherever it runs
- *(live)* a poisoned lock is taken back instead of panicking
- *(ui)* engine messages name the configuration file, not its file name
- *(ui)* the Make form's chips show their focus ring around the whole chip
- *(web)* the next song and a model switch show at once, with no hooks or locks behind them
- *(library)* a song deleted while it is added to a playlist answers 404 or 409, not 500
- *(web)* every open page shows the newest state, even after a gap, a race or a slow start
- *(docs-site)* the site works under any path, so GitHub Pages can serve it under /maio/
- *(docs-site)* the theme button works on the documentation home too
- *(release)* releases get their downloadable files again, attached before the release is published
