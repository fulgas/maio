// A fake ACE-Step `acestep-api` for the browser tests: the calls Maio makes (arc42 3.3), sine tones for songs.
import http from "node:http";

const port = Number(process.argv[2] ?? 7801);
const tasks = new Map(); // id -> { at, seconds, lyrics, seed }
let next = 0;
let down = false; // POST /fake/down with true or false: the health check fails while true, as a stopped engine's would

const wrap = (data) => JSON.stringify({ data, code: 200, error: null, timestamp: Date.now(), extra: null });

// A mono 16-bit 8 kHz WAV of a 440 Hz tone; small, and enough for ffmpeg to import.
function wav(seconds) {
  const rate = 8000, n = Math.round(rate * seconds), buf = Buffer.alloc(44 + n * 2);
  buf.write("RIFF", 0); buf.writeUInt32LE(36 + n * 2, 4); buf.write("WAVEfmt ", 8);
  buf.writeUInt32LE(16, 16); buf.writeUInt16LE(1, 20); buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(rate, 24); buf.writeUInt32LE(rate * 2, 28); buf.writeUInt16LE(2, 32); buf.writeUInt16LE(16, 34);
  buf.write("data", 36); buf.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) buf.writeInt16LE(Math.round(8000 * Math.sin((2 * Math.PI * 440 * i) / rate)), 44 + i * 2);
  return buf;
}

function body(req) {
  return new Promise((ok) => { let s = ""; req.on("data", (c) => (s += c)); req.on("end", () => ok(s ? JSON.parse(s) : {})); });
}

http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  const json = (data) => { res.writeHead(200, { "content-type": "application/json" }); res.end(wrap(data)); };
  if (url.pathname === "/health" && down) { res.writeHead(503); return res.end(); }
  if (url.pathname === "/health") return json({ status: "ok", service: "ACE-Step API", version: "1.0", models_initialized: true, llm_initialized: false, loaded_model: "acestep-v15-turbo", loaded_lm_model: null });
  if (url.pathname === "/v1/stats") return json({ jobs: { queued: 0, running: 0 }, avg_job_seconds: 2 });
  if (url.pathname === "/v1/model_inventory") return json({ models: [{ name: "acestep-v15-turbo", is_loaded: true }] });
  if (url.pathname === "/v1/audio") {
    const t = tasks.get(url.searchParams.get("path"));
    if (!t) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { "content-type": "audio/wav" });
    return res.end(wav(Math.min(t.seconds, 20)));
  }
  if (req.method !== "POST") { res.writeHead(404); return res.end(); }
  const b = await body(req);
  if (url.pathname === "/fake/down") { down = b === true; return json(down); }
  if (url.pathname === "/v1/create_sample") return json({ lyrics: "[verse]\nA fake song\n[chorus]\nSung by a sine", bpm: 90, keyscale: "A minor", timesignature: "4" });
  if (url.pathname === "/v1/init") return json({ loaded_model: b.model });
  if (url.pathname === "/release_task") {
    const id = `task${++next}`;
    tasks.set(id, { at: Date.now(), seconds: b.audio_duration ?? 30, lyrics: b.lyrics ?? "", seed: b.seed ?? 1 });
    return json({ task_id: id, status: "queued", queue_position: 1 });
  }
  if (url.pathname === "/query_result") {
    // ACE-Step's own shapes (query_result_service.py): an unknown id is "[]" with status 0; a song being made
    // carries its progress and stage.
    return json((b.task_id_list ?? []).map((id) => {
      const t = tasks.get(id);
      if (!t) return { task_id: id, status: 0, result: "[]" };
      const took = t.seconds * 50, done = Date.now() - t.at > took; // 5 % of real time, as a fast GPU would
      const result = done
        ? JSON.stringify([{ file: `/v1/audio?path=${id}`, status: 1, lyrics: t.lyrics, seed_value: String(t.seed), metas: { bpm: 90, keyscale: "A minor", timesignature: "4", duration: Math.min(t.seconds, 20) } }])
        : JSON.stringify([{ file: "", status: 0, progress: Math.min(0.99, (Date.now() - t.at) / took), stage: "running" }]);
      return { task_id: id, status: done ? 1 : 0, result };
    }));
  }
  res.writeHead(404); res.end();
}).listen(port, "127.0.0.1", () => console.log(`fake acestep-api on ${port}`));
