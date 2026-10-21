# 11. Risks and technical debt

## 11.1 Risks

| Risk | Effect | Handling |
|------|--------|----------|
| ACE-Step cannot cancel or delete a task | A cancelled song still uses the GPU until it ends | Accepted; ACE-Step's own cleanup removes the file (arc42 3.3). |
| ACE-Step's API changes between versions | Songs stop being made | Health reports `incompatible`; the fake in `e2e/fake-ace.mjs` mirrors ACE-Step's real response shapes. |
| One machine, one disk | A lost disk loses the library | No backup is built in; the data volume is plain files and one SQLite file to copy. |
| MediaMTX restarts or Maio restarts | RTSP, SRT and RTMP readers see `404` for a few seconds until Maio republishes | Readers reconnect; the MediaMTX panel shows it. |
| The API token leaks | Full control of the library | It lives only in `.env`, is compared in constant time and never logged; changing it means changing `.env` and restarting. |
| Live streams are open to anyone with the address | Unwanted listeners | Private links; a new link stops the old one at once. |

## 11.2 Technical debt

| Debt | Why it is there | Pay it when |
|------|-----------------|-------------|
| A failed job's reason is generic | ACE-Step's `detail` text is not shown, and a `429` from it is not retried | Failures become frequent or confusing. |
| No sweep of `tmp/` | An export cut off midway leaves its file | Disk use matters. |
| A copied energy setting stays on the Make form | "Make one like this" copies it; nothing shows it as a chip to remove | People notice. |
| Switching the model while a song is being made | Not guarded | It bites once. |
| The `/pkg` cache header does not read `hash.txt` | It trusts the hashed names | The build stops hashing. |
| The Docker build recompiles every dependency | No BuildKit cache mounts; a rebuild takes about 10 minutes | Rebuilds slow the work down. |
| No performance baseline | The `perf` branch waits for one | Before tuning anything. |
| No announcement of the song on air or the listener count | Every song change would speak; left silent on purpose | Someone listening with a screen reader asks for it. |
