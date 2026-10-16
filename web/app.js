'use strict';
/* Maio web UI: the Dial design. One file, a section per screen; data goes in as text only. */
const $ = (sel, root = document) => root.querySelector(sel);
const { mmss, ago, span, dialSlots, pickRadio, kindLabel, sizeLine, playlistAbout, credits, asker, upNext, songNote } = UI;
const SCOPES = { admin: ['read', 'queue', 'control', 'admin'], operator: ['read', 'queue', 'control'], listener: ['read'] };
const svg = (body, attrs = 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"') =>
  `<svg width="18" height="18" viewBox="0 0 24 24" ${attrs} aria-hidden="true">${body}</svg>`;
const ICON = {
  play: svg('<path d="M8 5l11 7-11 7z"></path>', 'fill="currentColor"'),
  pause: svg('<rect x="6" y="5" width="4" height="14" rx="1"></rect><rect x="14" y="5" width="4" height="14" rx="1"></rect>', 'fill="currentColor"'),
  sound: svg('<path d="M4 10v4h4l5 4V6L8 10H4z"></path><path d="M16.5 9a4 4 0 0 1 0 6"></path>'),
  muted: svg('<path d="M4 10v4h4l5 4V6L8 10H4z"></path><path d="M16 9l5 6M21 9l-5 6"></path>'),
  x: svg('<path d="M6 6l12 12M18 6L6 18"></path>'),
  heart: (on) => svg('<path d="M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z"></path>', `fill="${on ? 'currentColor' : 'none'}" stroke="currentColor" stroke-width="2" stroke-linejoin="round"`),
  left: svg('<path d="M15 6l-6 6 6 6"></path>'),
  right: svg('<path d="M9 6l6 6-6 6"></path>'),
  sun: svg('<circle cx="12" cy="12" r="4.5"></circle><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"></path>'),
  moon: svg('<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"></path>'),
  sliders: svg('<path d="M4 7h10M18 7h2M4 17h4M12 17h8"></path><circle cx="16" cy="7" r="2"></circle><circle cx="10" cy="17" r="2"></circle>'),
  lock: svg('<path d="M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z"></path>'),
  globe: svg('<path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"></path><circle cx="12" cy="12" r="9"></circle>'),
  grip: svg('<path d="M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01"></path>', 'fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"'),
};

let me = null;
const S = { radios: [], maxRadios: 4, tuned: 'main', status: null, playlists: [], events: null };

/* ---------- DOM helpers: data only ever goes in as text ---------- */
function h(tag, attrs = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k === 'style') el.style.cssText = v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  return el;
}
/** Fixed icon markup only (never data). */
function icon(markup) { const t = document.createElement('template'); t.innerHTML = markup; return t.content.firstChild; }
function toast(msg) { const t = $('#toast'); t.textContent = msg; t.hidden = false; clearTimeout(toast.t); toast.t = setTimeout(() => (t.hidden = true), 5000); }
const can = (scope) => !!me && SCOPES[me.role]?.includes(scope);
const need = (scope, el) => (can(scope) ? el : null);

/* ---------- API ---------- */
async function api(method, path, body) {
  const res = await fetch('/api/v1' + path, {
    method, credentials: 'same-origin',
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (res.status === 401 && path !== '/session') { showSignIn(); throw Object.assign(new Error('signed out'), { status: 401 }); }
  if (!res.ok) {
    const err = Object.assign(new Error(data?.error?.message || res.statusText), { status: res.status, code: data?.error?.code });
    if (err.code !== 'noncommercial_engine') toast(err.message);
    throw err;
  }
  return data;
}

/* ---------- theme ---------- */
const isDark = () => (document.documentElement.dataset.theme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
function paintThemeButtons() {
  for (const b of document.querySelectorAll('[data-theme-toggle]')) {
    const label = isDark() ? 'Switch to light theme' : 'Switch to dark theme';
    b.replaceChildren(icon(isDark() ? ICON.sun : ICON.moon));
    b.setAttribute('aria-label', label);
    b.title = label;
  }
}
function toggleTheme() {
  const next = isDark() ? 'light' : 'dark';
  document.documentElement.dataset.theme = next;
  try { localStorage.setItem('maio-theme', next); } catch {}
  paintThemeButtons();
}

/* ---------- doors ---------- */
function paintKnobs() {
  for (const face of document.querySelectorAll('[data-grip]')) {
    if (face.childElementCount) continue;
    const n = Number(face.dataset.grip);
    for (let i = 0; i < n; i++) face.append(h('i', { style: `transform: rotate(${(i * 360) / n}deg)` }));
    face.append(h('b'));
  }
}
function show(id) { for (const s of ['signin', 'change', 'app']) $('#' + s).hidden = s !== id; }
function showSignIn() {
  const first = $('#signin').hidden;
  me = null; S.events?.close(); S.events = null; clearInterval(S.poll);
  stopAudio?.();
  show('signin');
  if (first) $('#signin-form [name=username]').focus();
}
function showChange(forced) {
  $('#change-title').textContent = forced ? 'Choose a password' : 'Change your password';
  $('#change-help').textContent = forced ? 'An admin gave you a temporary password. Pick your own: 10 characters or more.' : 'Pick a new password: 10 characters or more.';
  $('#current-label').textContent = forced ? 'Temporary password' : 'Current password';
  $('#change-cancel').hidden = forced;
  show('change');
  $('#change-form [name=current]').focus();
}

/* ---------- radios and playlists, shared by every screen ---------- */
const radioPath = (p, id = S.tuned) => `/radios/${encodeURIComponent(id)}${p}`;
const tunedRadio = () => S.radios.find((r) => r.id === S.tuned);
async function loadRadios() {
  const res = await api('GET', '/radios');
  S.radios = res.radios;
  S.maxRadios = res.max_radios;
  let wanted = null;
  try { wanted = localStorage.getItem('maio-radio'); } catch {}
  const pick = pickRadio(S.radios, S.tuned === 'main' && wanted ? wanted : S.tuned);
  if (pick !== S.tuned) tune(pick);
}
async function loadPlaylists() { S.playlists = (await api('GET', '/playlists')).playlists; }
const playlistName = (id) => S.playlists.find((p) => p.id === id)?.name;

function tune(id) {
  if (!id || id === S.tuned) return;
  S.tuned = id;
  try { localStorage.setItem('maio-radio', id); } catch {}
  connectEvents();
  VIEWS.now?.refresh?.();
  if (!$('#audio').paused) startAudio?.();
}

/** One event stream, for the tuned radio; every visible view hears it. */
function connectEvents() {
  S.events?.close();
  S.events = new EventSource(`/api/v1${radioPath('/events')}`);
  for (const kind of ['now_playing', 'queue', 'on_air', 'playlists', 'radios', 'job']) {
    S.events.addEventListener(kind, (e) => {
      const data = e.data ? JSON.parse(e.data) : null;
      if (kind === 'radios' && Array.isArray(data)) {
        S.radios = data;
        const to = UI.retuneTo(S.radios, S.tuned); // the tuned radio was deleted: main instead
        if (to) return tune(to);
      }
      for (const v of Object.values(VIEWS)) v.onEvent?.(kind, data);
    });
  }
}

/* ---------- router ---------- */
const VIEWS = {};
let current = null;
function go(hash) { if (location.hash === hash) route(); else location.hash = hash; }
function route() {
  if (!me) return;
  const [path, query = ''] = location.hash.replace(/^#\/?/, '').split('?');
  const [name, ...rest] = path.split('/');
  const view = VIEWS[name] && (name !== 'users' || me.role === 'admin') ? name : 'now';
  current = view;
  for (const s of document.querySelectorAll('[data-view]')) s.hidden = s.dataset.view !== view;
  const tab = view === 'playlist' ? 'playlists' : view;
  for (const a of document.querySelectorAll('[data-tab]')) a.toggleAttribute('aria-current', a.dataset.tab === tab), a.dataset.tab === tab && a.setAttribute('aria-current', 'page');
  VIEWS[view].show(decodeURIComponent(rest.join('/')), new URLSearchParams(query));
}
window.addEventListener('hashchange', route);
const visible = (name) => current === name && !$('#app').hidden;

/* ---------- dialog: one form for every small question ---------- */
function ask({ title, fields, submit = 'Save', danger = false }) {
  const dlg = $('#dialog');
  return new Promise((resolve) => {
    const inputs = {};
    const rows = fields.map((f) => {
      let input;
      if (f.type === 'select') input = h('select', { name: f.name, class: 'field' }, ...f.options.map(([v, label]) => h('option', { value: v, selected: String(v) === String(f.value ?? '') }, label)));
      else if (f.type === 'checkbox') return h('label', { class: 'check' }, (inputs[f.name] = h('input', { type: 'checkbox', name: f.name, checked: !!f.value })), f.label);
      else if (f.type === 'textarea') input = h('textarea', { name: f.name, class: 'field', rows: '3' }, f.value ?? '');
      else input = h('input', { name: f.name, class: 'field', type: f.type || 'text', value: f.value ?? '', required: !!f.required, minlength: f.minlength, maxlength: f.maxlength, min: f.min, max: f.max, placeholder: f.placeholder, autocomplete: 'off' });
      inputs[f.name] = input;
      return h('label', { class: 'field-label' }, f.label, input, f.help ? h('span', { class: 'muted' }, f.help) : null);
    });
    const form = h('form', { method: 'dialog' },
      h('h2', { id: 'dialog-title', class: 'h2' }, title),
      ...rows,
      h('div', { class: 'row-gap' },
        h('button', { class: danger ? 'btn danger' : 'btn primary', value: 'ok' }, submit),
        h('button', { class: 'btn', value: 'cancel', formnovalidate: true }, 'Cancel')));
    dlg.replaceChildren(form);
    dlg.onclose = () => {
      if (dlg.returnValue !== 'ok') return resolve(null);
      const out = {};
      for (const [k, el] of Object.entries(inputs)) out[k] = el.type === 'checkbox' ? el.checked : el.value.trim();
      resolve(out);
    };
    dlg.returnValue = '';
    dlg.showModal();
    form.querySelector('input, select, textarea')?.focus();
  });
}

/* ---------- preview: one song in this browser only, off air ---------- */
let previewing = null;
function preview(track, button) {
  const p = $('#preview');
  const stop = () => { p.pause(); p.removeAttribute('src'); previewing?.button?.replaceChildren(icon(ICON.play)); previewing?.button?.setAttribute('aria-pressed', 'false'); previewing = null; };
  if (previewing?.id === track.id) return stop();
  stop();
  if (!$('#audio').paused) stopAudio();
  p.src = `/api/v1/library/${encodeURIComponent(track.id)}/audio`;
  p.play().catch(() => toast('The browser blocked playback. Press play again.'));
  previewing = { id: track.id, button };
  button?.replaceChildren(icon(ICON.pause));
  button?.setAttribute('aria-pressed', 'true');
  p.onended = stop;
}
const previewButton = (t) => {
  const b = h('button', { type: 'button', class: 'icon', 'aria-label': `Preview ${t.title} in this browser`, 'aria-pressed': 'false' });
  b.append(icon(previewing?.id === t.id ? ICON.pause : ICON.play));
  b.addEventListener('click', () => preview(t, b));
  return b;
};

/* ---------- boot ---------- */
async function boot() {
  paintKnobs();
  paintThemeButtons();
  for (const b of document.querySelectorAll('[data-theme-toggle]')) b.addEventListener('click', toggleTheme);
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', paintThemeButtons);
  try { me = await api('GET', '/me'); } catch { return; }
  if (me.must_change_password) return showChange(true);
  startApp();
}

async function startApp() {
  show('app');
  $('#who').textContent = me.username;
  for (const el of document.querySelectorAll('[data-admin]')) el.hidden = me.role !== 'admin';
  try { S.tuned = localStorage.getItem('maio-radio') || 'main'; } catch {}
  await Promise.all([loadRadios(), loadPlaylists()]).catch(() => {});
  connectEvents();
  route();
  clearInterval(S.poll);
  S.poll = setInterval(() => { if (!document.hidden) VIEWS[current]?.refresh?.(); }, 20000);
}

$('#signin-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  try {
    await api('POST', '/session', { username: f.username.value.trim(), password: f.password.value, remember: f.remember.checked });
    f.password.value = '';
    me = await api('GET', '/me');
    if (me.must_change_password) showChange(true); else startApp();
  } catch (err) { if (err.code === 'bad_credentials') toast('Wrong username or password.'); }
});
$('#change-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  try { await api('POST', '/me/password', { current: f.current.value, new: f.new.value }); } catch { return; }
  f.reset();
  me = await api('GET', '/me');
  toast('Password changed. Your other devices are signed out.');
  startApp();
});
$('#change-cancel').addEventListener('click', () => { $('#change-form').reset(); show('app'); });
$('#change-signout').addEventListener('click', async () => { await api('DELETE', '/session').catch(() => {}); showSignIn(); });
$('#password-btn').addEventListener('click', () => { $('.me').open = false; showChange(false); });
$('#signout-btn').addEventListener('click', async () => { await api('DELETE', '/session').catch(() => {}); showSignIn(); });
/* ---------- Now ---------- */
const audio = $('#audio');
let turns = 0;           // the knob's running position, so it turns the short way and keeps turning past a wrap
let history = [];
let clock = null;

function paintDialOnce() {
  const ticks = $('#dial-ticks');
  if (!ticks.childElementCount) for (let i = 0; i <= 120; i++) ticks.append(h('i', { style: `height: ${i % 10 === 0 ? 34 : i % 5 === 0 ? 22 : 12}px` }));
  const face = $('#knob-face');
  if (!face.childElementCount) { for (let i = 0; i < 24; i++) face.append(h('i', { style: `transform: rotate(${i * 15}deg)` })); face.append(h('b')); }
  $('#tune-down').replaceChildren(icon(ICON.left));
  $('#tune-up').replaceChildren(icon(ICON.right));
}

function renderDial() {
  const n = S.radios.length;
  const at = S.radios.findIndex((r) => r.id === S.tuned);
  const slots = dialSlots(n, Math.max(0, at)).map(({ offset, index }) => {
    const r = S.radios[index];
    const live = r.id === S.tuned && S.status ? S.status.now_playing : r.now_playing; // the tuned radio's status is the freshest
    const song = live?.track?.title || (r.running ? 'Silent' : 'Off air');
    return h('button', { type: 'button', class: 'dial-slot', 'aria-pressed': String(offset === 0), style: `left: calc(50% + ${offset * 300}px)`,
      'aria-label': offset === 0 ? `${r.name}, tuned` : `Tune to ${r.name}`, onclick: () => step(offset) },
      h('span', { class: 'name' }, r.name), h('span', { class: 'song' }, song));
  });
  $('#dial-slots').replaceChildren(...slots);
  $('#dial-ticks').style.transform = `translateX(${(((turns * -37) % 60) + 60) % 60 - 30}px)`;
  $('#knob-face').style.transform = `rotate(${(turns * 360) / Math.max(1, n)}deg)`;
  const one = n <= 1;
  $('#tune-down').disabled = one; $('#tune-up').disabled = one;
}

/** Turns the dial by `k` radios, wrapping round the ring. */
function step(k) {
  const n = S.radios.length;
  if (n <= 1 || !k) return;
  const at = S.radios.findIndex((r) => r.id === S.tuned);
  turns += k;
  tune(S.radios[(((at + k) % n) + n) % n].id);
  renderDial();
}

let wheelAt = 0;
$('#dial-knob').addEventListener('wheel', (e) => { e.preventDefault(); const t = Date.now(); if (t - wheelAt < 180) return; wheelAt = t; step(e.deltaY > 0 ? 1 : -1); }, { passive: false });
$('#tune-down').addEventListener('click', () => step(-1));
$('#tune-up').addEventListener('click', () => step(1));
document.querySelector('[data-view="now"] .dial').addEventListener('keydown', (e) => {
  if (e.key === 'ArrowLeft') { e.preventDefault(); step(-1); }
  if (e.key === 'ArrowRight') { e.preventDefault(); step(1); }
});

async function refreshNow() {
  try {
    const [status, plays] = await Promise.all([api('GET', radioPath('/status')), api('GET', radioPath('/history?limit=4'))]);
    S.status = status;
    history = plays.plays;
  } catch (err) {
    if (err.status === 404) { await loadRadios(); return refreshNow(); } // the tuned radio is gone: main instead
    return;
  }
  fillMoods();
  const up = S.status.engines.filter((e) => e.health === 'up');
  $('#engine-status').textContent = up.length ? `${up[0].info?.name ?? up[0].id} ${up[0].info?.version ?? ''} ready` : 'No engine ready';
  renderNow();
}

function fillMoods() {
  const sel = $('#ask-form [name=mood]');
  if (sel.dataset.filled || !S.status) return;
  sel.dataset.filled = '1';
  sel.append(h('option', { value: '' }, 'any'), ...S.status.moods.map((m) => h('option', { value: m }, m)));
}

function renderNow() {
  renderDial();
  const st = S.status;
  if (!st) return;
  const n = st.now_playing, t = n?.track;
  const radio = tunedRadio();
  const onAir = st.on_air;
  const where = $('#np-where');
  const fromPlaylist = onAir && (!n || n.source !== 'manual');
  where.replaceChildren(`Playing on ${radio?.name ?? S.tuned}`, ...(fromPlaylist
    ? [', from ', h('a', { href: `#/playlist/${encodeURIComponent(onAir.id)}` }, onAir.name)]
    : [n ? ', a request' : ', nothing on air']));
  $('#np-title').textContent = t ? t.title : st.playout.running ? 'Silence' : 'Off air';
  if (t) {
    const c = credits(t);
    $('#np-credits').replaceChildren(...[c.artist, h('span', { class: 'muted' }, ' on the album '), c.album, c.extra ? h('span', { class: 'muted' }, ` ${c.extra}`) : null].filter(Boolean));
  } else $('#np-credits').replaceChildren(h('span', { class: 'muted' }, onAir ? 'The playlist is getting the next song ready.' : 'Put a playlist on air or ask for a song.'));
  $('#np-lyrics').textContent = t?.lyrics || '';
  $('#np-lyrics').hidden = !t?.lyrics;
  const keepBtn = $('#keep-btn');
  keepBtn.disabled = !t;
  $('#skip-btn').disabled = !t;
  renderClock();
  renderPlayed();
  renderNext();
}

function renderClock() {
  const n = S.status?.now_playing;
  const len = n?.track?.seconds || 0, pos = Math.min(len, n?.position_s || 0);
  $('#np-pos').textContent = mmss(pos);
  $('#np-len').textContent = mmss(len);
  const pct = len ? Math.round((pos / len) * 100) : 0;
  $('#np-fill').style.width = `${pct}%`;
  $('#np-bar').setAttribute('aria-valuenow', String(pct));
}

function renderPlayed() {
  const now = S.status?.now_playing?.entry_id;
  const rows = history.filter((p) => p.ended_at || !now).slice(0, 3).map((p) => {
    const btn = need('control', h('button', { type: 'button', class: p.favourite ? 'icon kept' : 'icon muted', 'aria-pressed': String(p.favourite),
      'aria-label': p.favourite ? `Stop keeping ${p.title}` : `Keep ${p.title}`,
      onclick: () => keep(p.track_id, !p.favourite).then(refreshNow) }, icon(ICON.heart(p.favourite))));
    return h('div', { class: 'played-row' },
      h('span', { class: 'muted small-text' }, ago(p.started_at)),
      h('span', { class: 'ellipsis' }, p.title, p.skipped ? h('span', { class: 'muted' }, ' skipped') : null),
      btn || h('span'));
  });
  $('#played-list').replaceChildren(...(rows.length ? rows : [h('p', { class: 'empty' }, 'Nothing has played here yet.')]));
}

function nextRow(r) {
  const remove = need('queue', h('button', { type: 'button', class: 'icon muted', 'aria-label': `Remove ${r.title} from the queue`,
    onclick: () => api('DELETE', radioPath(`/queue/${encodeURIComponent(r.entry.entry_id)}`)).then(refreshNow).catch(() => {}) }, icon(ICON.x)));
  return h('div', { class: 'next-row' },
    h('span', { class: 'num muted small-text' }, r.at),
    h('span', { class: 'ellipsis', style: 'display: flex; flex-direction: column' }, h('span', { class: 'title ellipsis' }, r.title), r.note ? h('span', { class: 'note' }, r.note) : null),
    h('span', { class: 'num small-text' }, r.len),
    remove || h('span'));
}

function renderNext() {
  const st = S.status;
  const u = upNext(st.queue, st.now_playing?.remaining_s || 0);
  $('#next-summary').textContent = st.queue.length ? `${u.minutes} ${u.minutes === 1 ? 'minute' : 'minutes'} queued` : '';
  $('#next-a').replaceChildren(...(u.a.length ? u.a.map(nextRow) : [h('p', { class: 'empty' }, 'No requests. Ask for a song below.')]));
  $('#next-b-title').textContent = st.on_air ? `From ${st.on_air.name}` : 'From the playlist on air';
  $('#next-b').replaceChildren(...(u.b.length ? u.b.map(nextRow) : [h('p', { class: 'empty' }, st.on_air ? 'The playlist is choosing what plays next.' : 'Nothing on air. When requests run out, the radio goes quiet.')]));
  $('#clear-a-btn').hidden = !can('queue') || !u.a.length;
  $('#clear-b-btn').hidden = !can('control') || !(u.b.length || st.on_air);
}

const keep = (trackId, on) => api('PUT', `/library/${encodeURIComponent(trackId)}/favourite`, { favourite: on });

$('#skip-btn').addEventListener('click', () => api('POST', radioPath('/skip')).catch(() => {}));
$('#keep-btn').addEventListener('click', () => {
  const t = S.status?.now_playing?.track;
  if (t) keep(t.id, true).then(() => toast(`Kept. ${t.title} stays in the crate.`)).catch(() => {});
});
$('#clear-a-btn').addEventListener('click', () => api('DELETE', radioPath('/queue')).then(refreshNow).catch(() => {}));
$('#clear-b-btn').addEventListener('click', () => {
  if (!confirm('Clear the playlist part of the queue? The playlist also goes off air on this radio.')) return;
  api('DELETE', radioPath('/queue?side=b')).then(() => { toast('Cleared, and the playlist is off air.'); refreshNow(); }).catch(() => {});
});

$('#ask-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  const request = { seconds: Math.round(Number(f.minutes.value) * 60) };
  if (f.prompt.value.trim()) request.prompt = f.prompt.value.trim();
  if (f.mood.value) request.mood = f.mood.value;
  if (f.vocals.checked) request.vocals = true;
  if (!request.prompt && !request.mood) return toast('Describe the song or pick a mood.');
  const send = (extra = {}) => api('POST', radioPath('/queue'), { request: { ...request, ...extra }, position: 'next' });
  try { await send(); }
  catch (err) {
    if (err.code !== 'noncommercial_engine') return;
    if (!can('admin') || !confirm('Only a non-commercial engine can make this song. Make it anyway, for personal listening?')) return;
    try { await send({ allow_noncommercial: true }); } catch { return; }
  }
  f.prompt.value = '';
  toast('Asked. It plays after the song now playing.');
  refreshNow();
});

/* listening on this device */
function servedFormats() { const l = tunedRadio()?.listen || {}; return ['mp3', 'aac'].filter((k) => l[k]); }
function chosenFormat() {
  let pick = 'auto';
  try { pick = localStorage.getItem('maio-format') || 'auto'; } catch {}
  const served = servedFormats();
  if (pick !== 'auto' && served.includes(pick)) return pick;
  if (served.includes('aac') && audio.canPlayType('audio/aac')) return 'aac';
  return served.includes('mp3') ? 'mp3' : served[0];
}
function paintPlay() {
  const playing = !audio.paused;
  $('#play-btn').replaceChildren(icon(playing ? ICON.pause : ICON.play));
  $('#play-btn').setAttribute('aria-label', playing ? 'Stop listening on this device' : 'Listen on this device');
}
function startAudio() {
  const url = tunedRadio()?.listen?.[chosenFormat()];
  if (!url) return toast('This radio has no stream this browser can play.');
  if (previewing) preview({ id: previewing.id });
  audio.src = `${url}?t=${Date.now()}`;
  audio.play().catch(() => toast('The browser blocked playback. Press play again.'));
}
function stopAudio() { audio.pause(); audio.removeAttribute('src'); audio.load(); paintPlay(); }
$('#play-btn').addEventListener('click', () => (audio.paused ? startAudio() : stopAudio()));
audio.addEventListener('play', paintPlay);
audio.addEventListener('pause', paintPlay);
audio.addEventListener('error', paintPlay); // the stream ended (radio deleted, new key): show play again
$('#format').addEventListener('change', (e) => { try { localStorage.setItem('maio-format', e.target.value); } catch {} if (!audio.paused) startAudio(); });
let lastVolume = 70;
function setVolume(v) {
  $('#volume').value = v; audio.volume = v / 100;
  const muted = Number(v) === 0;
  $('#mute-btn').replaceChildren(icon(muted ? ICON.muted : ICON.sound));
  $('#mute-btn').setAttribute('aria-pressed', String(muted));
  $('#mute-btn').setAttribute('aria-label', muted ? 'Unmute this device' : 'Mute this device');
}
$('#volume').addEventListener('input', (e) => { const v = Number(e.target.value); if (v > 0) lastVolume = v; setVolume(v); });
$('#mute-btn').addEventListener('click', () => { const v = Number($('#volume').value); if (v > 0) { lastVolume = v; setVolume(0); } else setVolume(lastVolume || 70); });
setVolume(70);
paintPlay();

VIEWS.now = {
  show() {
    paintDialOnce();
    for (const el of document.querySelectorAll('[data-view="now"] [data-need]')) el.hidden = !can(el.dataset.need);
    try { $('#format').value = localStorage.getItem('maio-format') || 'auto'; } catch {}
    renderNow();
    refreshNow();
    clearInterval(clock);
    clock = setInterval(() => { const n = S.status?.now_playing; if (n && visible('now')) { n.position_s = Math.min(n.track.seconds, n.position_s + 1); renderClock(); } }, 1000);
  },
  refresh: refreshNow,
  onEvent(kind, data) {
    if (!S.status) return;
    if (kind === 'now_playing') { S.status.now_playing = data; api('GET', radioPath('/history?limit=4')).then((p) => { history = p.plays; if (visible('now')) renderNow(); }).catch(() => {}); }
    if (kind === 'queue') S.status.queue = data;
    if (kind === 'on_air') { S.status.on_air = data; loadPlaylists().catch(() => {}); }
    if (visible('now')) renderNow();
  },
};

/* ---------- Playlists ---------- */
const onAirOn = (id) => S.radios.filter((r) => r.on_air?.id === id).map((r) => r.name);
const moodOptions = () => [['', 'any'], ...(S.status?.moods || []).map((m) => [m, m])];
const genreOptions = () => [['', 'any'], ...(S.status?.genres || []).map((g) => [g, g])];

function playlistFields(p = {}) {
  const r = p.recipe || {}, rule = p.rule || {};
  const kind = p.kind || 'generative';
  const common = [
    { name: 'name', label: 'Name', value: p.name, required: true, maxlength: 80 },
    ...(p.id ? [] : [{ name: 'kind', label: 'Kind', type: 'select', value: kind, options: [['generative', 'Generative: makes new songs from a recipe'], ['fixed', 'Fixed: songs you pick'], ['smart', 'Smart: songs that match a rule']] }]),
    { name: 'order', label: 'Order', type: 'select', value: p.order || (kind === 'fixed' ? 'in_order' : 'shuffle'), options: [['in_order', 'In order'], ['shuffle', 'Shuffle']] },
    { name: 'max_tracks', label: 'Size limit', type: 'number', value: p.max_tracks ?? '', min: 1, help: 'Songs it holds at most. Leave empty for the default.' },
  ];
  const gen = [
    { name: 'prompt', label: 'What it sounds like (generative)', value: r.prompt, maxlength: 500 },
    { name: 'mood', label: 'Mood', type: 'select', value: r.mood ?? rule.mood ?? '', options: moodOptions() },
    { name: 'genre', label: 'Genre', type: 'select', value: r.genre ?? rule.genre ?? '', options: genreOptions() },
    { name: 'variations', label: 'Variations, separated by commas (generative)', value: (r.variations || []).join(', ') },
    { name: 'min_len', label: 'Shortest song, minutes (generative)', value: r.seconds ? mmss(r.seconds[0]) : '2:30' },
    { name: 'max_len', label: 'Longest song, minutes (generative)', value: r.seconds ? mmss(r.seconds[1]) : '4:00' },
    { name: 'vocals', label: 'With vocals and lyrics (generative)', type: 'checkbox', value: r.vocals },
    { name: 'kept', label: 'Kept songs only (smart)', type: 'checkbox', value: rule.kept },
    { name: 'max_minutes', label: 'At most this many minutes long (smart)', type: 'number', value: rule.max_seconds ? Math.round(rule.max_seconds / 60) : '', min: 1 },
    { name: 'days', label: 'Made in the last days (smart)', type: 'number', value: rule.made_within_days ?? '', min: 1 },
  ];
  if (p.kind === 'fixed') return common;
  return [...common, ...gen];
}

async function newPlaylist() {
  const f = await ask({ title: 'New playlist', fields: playlistFields(), submit: 'Create' });
  if (!f) return;
  try {
    const p = await api('POST', '/playlists', UI.playlistBody(f));
    await loadPlaylists();
    go(`#/playlist/${encodeURIComponent(p.id)}`);
  } catch {}
}

function renderPlaylists() {
  const el = $('[data-view="playlists"]');
  const builtins = S.playlists.filter((p) => ['crate', 'kept', 'requests'].includes(p.kind));
  const mine = S.playlists.filter((p) => !['crate', 'kept', 'requests'].includes(p.kind));
  const row = (p) => {
    const size = sizeLine(p), air = onAirOn(p.id);
    return h('div', { class: 'table-row lists-grid' },
      h('span', { style: 'min-width: 0; display: flex; flex-direction: column; gap: 2px' },
        h('a', { class: 'list-name ellipsis', href: `#/playlist/${encodeURIComponent(p.id)}` }, p.name), h('span', { class: 'muted small-text' }, kindLabel(p))),
      h('span', { class: 'muted2 small-text' }, playlistAbout(p)),
      h('span', { style: 'display: flex; flex-direction: column; gap: 6px' },
        h('span', { class: 'num small-text' }, size.text, size.full ? h('span', { style: 'color: var(--needle-text)' }, ' full, looping') : null),
        h('span', { class: 'bar', 'aria-hidden': 'true' }, h('span', { style: `width: ${size.pct}%` }))),
      h('span', { class: 'num small-text' }, span(p.seconds)),
      h('span', { class: 'small-text' }, air.length ? [h('i', { class: 'mark' }), `On air on ${air.join(', ')}`] : h('span', { class: 'muted' }, 'Not on air')),
      h('a', { class: 'btn small', href: `#/playlist/${encodeURIComponent(p.id)}` }, 'Open'));
  };
  el.replaceChildren(h('div', { class: 'page' },
    h('div', { class: 'page-head' },
      h('div', {}, h('h1', { class: 'display headline' }, 'Playlists'), h('p', { class: 'muted' }, 'Every radio plays from these. Put one on air on any radio.')),
      need('queue', h('button', { type: 'button', class: 'btn primary big', onclick: newPlaylist }, 'New playlist'))),
    h('section', { 'aria-labelledby': 'builtin-title', style: 'display: flex; flex-direction: column; gap: 12px' },
      h('h2', { id: 'builtin-title', class: 'h2' }, 'Always here'),
      h('div', { class: 'builtins' }, ...builtins.map((b) => h('a', { class: 'builtin', href: `#/playlist/${b.id}` },
        h('span', { style: 'display: flex; align-items: baseline; gap: 10px' }, h('span', { class: 'name' }, b.name), h('span', { class: 'num push-right' }, sizeLine(b).text)),
        h('span', { class: 'muted2 small-text' }, kindLabel(b)))))),
    h('section', { 'aria-labelledby': 'mine-title' },
      h('div', { class: 'row-center', style: 'padding-bottom: 10px' }, h('h2', { id: 'mine-title', class: 'h2' }, 'Made by people')),
      h('div', { class: 'table-head lists-grid' }, h('span', {}, 'Name'), h('span', {}, 'What it plays'), h('span', {}, 'Size'), h('span', {}, 'Length'), h('span', {}, 'On air'), h('span')),
      ...(mine.length ? mine.map(row) : [h('p', { class: 'empty' }, can('queue') ? 'No playlists yet. Make one: a generative playlist keeps writing new songs in a style.' : 'No playlists yet.')]))));
}

VIEWS.playlists = {
  async show() { renderPlaylists(); await Promise.all([loadPlaylists(), loadRadios()]).catch(() => {}); if (visible('playlists')) renderPlaylists(); },
  refresh() { this.show(); },
  onEvent(kind) { if ((kind === 'playlists' || kind === 'on_air' || kind === 'radios') && visible('playlists')) loadPlaylists().then(renderPlaylists).catch(() => {}); },
};

/* a playlist's page */
let openList = null;
let openAir = null; // GET /radios/{r}/on-air for a radio playing this playlist: what it plays next

async function loadPlaylist(id) {
  let p = await api('GET', `/playlists/${encodeURIComponent(id)}?limit=200`);
  let tracks = p.tracks;
  while (p.next_cursor != null && tracks.length < 1000) {
    p = { ...p, ...(await api('GET', `/playlists/${encodeURIComponent(id)}?limit=200&cursor=${p.next_cursor}`)) };
    tracks = tracks.concat(p.tracks);
  }
  return { ...p, tracks };
}

function recipeRows(p) {
  if (p.kind === 'generative') {
    const r = p.recipe || {};
    return [['Mood', r.mood || 'any'], ['Genre', r.genre || 'any'], ['Variations', (r.variations || []).join('; ') || 'none'],
      ['Song length', r.seconds ? `${mmss(r.seconds[0])} to ${mmss(r.seconds[1])}` : ''], ['Replays', `${Math.round((r.library_share ?? 0.25) * 100)}% of gaps play an older song`],
      ['Vocals', r.vocals ? `Sung in ${r.vocal_language || 'en'}` : 'Instrumental'], ['Size limit', `${p.max_tracks} songs, then it loops`], ['Order', p.order === 'shuffle' ? 'Shuffle' : 'In order']];
  }
  if (p.kind === 'smart') return [['Matches', playlistAbout(p)], ['Plays', `the newest ${p.max_tracks}`], ['Order', p.order === 'shuffle' ? 'Shuffle' : 'In order']];
  return [['Order', p.order === 'shuffle' ? 'Shuffle' : 'In order'], ...(p.max_tracks ? [['Size limit', `${p.max_tracks} songs`]] : [])];
}

function describe(p) {
  if (p.kind === 'generative') return `Generative. While it is on air it makes new songs in this style, until it has ${p.max_tracks}; then it loops them.`;
  if (p.kind === 'smart') return 'Smart. It plays the crate songs that match its rule, newest first.';
  if (p.kind === 'fixed') return 'Fixed. Songs picked by hand, played in order or shuffled, on a loop.';
  return kindLabel(p) + '.';
}

async function saveOrder(p, ids) {
  try { await api('PUT', `/playlists/${encodeURIComponent(p.id)}/tracks`, { track_ids: ids }); } catch {}
  VIEWS.playlist.show(p.id);
}

function renderPlaylist() {
  const p = openList, el = $('[data-view="playlist"]');
  if (!p) return;
  const stored = p.kind === 'fixed' || p.kind === 'generative';
  const builtin = ['crate', 'kept', 'requests'].includes(p.kind);
  const air = S.radios.filter((r) => r.on_air?.id === p.id);
  const onTuned = air.some((r) => r.id === S.tuned);
  const radioSel = h('select', { class: 'field', 'aria-label': 'Radio' }, ...S.radios.map((r) => h('option', { value: r.id, selected: r.id === S.tuned }, r.name)));
  const ids = p.tracks.map((t) => t.id);
  let dragFrom = null;
  const nextNotes = UI.upcomingNotes(openAir, openAir?.radio);
  const rows = p.tracks.map((t, i) => {
    const base = songNote(t, S.radios);
    const note = base.live || !nextNotes.has(t.id) ? base : { text: nextNotes.get(t.id), live: false };
    const move = (to) => { if (to < 0 || to >= ids.length) return; const next = ids.slice(); next.splice(to, 0, next.splice(i, 1)[0]); saveOrder(p, next); };
    const handle = stored && can('queue') ? h('button', { type: 'button', class: 'icon handle muted', 'aria-label': `Move ${t.title}; use the up and down arrow keys`,
      onkeydown: (e) => { if (e.key === 'ArrowUp') { e.preventDefault(); move(i - 1); } if (e.key === 'ArrowDown') { e.preventDefault(); move(i + 1); } } }, icon(ICON.grip)) : h('span');
    const row = h('div', { class: `song-row${note.live ? ' live' : ''}`, draggable: stored && can('queue') ? 'true' : null,
      ondragstart: () => { dragFrom = i; row.classList.add('dragging'); }, ondragend: () => row.classList.remove('dragging'),
      ondragover: (e) => { if (dragFrom != null) e.preventDefault(); },
      ondrop: (e) => { e.preventDefault(); if (dragFrom == null || dragFrom === i) return; const next = ids.slice(); next.splice(i, 0, next.splice(dragFrom, 1)[0]); dragFrom = null; saveOrder(p, next); } },
      h('span', { class: 'num muted small-text' }, String(i + 1)),
      previewButton(t),
      h('span', { class: 'ellipsis', style: 'display: flex; flex-direction: column' }, h('span', { class: 'ellipsis', style: 'font-weight: 600' }, t.title), note.text ? h('span', { class: 'note' }, note.text) : null),
      h('span', { class: 'muted2 small-text key' }, t.key || ''),
      h('span', { class: 'muted2 small-text num bpm' }, t.bpm ? `${t.bpm} bpm` : ''),
      h('span', { class: 'num small-text' }, mmss(t.seconds)),
      handle,
      stored ? need('queue', h('button', { type: 'button', class: 'icon muted', 'aria-label': `Remove ${t.title} from this playlist`,
        onclick: () => api('DELETE', `/playlists/${encodeURIComponent(p.id)}/tracks/${encodeURIComponent(t.id)}`).then(() => VIEWS.playlist.show(p.id)).catch(() => {}) }, icon(ICON.x))) || h('span') : h('span'));
    return row;
  });
  const size = sizeLine(p);
  el.replaceChildren(h('div', { class: 'page' },
    h('a', { href: '#/playlists', class: 'row-center small-link', style: 'align-self: flex-start' }, icon(ICON.left), 'Playlists'),
    h('div', { class: 'page-head pl-head' },
      h('div', { style: 'display: flex; flex-direction: column; gap: 8px; min-width: 0' }, h('h1', { class: 'display' }, p.name), h('p', { class: 'muted2' }, describe(p))),
      h('div', { class: 'row-gap', style: 'align-items: center' },
        air.length ? h('span', { class: 'small-text' }, h('i', { class: 'mark' }), `On air on ${air.map((r) => r.name).join(', ')}`) : h('span', { class: 'muted small-text' }, 'Not on air'),
        ...(can('control') ? [
          onTuned ? h('button', { type: 'button', class: 'btn', onclick: () => api('PUT', radioPath('/on-air'), { playlist_id: null }).then(() => loadRadios()).then(renderPlaylist).catch(() => {}) }, `Take off ${tunedRadio()?.name ?? ''}`) : null,
          radioSel,
          h('button', { type: 'button', class: 'btn primary', onclick: () => api('PUT', radioPath('/on-air', radioSel.value), { playlist_id: p.id }).then(() => { toast(`On air on ${S.radios.find((r) => r.id === radioSel.value)?.name}.`); return loadRadios(); }).then(renderPlaylist).catch(() => {}) }, 'Put on air'),
        ] : []))),
    h('div', { class: 'pl-body' },
      h('section', { 'aria-labelledby': 'songs-title', style: 'display: flex; flex-direction: column; min-width: 0' },
        h('div', { class: 'row-center', style: 'padding-bottom: 10px; flex-wrap: wrap' },
          h('h2', { id: 'songs-title', class: 'h2' }, 'Songs'),
          h('span', { class: 'muted small-text' }, `${size.text}, ${p.order === 'shuffle' ? 'shuffled' : 'in order'}, ${span(p.seconds)}`),
          p.max_tracks ? h('span', { class: 'bar', style: 'width: 160px', 'aria-hidden': 'true' }, h('span', { style: `width: ${size.pct}%` })) : null,
          stored && can('queue') && p.tracks.length > 1 ? h('span', { class: 'muted small-text push-right' }, 'Drag to reorder') : null),
        ...(rows.length ? rows : [h('p', { class: 'empty' }, stored ? 'No songs yet. Add some from the crate, or put it on air to make new ones.' : 'No songs match yet.')])),
      builtin ? h('span') : h('aside', { class: 'panel recipe', 'aria-labelledby': 'recipe-title', style: 'display: flex; flex-direction: column; gap: 16px' },
        h('div', { class: 'row-center' }, h('h2', { id: 'recipe-title', class: 'h2' }, p.kind === 'generative' ? 'Recipe' : p.kind === 'smart' ? 'Rule' : 'Settings'),
          need('queue', h('button', { type: 'button', class: 'btn small push-right', onclick: () => editPlaylist(p) }, 'Edit'))),
        p.recipe?.prompt ? h('p', { class: 'prompt' }, p.recipe.prompt) : null,
        h('dl', {}, ...recipeRows(p).flatMap(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
        need('queue', h('button', { type: 'button', class: 'btn danger small', style: 'align-self: flex-start', onclick: () => deletePlaylist(p) }, 'Delete playlist'))))));
}

async function editPlaylist(p) {
  const f = await ask({ title: `Edit ${p.name}`, fields: playlistFields(p) });
  if (!f) return;
  const body = UI.mergeEdit(p, UI.playlistBody({ ...f, kind: p.kind }));
  delete body.kind;
  try { await api('PATCH', `/playlists/${encodeURIComponent(p.id)}`, body); } catch { return; }
  await loadPlaylists();
  VIEWS.playlist.show(p.id);
}

async function deletePlaylist(p) {
  if (!confirm(`Delete the playlist ${p.name}? Its songs stay in the crate.`)) return;
  try { await api('DELETE', `/playlists/${encodeURIComponent(p.id)}`); } catch { return; }
  await loadPlaylists();
  go('#/playlists');
}

VIEWS.playlist = {
  async show(id) {
    if (id) openList = { id, ...(openList?.id === id ? openList : { name: '', tracks: [], kind: '' }) };
    try { openList = await loadPlaylist(openList.id); } catch (err) { if (err.status === 404) go('#/playlists'); return; }
    await loadRadios().catch(() => {});
    const playing = S.radios.find((r) => r.id === S.tuned && r.on_air?.id === openList.id) || S.radios.find((r) => r.on_air?.id === openList.id);
    openAir = playing ? { radio: playing.name, ...(await api('GET', radioPath('/on-air', playing.id)).catch(() => null)) } : null;
    if (visible('playlist')) renderPlaylist();
  },
  refresh() { if (openList) this.show(openList.id); },
  onEvent(kind) { if (['playlists', 'on_air', 'now_playing', 'radios'].includes(kind) && visible('playlist') && openList) this.show(openList.id); },
};

/* ---------- Crate ---------- */
const crate = { q: '', from: '', sort: 'newest', kept: false, tracks: [], next: null };

async function loadCrate(more = false) {
  const res = await api('GET', `/library?${UI.crateQuery(crate, more ? crate.next : 0)}`);
  crate.tracks = more ? crate.tracks.concat(res.tracks) : res.tracks;
  crate.next = res.next_cursor;
}

async function addToPlaylist(t) {
  const lists = S.playlists.filter((p) => p.kind === 'fixed' || p.kind === 'generative');
  if (!lists.length) return toast('Make a fixed or generative playlist first, under Playlists.');
  const f = await ask({ title: `Add ${t.title} to a playlist`, fields: [{ name: 'playlist', label: 'Playlist', type: 'select', options: lists.map((p) => [p.id, `${p.name} (${sizeLine(p).text})`]) }], submit: 'Add' });
  if (!f) return;
  try { await api('POST', `/playlists/${encodeURIComponent(f.playlist)}/tracks`, { track_ids: [t.id] }); toast(`Added to ${playlistName(f.playlist)}.`); loadPlaylists().catch(() => {}); } catch {}
}

async function renameTrack(t) {
  const f = await ask({ title: 'Rename song', fields: [{ name: 'title', label: 'Title', value: t.title, required: true, maxlength: 80 }] });
  if (!f || f.title === t.title) return;
  try { await api('PATCH', `/library/${encodeURIComponent(t.id)}`, { title: f.title }); } catch { return; }
  renderCrateRows(true);
}

function crateRow(t) {
  const busy = S.radios.find((r) => r.now_playing?.track?.id === t.id);
  return h('div', { class: 'table-row crate-grid' },
    previewButton(t),
    h('span', { class: 'ellipsis', style: 'display: flex; flex-direction: column' }, h('span', { class: 'ellipsis', style: 'font-weight: 600' }, t.title), h('span', { class: 'muted small-text' }, UI.trackFrom(t, S.playlists))),
    h('span', { class: 'muted2 small-text tags' }, [t.mood, t.genre].filter(Boolean).join(', ')),
    h('span', { class: 'num small-text' }, mmss(t.seconds)),
    h('span', { class: 'muted2 small-text made' }, ago(t.created_at)),
    h('span', { class: 'muted2 small-text played-at' }, busy ? `playing on ${busy.name}` : ago(t.last_played_at)),
    need('control', h('button', { type: 'button', class: t.favourite ? 'icon kept' : 'icon muted', 'aria-pressed': String(t.favourite), 'aria-label': t.favourite ? `Stop keeping ${t.title}` : `Keep ${t.title}`,
      onclick: () => keep(t.id, !t.favourite).then(() => renderCrateRows(true)).catch(() => {}) }, icon(ICON.heart(t.favourite)))) || h('span'),
    can('queue') ? h('span', { class: 'row-gap actions', style: 'gap: 6px; flex-wrap: nowrap' },
      h('button', { type: 'button', class: 'btn small', onclick: () => addToPlaylist(t) }, 'Add to playlist'),
      h('button', { type: 'button', class: 'btn small', onclick: () => renameTrack(t) }, 'Rename'),
      h('button', { type: 'button', class: 'btn small', onclick: () => api('POST', radioPath('/queue'), { track_id: t.id, position: 'next' }).then(() => toast(`${t.title} plays next on ${tunedRadio()?.name}.`)).catch(() => {}) }, 'Play next')) : h('span'),
    need('queue', h('button', { type: 'button', class: 'icon muted', 'aria-label': `Delete ${t.title}`,
      onclick: () => { if (confirm(`Delete ${t.title} from the crate and from disk?`)) api('DELETE', `/library/${encodeURIComponent(t.id)}`).then(() => renderCrateRows(true)).catch(() => {}); } }, icon(ICON.x))) || h('span'));
}

async function renderCrateRows(reload = false) {
  if (reload) { try { await loadCrate(); } catch { return; } }
  const list = $('#crate-list');
  if (!list) return;
  const filtered = crate.q || crate.from || crate.kept;
  list.replaceChildren(...(crate.tracks.length ? crate.tracks.map(crateRow) : [h('p', { class: 'empty' }, filtered ? 'No songs match. Try fewer filters.' : 'The crate is empty. Ask for a song on Now, or put a generative playlist on air.')]));
  $('#crate-more').hidden = crate.next == null;
}

function renderCrate() {
  const st = S.status?.storage;
  const gb = (b) => (b / 1e9).toFixed(1);
  const all = S.playlists.find((p) => p.kind === 'crate');
  const search = h('input', { type: 'search', class: 'field', placeholder: 'Search titles, moods, lyrics', value: crate.q, 'aria-label': 'Search the crate',
    oninput: (e) => { crate.q = e.target.value; clearTimeout(renderCrate.t); renderCrate.t = setTimeout(() => renderCrateRows(true), 300); } });
  const from = h('select', { class: 'field', onchange: (e) => { crate.from = e.target.value; renderCrateRows(true); } },
    h('option', { value: '' }, 'Anywhere'), ...S.playlists.filter((p) => p.kind !== 'crate').map((p) => h('option', { value: p.id, selected: p.id === crate.from }, p.name)));
  const sort = h('select', { class: 'field', onchange: (e) => { crate.sort = e.target.value; renderCrateRows(true); } },
    ...[['newest', 'Newest'], ['played', 'Last played'], ['title', 'Title']].map(([v, l]) => h('option', { value: v, selected: v === crate.sort }, l)));
  const keptBtn = h('button', { type: 'button', class: 'btn', 'aria-pressed': String(crate.kept), onclick: () => { crate.kept = !crate.kept; keptBtn.setAttribute('aria-pressed', String(crate.kept)); keptBtn.classList.toggle('solid', crate.kept); renderCrateRows(true); } }, 'Kept only');
  keptBtn.classList.toggle('solid', crate.kept);
  $('[data-view="crate"]').replaceChildren(h('div', { class: 'page' },
    h('div', { class: 'page-head' },
      h('div', {}, h('h1', { class: 'display headline' }, 'Crate'), h('p', { class: 'muted' }, 'Every song Maio has made. When space runs out, the oldest songs nobody kept go first.')),
      st ? h('div', { class: 'meter' },
        h('span', { class: 'row-center small-text' }, h('span', {}, st.max_bytes ? `${gb(st.used_bytes)} of ${gb(st.max_bytes)} GB used` : `${gb(st.used_bytes)} GB used`),
          h('span', { class: 'muted push-right' }, `${all ? sizeLine(all).text : ''}, stored as ${st.format === 'aac' ? 'AAC' : st.format === 'mp3' ? 'MP3' : st.format === 'flac' ? 'FLAC' : 'Opus'}`)),
        h('span', { class: 'bar', 'aria-hidden': 'true' }, h('span', { style: `width: ${st.max_bytes ? Math.min(100, Math.round((st.used_bytes / st.max_bytes) * 100)) : 0}%` })),
        st.full ? h('span', { class: 'small-text', style: 'color: var(--needle-text)' }, 'Full of kept songs: new songs wait until you empty the crate or stop keeping some.') : null) : h('span')),
    h('form', { role: 'search', class: 'search', onsubmit: (e) => { e.preventDefault(); renderCrateRows(true); } },
      search, h('label', { class: 'row-center small-text' }, 'From', from), h('label', { class: 'row-center small-text' }, 'Sort', sort), keptBtn,
      can('admin') ? h('button', { type: 'button', class: 'btn danger', onclick: emptyCrate }, 'Empty the crate') : null),
    h('section', { 'aria-label': 'Songs' },
      h('div', { class: 'table-head crate-grid' }, h('span'), h('span', {}, 'Song'), h('span', {}, 'Mood and genre'), h('span', {}, 'Length'), h('span', {}, 'Made'), h('span', {}, 'Last played'), h('span'), h('span'), h('span')),
      h('div', { id: 'crate-list' }),
      h('button', { type: 'button', class: 'btn', id: 'crate-more', hidden: true, style: 'margin-top: 16px', onclick: () => loadCrate(true).then(() => renderCrateRows()).catch(() => {}) }, 'Show more'))));
  renderCrateRows();
}

async function emptyCrate() {
  if (!confirm('Empty the crate? Every song nobody kept, and that is not in a fixed playlist, is deleted from disk. Queued and playing songs stay. This cannot be undone.')) return;
  try { const r = await api('DELETE', '/library'); toast(`Deleted ${r.deleted} ${r.deleted === 1 ? 'song' : 'songs'}.`); } catch { return; }
  renderCrateRows(true);
}

VIEWS.crate = {
  async show(_arg, query) {
    if (query?.get('sort')) crate.sort = query.get('sort');
    if (!S.status) S.status = await api('GET', radioPath('/status')).catch(() => null);
    renderCrate();
    await Promise.all([loadCrate(), loadPlaylists()]).catch(() => {});
    if (visible('crate')) renderCrate();
  },
  refresh() { renderCrateRows(true); },
  onEvent(kind) { if (kind === 'now_playing' && visible('crate')) renderCrateRows(); },
};

/* ---------- Radios ---------- */
let linksFor = null;

function renderRadios() {
  const el = $('[data-view="radios"]');
  const full = S.radios.length >= S.maxRadios;
  const cards = S.radios.map((r) => {
    const tuned = r.id === S.tuned;
    const song = r.now_playing?.track?.title;
    const formats = ['mp3', 'aac', 'hls'].filter((k) => r.listen?.[k]).map((k) => k.toUpperCase());
    return h('article', { class: `radio-card${tuned ? ' tuned' : ''}` },
      h('div', { style: 'display: flex; flex-direction: column; gap: 4px' }, h('h2', {}, r.name),
        h('span', { class: 'row-center small-text muted2', style: 'gap: 8px' }, icon(r.visibility === 'private' ? ICON.lock : ICON.globe), r.visibility === 'private' ? 'Private, secret link' : 'Public')),
      h('div', { style: 'min-width: 0; display: flex; flex-direction: column; gap: 4px' },
        h('span', { class: 'radio-song' }, song ? h('i') : null, h('span', { class: 'ellipsis' }, song || (r.running ? 'Silent' : 'Off air'))),
        h('span', { class: 'muted2', style: 'font-size: 15px' }, r.on_air ? `from ${r.on_air.name}` : 'No playlist on air')),
      h('div', { style: 'display: flex; flex-direction: column; gap: 4px; font-size: 14px' },
        h('span', {}, r.listeners ? `${r.listeners} listening` : 'Nobody listening'),
        h('span', { class: 'muted' }, formats.length ? formats.join(', ').replace(/, ([^,]*)$/, ' and $1') : 'No streams')),
      h('div', { class: 'row-gap', style: 'justify-content: flex-end; align-items: center' },
        tuned ? h('span', { style: 'font-weight: 700; font-size: 15px' }, "You're listening") : h('button', { type: 'button', class: 'btn primary', onclick: () => { tune(r.id); go('#/now'); } }, 'Tune in'),
        h('button', { type: 'button', class: 'btn', 'aria-expanded': String(linksFor === r.id), onclick: () => { linksFor = linksFor === r.id ? null : r.id; renderRadios(); } }, 'Listen address'),
        can('admin') ? h('button', { type: 'button', class: 'icon', 'aria-label': `Settings for ${r.name}`, onclick: () => radioSettings(r) }, icon(ICON.sliders)) : null));
  });
  const open = S.radios.find((r) => r.id === linksFor);
  const links = open ? h('section', { class: 'panel', 'aria-labelledby': 'links-title', style: 'display: flex; flex-direction: column; gap: 10px' },
    h('div', { class: 'row-gap', style: 'align-items: baseline' },
      h('h2', { id: 'links-title', class: 'h2', style: 'font-size: 18px' }, `Listen address for ${open.name}`),
      h('span', { class: 'muted2 small-text' }, open.visibility === 'private' ? 'Private: only people with this link can listen, in any player or browser.' : 'Public: anyone who knows the name can listen, in any player or browser.'),
      ...(can('admin') ? [
        open.visibility === 'private' ? h('button', { type: 'button', class: 'btn small push-right', onclick: () => newLink(open) }, 'Make a new link') : null,
        h('button', { type: 'button', class: `btn small${open.visibility === 'private' ? '' : ' push-right'}`, onclick: () => setVisibility(open, open.visibility === 'private' ? 'public' : 'private') }, open.visibility === 'private' ? 'Make public' : 'Make private'),
      ] : [])),
    h('div', { class: 'links' }, ...UI.listenLinks(open.listen, location.origin).flatMap((l) => [
      h('span', { class: 'muted' }, l.kind), h('code', {}, l.url),
      h('button', { type: 'button', class: 'btn small', onclick: () => navigator.clipboard.writeText(l.url).then(() => toast(`${l.kind} address copied.`), () => toast('Copying is blocked here. Select the address instead.')) }, 'Copy')]))) : null;
  el.replaceChildren(h('div', { class: 'page' },
    h('div', { class: 'page-head' },
      h('div', {}, h('h1', { class: 'display headline' }, 'Radios'), h('p', { class: 'muted' }, 'Each radio is its own live stream. Public radios have an address anyone can use; private ones have a secret link.')),
      can('admin') ? h('div', { style: 'display: flex; flex-direction: column; align-items: flex-end; gap: 6px' },
        h('button', { type: 'button', class: 'btn primary big', disabled: full, 'aria-describedby': 'radio-limit', onclick: newRadio }, 'New radio'),
        h('span', { id: 'radio-limit', class: 'muted small-text' }, full ? `${S.radios.length} of ${S.maxRadios} radios. Remove one to make another.` : `${S.radios.length} of ${S.maxRadios} radios`)) : h('span')),
    h('div', { style: 'display: flex; flex-direction: column; gap: 14px' }, ...cards),
    links));
}

async function newRadio() {
  const f = await ask({ title: 'New radio', submit: 'Create', fields: [
    { name: 'name', label: 'Name', required: true, maxlength: 80 },
    { name: 'id', label: 'Address name', required: true, maxlength: 32, help: '2 to 32 characters: a to z, 0 to 9 and -. Public radios listen at /radios/<address name>/.' },
    { name: 'visibility', label: 'Who can listen', type: 'select', value: 'public', options: [['public', 'Anyone who knows the name'], ['private', 'Only people with a secret link']] },
    { name: 'playlist_id', label: 'Put on air', type: 'select', value: '', options: [['', 'Nothing yet'], ...S.playlists.map((p) => [p.id, p.name])] },
  ] });
  if (!f) return;
  const body = { id: f.id.toLowerCase(), name: f.name, visibility: f.visibility };
  if (f.playlist_id) body.playlist_id = f.playlist_id;
  try { await api('POST', '/radios', body); } catch { return; }
  await loadRadios();
  renderRadios();
}

async function radioSettings(r) {
  const f = await ask({ title: `Settings for ${r.name}`, fields: [
    { name: 'name', label: 'Name', value: r.name, required: true, maxlength: 80 },
    { name: 'running', label: 'On air (unticked: this radio sends silence)', type: 'checkbox', value: r.running },
    ...(r.id === 'main' ? [] : [{ name: 'remove', label: `Remove ${r.name}: its queue is dropped and its listeners disconnected`, type: 'checkbox', value: false }]),
  ] });
  if (!f) return;
  try {
    if (f.remove) {
      if (!confirm(`Remove the radio ${r.name}? This cannot be undone.`)) return;
      await api('DELETE', `/radios/${encodeURIComponent(r.id)}`);
      if (S.tuned === r.id) tune('main');
    } else {
      if (f.name !== r.name) await api('PATCH', `/radios/${encodeURIComponent(r.id)}`, { name: f.name });
      if (f.running !== r.running) await api('POST', radioPath(f.running ? '/start' : '/stop', r.id));
    }
  } catch {}
  await loadRadios();
  renderRadios();
}

async function newLink(r) {
  if (!confirm(`Make a new secret link for ${r.name}? The old link stops working at once and its listeners are disconnected.`)) return;
  try { await api('POST', `/radios/${encodeURIComponent(r.id)}/listen-key`); } catch { return; }
  await loadRadios();
  renderRadios();
  toast('New link made. Share it again with the people who listen.');
}

async function setVisibility(r, visibility) {
  if (!confirm(visibility === 'private' ? `Make ${r.name} private? Its public address stops working and current listeners are disconnected.` : `Make ${r.name} public? Anyone who knows its name can listen.`)) return;
  try { await api('PATCH', `/radios/${encodeURIComponent(r.id)}`, { visibility }); } catch { return; }
  await loadRadios();
  renderRadios();
}

VIEWS.radios = {
  async show() { renderRadios(); await Promise.all([loadRadios(), loadPlaylists()]).catch(() => {}); if (visible('radios')) renderRadios(); },
  refresh() { this.show(); },
  onEvent(kind) { if ((kind === 'radios' || kind === 'now_playing') && visible('radios')) loadRadios().then(renderRadios).catch(() => {}); },
};

/* ---------- Users (admins) ---------- */
async function renderUsers() {
  let people = [];
  try { people = await api('GET', '/users'); } catch { return; }
  const row = (u) => {
    const self = u.username === me.username;
    return h('div', { class: 'table-row people-grid' },
      h('span', { class: 'chip', 'aria-hidden': 'true' }, (u.name[0] || '?').toUpperCase()),
      h('span', { style: 'min-width: 0; display: flex; flex-direction: column' }, h('span', { style: 'font-weight: 600' }, self ? `${u.name} (you)` : u.name), h('span', { class: 'muted small-text' }, u.username)),
      h('label', { class: 'role' }, h('span', { class: 'sr' }, `Role for ${u.name}`),
        h('select', { class: 'field', style: 'width: 100%', onchange: (e) => api('PATCH', `/users/${encodeURIComponent(u.username)}`, { role: e.target.value }).then(renderUsers).catch(renderUsers) },
          ...['admin', 'operator', 'listener'].map((r) => h('option', { value: r, selected: r === u.role }, r)))),
      h('span', { class: 'muted2 small-text seen' }, u.must_change ? 'Must choose a password' : u.last_seen_at ? `Seen ${ago(u.last_seen_at)}` : 'Never signed in'),
      h('button', { type: 'button', class: 'btn small reset', onclick: () => resetPassword(u) }, 'Reset password'),
      self ? h('span') : h('button', { type: 'button', class: 'icon muted', 'aria-label': `Remove ${u.name}`, onclick: () => { if (confirm(`Remove ${u.name}? They are signed out at once.`)) api('DELETE', `/users/${encodeURIComponent(u.username)}`).then(renderUsers).catch(() => {}); } }, icon(ICON.x)));
  };
  const form = h('form', { class: 'panel add-person', onsubmit: async (e) => {
    e.preventDefault();
    const f = e.target;
    try { await api('POST', '/users', { name: f.name.value.trim(), username: f.username.value.trim(), password: f.password.value, role: f.role.value }); f.reset(); renderUsers(); } catch {}
  } },
    h('label', { class: 'field-label', style: 'flex: 2' }, 'Name', h('input', { name: 'name', class: 'field', required: true, maxlength: 80 })),
    h('label', { class: 'field-label' }, 'Username', h('input', { name: 'username', class: 'field', required: true, spellcheck: 'false', autocomplete: 'off' })),
    h('label', { class: 'field-label' }, 'Temporary password', h('input', { name: 'password', class: 'field', type: 'password', autocomplete: 'new-password', required: true, minlength: 10 })),
    h('label', { class: 'field-label', style: 'flex: 0 0 150px' }, 'Role', h('select', { name: 'role', class: 'field' }, ...['operator', 'listener', 'admin'].map((r) => h('option', { value: r }, r)))),
    h('button', { class: 'btn primary' }, 'Add person'));
  $('[data-view="users"]').replaceChildren(h('div', { class: 'page' },
    h('div', {}, h('h1', { class: 'display headline' }, 'Users'), h('p', { class: 'muted' }, 'Everyone who signs in to Maio.')),
    h('section', { 'aria-labelledby': 'people-title' },
      h('div', { class: 'row-center', style: 'padding-bottom: 10px' }, h('h2', { id: 'people-title', class: 'h2' }, 'People'), h('span', { class: 'muted small-text' }, `${people.length} ${people.length === 1 ? 'person' : 'people'}`)),
      ...people.map(row),
      form,
      h('div', { class: 'roles' },
        h('p', {}, h('strong', { style: 'color: var(--ink)' }, 'Admin'), ' can do everything, including people, radios, and turning radios on and off.'),
        h('p', {}, h('strong', { style: 'color: var(--ink)' }, 'Operator'), ' runs the music: asks for songs, edits playlists, puts them on air, skips and keeps.'),
        h('p', {}, h('strong', { style: 'color: var(--ink)' }, 'Listener'), ' sees what plays and listens. Changes nothing.')))));
}

async function resetPassword(u) {
  const f = await ask({ title: `Reset the password for ${u.name}`, submit: 'Reset', fields: [{ name: 'password', label: 'Temporary password', type: 'password', required: true, minlength: 10, help: 'They choose their own the next time they sign in, and are signed out now.' }] });
  if (!f) return;
  try { await api('POST', `/users/${encodeURIComponent(u.username)}/password`, { password: f.password }); toast(`${u.name} is signed out and must choose a new password.`); renderUsers(); } catch {}
}

VIEWS.users = { show() { renderUsers(); }, refresh() { renderUsers(); } };

document.addEventListener('DOMContentLoaded', boot);
