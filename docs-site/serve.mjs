import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { build, outputRoot } from './build.mjs';

await build();
const host = process.env.DOCS_HOST ?? '0.0.0.0';
const port = Number(process.env.DOCS_PORT ?? 8000);
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('DOCS_PORT must be between 1 and 65535.');
const types = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.json': 'application/json; charset=utf-8', '.md': 'text/plain; charset=utf-8', '.svg': 'image/svg+xml' };
const server = createServer(async (request, response) => {
  if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405, { Allow: 'GET, HEAD' }); response.end(); return; }
  try {
    const url = new URL(request.url, 'http://localhost');
    const filename = path.resolve(outputRoot, `.${decodeURIComponent(url.pathname)}`);
    if (filename !== outputRoot && !filename.startsWith(outputRoot + path.sep)) { response.writeHead(403); response.end('Forbidden'); return; }
    const file = (await stat(filename)).isDirectory() ? path.join(filename, 'index.html') : filename;
    const data = await readFile(file);
    response.writeHead(200, { 'Content-Type': types[path.extname(file)] ?? 'application/octet-stream', 'Content-Length': data.length, 'Cache-Control': 'no-cache', 'X-Content-Type-Options': 'nosniff' });
    response.end(request.method === 'HEAD' ? undefined : data);
  } catch (error) {
    response.writeHead(error.code === 'ENOENT' || error.code === 'ENOTDIR' ? 404 : 400);
    response.end('Document not found');
  }
});
server.on('error', error => { console.error(`Cannot listen on ${host}:${port}: ${error.message}`); process.exitCode = 1; });
server.listen(port, host, () => { console.log(`The architecture website: http://${host}:${port}`); console.log(`Open http://localhost:${port} on this machine.`); });
