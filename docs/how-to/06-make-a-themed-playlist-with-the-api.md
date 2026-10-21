# Make a themed playlist with the API

Use this to fill a playlist with songs that share a mood and a genre, from a script instead of the web app. You need `curl`, `jq` and the API token (`MAIO_API_TOKEN` in `.env`).

1. Load the token and the address into your shell, without printing the token:

   ```bash
   set -a; . ./.env; set +a
   API=http://localhost:8080/api/v1
   AUTH="Authorization: Bearer $MAIO_API_TOKEN"
   ```

2. In a second terminal (load the same variables there too), watch for finished songs. When a job is done, its `job` event carries the new song's id:

   ```bash
   curl -sN -H "$AUTH" $API/events | sed -n 's/^data: //p' | jq -r 'select(.state == "done") | .song_id'
   ```

3. Create the playlist and keep its id:

   ```bash
   PL=$(curl -s -H "$AUTH" -H 'Content-Type: application/json' -d '{"name": "Deep Work"}' $API/playlists | jq -r .id)
   ```

4. Ask for the songs. Keep the genre and the prompt the same so the songs belong together, and change the tempo a little so they do not all sound alike. Each request answers with a `job_id`:

   ```bash
   for tempo in 80 85 90 84; do
     curl -s -H "$AUTH" -H 'Content-Type: application/json' -d "{
       \"prompt\": \"soft drums, warm keys, vinyl crackle\",
       \"mood\": \"focused\", \"genre\": \"lo-fi\", \"energy\": \"low\",
       \"tempo\": $tempo, \"seconds\": 210, \"vocals\": false}" $API/songs | jq -r .job_id
   done
   ```

   At most `pending_jobs` songs wait at once (4 in `maio.example.toml`), and `[limits] make` caps how many you ask for in a window (`20/10m`). Past either, Maio answers `429` with a `Retry-After`: wait for songs to finish, then ask for the rest. ACE-Step makes one song at a time.

5. When the song ids appear in the second terminal, add them to the playlist in the order you want:

   ```bash
   curl -s -H "$AUTH" -H 'Content-Type: application/json' \
     -d '{"song_ids": ["01J...", "01J...", "01J...", "01J..."]}' $API/playlists/$PL/songs
   ```

6. Repeat from step 3 for the next playlist. To put one on air, see [Go live and share a stream](03-go-live-and-share-a-stream.md).

## Playlists to start from

Every mood and genre must be in `moods` and `genres` in `maio.toml`; Maio refuses any other with `unknown_mood` or `unknown_genre`. The genres marked * are not in `maio.example.toml`: add them to `genres` and restart Maio first. A tempo is `slow` (75 bpm), `medium` (105), `fast` (135) or a number from 40 to 220; `seconds` is 15 to 600.

| Playlist | mood | genre | energy | tempo | vocals | seconds | prompt |
|---|---|---|---|---|---|---|---|
| Deep Work | focused | lo-fi | low | 85 | false | 210 | soft drums, warm keys, vinyl crackle |
| Study Beats | focused | chillhop* | low | 88 | false | 180 | jazzy chords, dusty drums, mellow bass |
| Night Drive | dreamy | synthwave | medium | 105 | some | 230 | analog pads, steady bass, neon city at 2am |
| Morning Coffee | upbeat | jazz | medium | medium | false | 180 | brushed drums, upright bass, bright piano |
| Sleep | calm | ambient | low | 55 | false | 420 | slow drones, no percussion, long reverb |
| Rainy Day | melancholy | lo-fi | low | slow | some | 210 | rain texture, muted guitar, sparse beat |
| Campfire | calm | folk* | low | 80 | true | 200 | acoustic guitar, soft voice, crackling fire |
| Sunday Funk | upbeat | funk* | medium | 110 | some | 200 | slap bass, wah guitar, tight horns |
| Late Night Cypher | tense | hip-hop* | medium | 90 | true | 180 | boom bap drums, dark piano loop |
| Workout | triumphant | electronic | high | 132 | some | 210 | punchy kick, build-ups, big drops |
| Epic Focus | triumphant | orchestral | medium | 100 | false | 240 | strings, brass swells, film-score build |
| Garage Energy | playful | rock | high | fast | true | 180 | driving guitars, loud drums, shout-along chorus |
| Boss Fight | tense | orchestral | high | 150 | false | 180 | pounding drums, staccato strings, choir |

"some" in the vocals column means `true` for about a third of the songs and `false` for the rest. Change the tempo by up to 10 bpm from song to song; for Sleep, let it fall instead (65 down to 50), and for Workout let the energy go `medium`, `high`, `high`, `medium`.

Every field a song request takes, and what each one does: [arc42 8.2](../architecture/08-crosscutting-concepts.md#82-songs). Every API call: [arc42 3.5](../architecture/03-system-scope-and-context.md#35-the-http-api).
