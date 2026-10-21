// maio's C4 model: the elements, and the views that place them. The explorer and the previews in the
// documentation both draw from here. `doc` points into the arc42 documentation (document id, optional section).
const element = (name, type, description, options = {}) => ({ name, type, description, ...options });
const A = 'architecture/';

export const elements = {
  you: element('You', 'Person', 'The owner. Makes songs, keeps them, builds playlists, plays them and puts them on air.', { external: true, responsibilities: ['Make songs from prompts', 'Build and order playlists', 'Go live, public or private'], doc: `${A}01-introduction-and-goals?section=14-stakeholders` }),
  scripts: element('Scripts', 'Person', 'Programs with the API token. Anything the web app does, they can do through the JSON API.', { external: true, responsibilities: ['Call /api/v1 with a bearer token'], doc: `${A}03-system-scope-and-context?section=35-the-http-api` }),
  listeners: element('Listeners', 'Person', 'Anyone with a live playlist\'s address, in any player, with no account.', { external: true, responsibilities: ['Pull MP3, AAC or HLS from maio', 'Pull RTSP, SRT or RTMP from MediaMTX'], doc: `${A}08-crosscutting-concepts?section=84-live` }),
  maio: element('maio', 'Software system', 'Makes songs through music engines, keeps them in a library, builds playlists, plays them and streams live playlists.', { drill: 'containers', responsibilities: ['Make songs, one prompt each', 'Keep the library and playlists', 'Play on demand and stream live'], doc: `${A}01-introduction-and-goals?section=11-what-maio-is` }),
  acestep: element('ACE-Step 1.5', 'External system', 'acestep-api on the Mac, with Metal. Makes the music, can write lyrics, reports progress and models.', { external: true, responsibilities: ['Generate songs', 'Write lyrics with its planning model', 'Load and switch models'], doc: `${A}03-system-scope-and-context?section=33-the-music-engine-ace-step-15` }),
  textengines: element('Text engines', 'External system', 'Anthropic or any OpenAI-compatible chat API. They write lyrics, titles and descriptions, never music.', { external: true, doc: `${A}03-system-scope-and-context?section=34-text-engines` }),
  otel: element('OpenTelemetry collector', 'External system', 'Optional. Receives traces, metrics and logs over OTLP/HTTP when an endpoint is set.', { external: true, optional: true, doc: `${A}08-crosscutting-concepts?section=86-logs-traces-and-metrics` }),

  webapp: element('Web app', 'Container · Leptos', 'Rendered by maio, then hydrated in your browser. Calls /api/v1 like any script.', { drill: 'webapp', responsibilities: ['Songs and Playlists screens', 'The player', 'Engine and MediaMTX panels'], doc: `${A}08-crosscutting-concepts?section=85-the-web-app` }),
  server: element('maio', 'Container · Rust', 'One binary with ffmpeg on :8080: the web app, the JSON API, events, the jobs, the live playout and its streams.', { drill: 'components', responsibilities: ['Serve the web app, API and streams', 'Run jobs against the engines', 'Play live playlists'], doc: `${A}07-deployment-view?section=71-nodes` }),
  mediamtx: element('MediaMTX', 'Container · MediaMTX', 'Relays live playlists over RTSP, SRT and RTMP; asks maio who may read or publish.', { responsibilities: ['Carry AAC per live path', 'Ask /relay/auth about every reader', 'Report its state on :9997'], doc: 'adr/0007-live-playout-on-the-server-mediamtx-as-relay' }),
  db: element('Library', 'Data store · SQLite', 'library.db: songs, playlists, the account and sessions.', { doc: `${A}07-deployment-view?section=72-the-data-volume` }),
  files: element('Song files', 'Data store · files', 'Loudness-normalised song files with tags, Opus by default; HLS segments and the song on air.', { doc: `${A}08-crosscutting-concepts?section=what-a-song-carries` }),

  web: element('Web layer', 'Component · axum', 'The JSON API, server-sent events, streams, the relay auth hook and pages. Maps errors to statuses; no rules here.', { responsibilities: ['Check sessions, token and Origin', 'Turn requests into use cases', 'Serve streams and pages'], doc: `${A}05-building-block-view?section=51-level-1-the-layers` }),
  usecases: element('Use cases', 'Component · application', 'One per action: make a song, cancel, rename, delete, playlists, go live, sign in. Own the ports.', { responsibilities: ['Apply limits and licences', 'Run jobs in engine order', 'Keep live streams in step with playlists'], doc: `${A}05-building-block-view?section=52-level-2-the-ports` }),
  domain: element('Domain', 'Component · domain', 'Songs, playlists and their rules, song requests, titles in layers, engine choice, the live order. No I/O.', { responsibilities: ['Express the rules', 'Stay free of HTTP, SQLite, ffmpeg and Leptos'], doc: 'adr/0002-a-hexagon-in-one-crate' }),
  sqlite: element('SQLite adapter', 'Component · adapter', 'SongRepository, PlaylistRepository and AccountStore over rusqlite.', { doc: `${A}05-building-block-view?section=52-level-2-the-ports` }),
  engine: element('Engine client', 'Component · adapter', 'MusicEngine over acestep-api: health every 30 s, release_task, query_result, audio, model switch; traceparent.', { doc: `${A}03-system-scope-and-context?section=33-the-music-engine-ace-step-15` }),
  text: element('Text engine clients', 'Component · adapter', 'TextEngine over the Anthropic and OpenAI-compatible APIs.', { doc: `${A}03-system-scope-and-context?section=34-text-engines` }),
  ffmpeg: element('Audio files', 'Component · adapter', 'AudioFiles with ffmpeg: import to -16 LUFS, tags, duration, export to five formats.', { doc: `${A}08-crosscutting-concepts?section=what-a-song-carries` }),
  live: element('Live stations', 'Component · adapter', 'LiveStreams: one station per live playlist, its encoders, and the MediaMTX relay.', { drill: 'live', responsibilities: ['Play out each live playlist', 'Encode MP3, AAC and HLS', 'Publish to and watch MediaMTX'], doc: `${A}05-building-block-view?section=53-level-3-inside-the-bigger-adapters` }),

  playout: element('Playout', 'Component · live/station.rs', 'Picks songs in the live order, decodes each while it plays, paces 20 ms PCM chunks; remembers the song on air.', { doc: `${A}06-runtime-view?section=63-going-live-and-playing-out` }),
  mixer: element('Mixer', 'Component · live/mixer.rs', 'Two decks with an equal-power crossfade, 6 s by default.', { doc: `${A}05-building-block-view?section=53-level-3-inside-the-bigger-adapters` }),
  encoders: element('Encoders', 'Component · ffmpeg', 'One per format, fed from the PCM bus: MP3, AAC and HLS, with ICY titles for players that ask.', { doc: `${A}08-crosscutting-concepts?section=84-live` }),
  relay: element('Relay', 'Component · live/relay.rs', 'Publishes AAC per live path, answers MediaMTX\'s auth hook, checks its control API every 5 s.', { doc: `${A}06-runtime-view?section=65-watching-mediamtx` }),
  streams: element('Stream server', 'Component · web/streams.rs', '/live/{playlist}/… and /listen/{key}/…: MP3, AAC and HLS; counts listeners; ends streams when the address moves.', { doc: `${A}08-crosscutting-concepts?section=84-live` }),

  frame: element('Header', 'Component · ui/frame.rs', 'Tabs, the engine line and panel, the MediaMTX line and panel, the theme switch and the account menu.', { doc: `${A}08-crosscutting-concepts?section=85-the-web-app` }),
  screens: element('Songs and Playlists', 'Component · ui/songs.rs', 'The shelf, Make a song, Being made, the song list with search; Playlists as rows.', { doc: `${A}08-crosscutting-concepts?section=85-the-web-app` }),
  playlistpage: element('Playlist page', 'Component · ui/playlist.rs', 'The band with play and shuffle play, the live strip, the songs with drag and arrow keys.', { doc: `${A}08-crosscutting-concepts?section=85-the-web-app` }),
  player: element('Player', 'Component · ui/player.rs', 'One <audio> for the whole app: songs on demand, or a live playlist\'s stream.', { doc: `${A}08-crosscutting-concepts?section=83-playlists-and-the-player` }),
  dialogs: element('Dialogs and details', 'Component · ui/modal.rs', 'The browser\'s own <dialog>: new playlist, rename, delete, add songs, song details.', { doc: 'adr/0010-native-browser-features-first' }),
  deployServer: element('maio', 'Container instance · Docker', 'host 8080 → 8080. Config /etc/maio/maio.toml, read-only; data on /data.', { responsibilities: ['Serve the web app, API and streams', 'Publish live playlists to MediaMTX'], doc: `${A}07-deployment-view?section=71-nodes` }),
  deployMediamtx: element('MediaMTX', 'Container instance · Docker', 'host 8555 → 8554 RTSP, 8891 → 8890/udp SRT, 1936 → 1935 RTMP; control API :9997 on the network only.', { doc: `${A}07-deployment-view?section=71-nodes` }),
  deployData: element('Data volume', 'Volume · ./data → /data', 'library.db, songs/, tmp/, live/{playlist}/hls and on-air.', { doc: `${A}07-deployment-view?section=72-the-data-volume` }),
  deployAcestep: element('ACE-Step 1.5', 'Process · acestep-api', '127.0.0.1:7801 with Metal, from the checkout beside this repo; started by just engine.', { external: true, doc: `${A}03-system-scope-and-context?section=33-the-music-engine-ace-step-15` }),
  client: element('API client', 'Component · ui/api.rs', 'Typed calls to /api/v1; server-sent events nudge lists to reload.', { doc: 'adr/0004-the-web-app-uses-only-the-public-api' }),
};

const node = (id, x, y) => ({ id, x, y, width: 250, height: 156 });
const edge = (from, to, label, options = {}) => ({ from, to, label, ...options });

export const views = {
  context: {
    title: 'System context', level: 'C4 · Level 1', scope: 'maio', crumb: 'context',
    description: 'Who uses maio, and which systems surround it?',
    hint: 'Double-click maio to explore its containers.',
    nodes: [node('you', 30, 60), node('scripts', 30, 420), node('maio', 480, 240), node('listeners', 930, 60), node('acestep', 930, 420), node('textengines', 480, 560), node('otel', 930, 700)],
    edges: [edge('you', 'maio', 'Makes, plays, goes live', { channel: 'input' }), edge('scripts', 'maio', 'Same, by API'), edge('maio', 'acestep', 'Song requests', { channel: 'input' }), edge('acestep', 'maio', 'The audio, lyrics', { channel: 'output' }), edge('maio', 'listeners', 'Live streams', { channel: 'output' }), edge('maio', 'textengines', 'Lyrics, titles'), edge('maio', 'otel', 'Traces, metrics, logs', { dashed: true })],
    flows: [{ id: 'input', legs: [{ from: 'you', to: 'maio' }, { from: 'maio', to: 'acestep' }] }, { id: 'output', legs: [{ from: 'acestep', to: 'maio' }, { from: 'maio', to: 'listeners' }] }],
    parent: null,
  },
  containers: {
    title: 'Containers', level: 'C4 · Level 2', scope: 'Inside maio', crumb: 'containers',
    description: 'One Rust container and MediaMTX in Docker; the web app runs in your browser.',
    hint: 'Double-click maio for its components, or the web app for its parts.',
    nodes: [node('you', 20, 90), node('scripts', 20, 440), node('webapp', 420, 90), node('server', 800, 330), node('mediamtx', 1180, 90), node('db', 610, 610), node('files', 990, 610), node('listeners', 1600, 90), node('acestep', 1600, 400), node('textengines', 1600, 680)],
    edges: [edge('you', 'webapp', 'Uses', { channel: 'input' }), edge('webapp', 'server', 'Everything · /api/v1, SSE', { channel: 'input' }), edge('scripts', 'server', 'Everything · token'), edge('server', 'mediamtx', 'Publishes AAC', { channel: 'output' }), edge('mediamtx', 'server', 'Who may read? /relay/auth'), edge('server', 'listeners', 'MP3 · AAC · HLS', { channel: 'output' }), edge('mediamtx', 'listeners', 'RTSP · SRT · RTMP', { channel: 'output' }), edge('server', 'db', 'Reads, writes'), edge('server', 'files', 'Song files'), edge('server', 'acestep', 'Makes songs · HTTP', { channel: 'input' }), edge('acestep', 'server', 'The audio', { channel: 'output' }), edge('server', 'textengines', 'Words · HTTPS')],
    flows: [{ id: 'input', legs: [{ from: 'you', to: 'webapp' }, { from: 'webapp', to: 'server' }, { from: 'server', to: 'acestep' }] }, { id: 'output', legs: [{ from: 'acestep', to: 'server' }, { from: 'server', to: 'mediamtx' }, { from: 'server', to: 'listeners' }, { from: 'mediamtx', to: 'listeners' }] }],
    boundary: { x: 380, y: 40, width: 1090, height: 770, label: 'maio · Docker compose on the Mac' }, parent: 'context',
  },
  components: {
    title: 'Components', level: 'C4 · Level 3', scope: 'Inside maio', crumb: 'components',
    description: 'A hexagon in one crate: domain ← use cases ← adapters ← web.',
    hint: 'Dashed arrows: an adapter implements a port the use cases own. Double-click Live stations for the playout.',
    nodes: [node('web', 610, 40), node('usecases', 420, 320), node('domain', 800, 320), node('sqlite', 20, 620), node('engine', 320, 620), node('text', 620, 620), node('ffmpeg', 920, 620), node('live', 1220, 620), node('db', 20, 920), node('acestep', 320, 920), node('textengines', 620, 920), node('files', 920, 920), node('mediamtx', 1220, 920)],
    edges: [edge('web', 'usecases', 'Calls'), edge('usecases', 'domain', 'Uses'), edge('sqlite', 'usecases', 'Repositories', { dashed: true }), edge('engine', 'usecases', 'MusicEngine', { dashed: true }), edge('text', 'usecases', 'TextEngine', { dashed: true }), edge('ffmpeg', 'usecases', 'AudioFiles', { dashed: true }), edge('live', 'usecases', 'LiveStreams', { dashed: true }), edge('sqlite', 'db', 'SQL'), edge('engine', 'acestep', 'HTTP'), edge('text', 'textengines', 'HTTPS'), edge('ffmpeg', 'files', 'Writes'), edge('live', 'mediamtx', 'Publishes, checks')],
    boundary: { x: -20, y: -10, width: 1530, height: 830, label: 'maio · one Rust binary' }, parent: 'containers',
  },
  live: {
    title: 'Live stations', level: 'C4 · Level 3', scope: 'Inside Live stations', crumb: 'components', deeper: true,
    description: 'One station per live playlist: playout, mixer, encoders and the relay.',
    hint: 'The animation follows the audio from the song files to the listeners.',
    nodes: [node('files', 20, 330), node('playout', 400, 330), node('mixer', 780, 330), node('encoders', 1160, 130), node('relay', 1160, 530), node('streams', 1540, 130), node('mediamtx', 1540, 530), node('listeners', 1920, 330)],
    edges: [edge('playout', 'files', 'Decodes songs'), edge('playout', 'mixer', 'Decks', { channel: 'output' }), edge('mixer', 'encoders', 'PCM bus', { channel: 'output' }), edge('mixer', 'relay', 'PCM bus', { channel: 'output' }), edge('encoders', 'streams', 'MP3, AAC, HLS', { channel: 'output' }), edge('relay', 'mediamtx', 'AAC · checks · auth', { channel: 'output' }), edge('listeners', 'streams', 'Tune in'), edge('listeners', 'mediamtx', 'Tune in')],
    flows: [{ id: 'output', legs: [{ from: 'playout', to: 'mixer' }, { from: 'mixer', to: 'encoders' }, { from: 'mixer', to: 'relay' }, { from: 'encoders', to: 'streams' }, { from: 'relay', to: 'mediamtx' }] }],
    boundary: { x: 360, y: 80, width: 1090, height: 650, label: 'Live stations · adapters/live' }, parent: 'components',
  },
  webapp: {
    title: 'Web app', level: 'C4 · Level 3', scope: 'Inside the web app', crumb: 'components', deeper: true,
    description: 'Leptos components in your browser; they reach the server only through /api/v1.',
    hint: 'Every screen goes through the API client, as a script would.',
    nodes: [node('frame', 20, 40), node('screens', 330, 40), node('playlistpage', 640, 40), node('dialogs', 950, 40), node('player', 330, 360), node('client', 800, 360), node('server', 560, 680)],
    edges: [edge('frame', 'client', 'Status, models'), edge('screens', 'client', 'Songs, playlists, jobs'), edge('playlistpage', 'client', 'Playlist, live'), edge('dialogs', 'client', 'Create, rename, delete'), edge('screens', 'player', 'Play'), edge('playlistpage', 'player', 'Play, shuffle play'), edge('client', 'server', '/api/v1, SSE'), edge('player', 'server', 'Audio, live stream')],
    boundary: { x: -20, y: -10, width: 1260, height: 590, label: 'Web app · Leptos, hydrated in the browser' }, parent: 'containers',
  },
  deployment: {
    title: 'Deployment', level: 'C4 · Deployment', scope: 'The Mac', crumb: 'containers', deeper: true,
    description: 'Where each part runs: Docker compose on the Mac, ACE-Step beside it.',
    hint: 'The ports are the ones listeners and browsers use; MediaMTX\'s control API stays inside Docker.',
    nodes: [node('you', 20, 120), node('listeners', 20, 520), node('deployServer', 470, 130), node('deployMediamtx', 890, 300), node('deployData', 890, 560), node('deployAcestep', 1400, 130)],
    edges: [edge('you', 'deployServer', 'HTTP · 8080'), edge('listeners', 'deployServer', 'MP3 · AAC · HLS · 8080'), edge('listeners', 'deployMediamtx', 'RTSP 8555 · SRT 8891 · RTMP 1936'), edge('deployServer', 'deployMediamtx', 'Publishes AAC'), edge('deployMediamtx', 'deployServer', 'POST /relay/auth'), edge('deployServer', 'deployData', 'Reads, writes'), edge('deployServer', 'deployAcestep', 'host.docker.internal:7801')],
    boundaries: [{ x: 380, y: 20, width: 1360, height: 740, label: 'The Mac' }, { x: 420, y: 70, width: 800, height: 660, label: 'Docker compose · maio' }], parent: 'containers',
  },
};

/** A view's boundaries, outermost first. */
export function boundariesOf(view) {
  return view.boundaries ?? (view.boundary ? [view.boundary] : []);
}

export function resolveView(id) {
  return views[id] ?? views.context;
}

export function connectedIds(view, id) {
  return new Set([id, ...view.edges.filter(e => e.from === id || e.to === id).flatMap(e => [e.from, e.to])]);
}

/** The animated flows a view declares; each leg must be one of its edges. */
export function flowChannelsFor(view) {
  const flows = view.flows ?? [];
  return flows.every(flow => flow.legs.every(leg => view.edges.some(e => e.from === leg.from && e.to === leg.to))) ? flows : [];
}

export function boundsFor(view) {
  const boxes = [...view.nodes, ...boundariesOf(view)];
  const left = Math.min(...boxes.map(n => n.x)) - 55;
  const top = Math.min(...boxes.map(n => n.y)) - 60;
  const right = Math.max(...boxes.map(n => n.x + n.width)) + 55;
  const bottom = Math.max(...boxes.map(n => n.y + n.height)) + 55;
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function nodeShape(node) {
  if (elements[node.id]?.type === 'Person') return {
    body: { x: (node.width - 190) / 2, y: 52, width: 190, height: 96, rx: 32 },
    head: { cx: node.width / 2, cy: 18, r: 42 },
  };
  return { body: { x: 0, y: 0, width: node.width, height: node.height, rx: 10 } };
}

export function edgeGeometry(from, to, lane = 0) {
  const connectionBox = n => {
    const { body } = nodeShape(n);
    return { ...n, x: n.x + body.x, y: n.y + body.y, width: body.width, height: body.height };
  };
  from = connectionBox(from); to = connectionBox(to);
  const center = n => ({ x: n.x + n.width / 2, y: n.y + n.height / 2 });
  const a = center(from), b = center(to);
  const dx = b.x - a.x, dy = b.y - a.y;
  const distance = Math.hypot(dx, dy) || 1, nx = -dy / distance, ny = dx / distance;
  a.x += nx * lane; a.y += ny * lane; b.x += nx * lane; b.y += ny * lane;
  const intersect = (n, c, sign) => {
    const vx = sign * dx, vy = sign * dy;
    const tx = vx > 0 ? (n.x + n.width - c.x) / vx : vx < 0 ? (n.x - c.x) / vx : Infinity;
    const ty = vy > 0 ? (n.y + n.height - c.y) / vy : vy < 0 ? (n.y - c.y) / vy : Infinity;
    const t = Number.isFinite(Math.min(tx, ty)) ? Math.min(tx, ty) : 0;
    return { x: c.x + sign * dx * t, y: c.y + sign * dy * t };
  };
  const start = intersect(from, a, 1), end = intersect(to, b, -1);
  return { start, end, label: { x: (start.x + end.x) / 2 + (lane ? nx * 14 : 0), y: (start.y + end.y) / 2 + (lane ? ny * 14 : -10) } };
}

export function relationshipGeometry(view, edge) {
  const nodes = new Map(view.nodes.map(n => [n.id, n]));
  const paired = view.edges.some(other => other.from === edge.to && other.to === edge.from);
  return edgeGeometry(nodes.get(edge.from), nodes.get(edge.to), paired ? 20 : 0);
}

export function wrapText(value, max = 33, lines = 3) {
  const result = [];
  let current = '';
  for (const word of value.split(/\s+/)) {
    if (current && (current + ' ' + word).length > max) { result.push(current); current = word; }
    else current += (current ? ' ' : '') + word;
  }
  if (current) result.push(current);
  if (result.length > lines) { result.length = lines; result[lines - 1] = result[lines - 1].replace(/[.,;:]$/, '') + '…'; }
  return result;
}
