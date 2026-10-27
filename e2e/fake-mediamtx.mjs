// A fake MediaMTX control API for the browser tests: the two calls Maio's relay check makes (arc42 6.5), with no
// stream on it. POST /down makes it stop answering, POST /up brings it back.
import http from "node:http";

const port = Number(process.argv[2] ?? 9997);
const started = new Date().toISOString();
let up = true;

http.createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  if (req.method === "POST" && (url.pathname === "/down" || url.pathname === "/up")) {
    up = url.pathname === "/up";
    res.writeHead(204);
    return res.end();
  }
  const json = (data) => { res.writeHead(200, { "content-type": "application/json" }); res.end(JSON.stringify(data)); };
  if (!up) { res.writeHead(503); return res.end(); }
  if (url.pathname === "/v3/info") return json({ version: "v1.21.2", started });
  if (url.pathname === "/v3/paths/list") return json({ items: [] });
  res.writeHead(404); res.end();
}).listen(port, "127.0.0.1", () => console.log(`fake MediaMTX control API on ${port}`));
