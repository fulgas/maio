/* Pure helpers for the web UI: no DOM, so node tests load this file too (web/tests). */
(function (root) {
  'use strict';

  const mmss = (s) => { s = Math.max(0, Math.round(s || 0)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };

  /** "3 min ago", "3 h ago", "yesterday", "5 days ago". */
  function ago(iso, now = Date.now()) {
    if (!iso) return 'never';
    const s = Math.max(0, (now - Date.parse(iso)) / 1000);
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)} min ago`;
    if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
    const d = Math.floor(s / 86400);
    return d === 1 ? 'yesterday' : `${d} days ago`;
  }

  /** A total length: "58 min", "1 h 52 min". */
  function span(seconds) {
    if (seconds > 0 && seconds < 60) return 'under 1 min';
    const m = Math.round((seconds || 0) / 60);
    return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
  }

  /** The dial is a ring: the tuned radio under the needle, neighbours wrapping round both sides, each radio once. */
  function dialSlots(n, at) {
    if (n <= 0) return [];
    const i0 = ((at % n) + n) % n;
    const left = Math.min(2, Math.floor((n - 1) / 2));
    const right = Math.min(2, n - 1 - left);
    const out = [];
    for (let k = 0 - left; k <= right; k++) out.push({ offset: k, index: (((i0 + k) % n) + n) % n });
    return out;
  }

  /** The radio to tune: the one wanted if it still exists, else main, else the first. */
  function pickRadio(radios, wanted) {
    if (radios.some((r) => r.id === wanted)) return wanted;
    return radios.some((r) => r.id === 'main') ? 'main' : radios[0]?.id;
  }

  function kindLabel(p) {
    switch (p.kind) {
      case 'generative': return 'Generative, makes new songs';
      case 'fixed': return p.order === 'shuffle' ? 'Fixed, shuffled' : 'Fixed, in order';
      case 'smart': return 'Smart, newest first';
      case 'crate': return 'Everything Maio has made and not deleted';
      case 'kept': return 'Songs someone kept; never deleted to save space';
      case 'requests': return 'Songs people asked for one at a time';
      default: return '';
    }
  }

  function sizeLine(p) {
    if (!p.max_tracks) return { text: `${p.count} ${p.count === 1 ? 'song' : 'songs'}`, full: false, pct: 0 };
    return { text: `${p.count} of ${p.max_tracks}`, full: p.count >= p.max_tracks, pct: Math.min(100, Math.round((p.count / p.max_tracks) * 100)) };
  }

  function playlistAbout(p) {
    if (p.kind === 'generative') return [p.recipe?.prompt, p.recipe?.genre, p.recipe?.mood].filter(Boolean).join('; ');
    if (p.kind === 'smart') {
      const r = p.rule || {};
      const parts = [r.kept ? 'Kept songs' : 'Songs', r.mood, r.genre, r.side === 'a' ? 'asked for' : r.side === 'b' ? 'made by playlists' : null,
        r.max_seconds ? `under ${Math.round(r.max_seconds / 60)} minutes` : null, r.made_within_days ? `made in the last ${r.made_within_days} days` : null];
      return parts.filter(Boolean).join(', ');
    }
    if (p.kind === 'fixed') return 'Picked by hand from the crate';
    return kindLabel(p);
  }

  function credits(t) {
    const extra = [t.key && `in ${t.key}`, t.bpm && `${t.bpm} bpm`].filter(Boolean).join(', ');
    return { artist: t.artist || 'Maio', album: t.album || 'Requests', extra };
  }

  const asker = (by) => (by && by.startsWith('user:') ? `asked by ${by.slice(5)}` : '');

  /** The queue as the Now screen shows it: when each entry starts, requests first, then the playlist. */
  function upNext(queue, remaining) {
    let at = Math.max(0, remaining || 0);
    const a = [], b = [];
    for (const e of queue) {
      const note = e.state === 'generating'
        ? (e.side === 'a' ? `being made, about ${Math.max(1, Math.round(e.eta_s || 0))} s` : 'new for this playlist')
        : (e.side === 'a' ? asker(e.by) : 'from the playlist');
      (e.side === 'a' ? a : b).push({ entry: e, at: `+${mmss(at)}`, title: e.title, note, len: mmss(e.seconds) });
      at += e.seconds || 0;
    }
    return { a, b, minutes: Math.round((at - Math.max(0, remaining || 0)) / 60) };
  }

  /** Why a song stands out in a playlist: playing somewhere now, kept, or new today. */
  function songNote(t, radios, now = Date.now()) {
    const on = radios.find((r) => r.now_playing?.track?.id === t.id);
    if (on) return { text: `playing now on ${on.name}`, live: true };
    if (t.favourite) return { text: 'kept', live: false };
    if (t.created_at && new Date(t.created_at).toDateString() === new Date(now).toDateString()) return { text: 'new today', live: false };
    return { text: '', live: false };
  }

  /** "2:30" or "4" (minutes) into seconds. */
  const toSeconds = (v) => { const [m, s = '0'] = String(v).split(':'); return Math.round(Number(m) * 60 + Number(s)); };

  /** The new or edited playlist form, as a POST /playlists (or PATCH) body. */
  function playlistBody(f) {
    const body = { name: f.name.trim(), kind: f.kind, order: f.order };
    if (f.max_tracks) body.max_tracks = Number(f.max_tracks);
    if (f.kind === 'generative') {
      const r = {};
      if (f.prompt) r.prompt = f.prompt;
      if (f.mood) r.mood = f.mood;
      if (f.genre) r.genre = f.genre;
      const v = (f.variations || '').split(',').map((x) => x.trim()).filter(Boolean);
      if (v.length) r.variations = v;
      if (f.vocals) r.vocals = true;
      r.seconds = [toSeconds(f.min_len || '2:30'), toSeconds(f.max_len || '4')];
      body.recipe = r;
    }
    if (f.kind === 'smart') {
      const r = {};
      if (f.mood) r.mood = f.mood;
      if (f.genre) r.genre = f.genre;
      if (f.kept) r.kept = true;
      if (f.max_minutes) r.max_seconds = Math.round(Number(f.max_minutes) * 60);
      if (f.days) r.made_within_days = Number(f.days);
      body.rule = r;
    }
    return body;
  }

  function crateQuery(f, cursor) {
    const p = new URLSearchParams({ limit: '60', sort: f.sort || 'newest' });
    if (f.q && f.q.trim()) p.set('q', f.q.trim());
    if (f.from) p.set('playlist', f.from);
    if (f.kept) p.set('fav', 'true');
    if (cursor) p.set('cursor', String(cursor));
    return p.toString();
  }

  function trackFrom(t, playlists) {
    if (t.made_by) {
      const p = playlists.find((x) => x.id === t.made_by);
      return p ? `from ${p.name}` : 'from a playlist no longer here';
    }
    return asker(t.requested_by) || 'asked for once';
  }

  /** A radio's listen addresses, full URLs, in a fixed order: the HTTP streams first, then the relay. */
  function listenLinks(listen, origin) {
    return ['mp3', 'aac', 'hls', 'rtsp', 'srt', 'rtmp'].filter((k) => listen?.[k])
      .map((k) => ({ kind: k.toUpperCase(), url: listen[k].startsWith('/') ? origin + listen[k] : listen[k] }));
  }

  /** After the radio list changes: the radio to tune when the tuned one is gone, else null. */
  const retuneTo = (radios, tuned) => (radios.some((r) => r.id === tuned) ? null : pickRadio(radios, tuned));

  /** An edit replaces the whole recipe or rule: keep the fields the form does not show (library share, engine, side...). */
  function mergeEdit(p, body) {
    const out = { ...body };
    if (body.recipe) out.recipe = { ...(p.recipe || {}), ...body.recipe };
    if (body.rule) out.rule = { ...(p.rule || {}), ...body.rule };
    return out;
  }

  /** Notes for the songs a radio will play next from the playlist on air (GET /radios/{r}/on-air). */
  function upcomingNotes(onAir, radioName) {
    const notes = new Map();
    (onAir?.upcoming || []).forEach((id, i) => { if (!notes.has(id)) notes.set(id, `${i === 0 ? 'next' : 'then'} on ${radioName}`); });
    return notes;
  }

  root.UI = { mmss, ago, span, dialSlots, pickRadio, kindLabel, sizeLine, playlistAbout, credits, asker, upNext, songNote, playlistBody, crateQuery, trackFrom, listenLinks, retuneTo, mergeEdit, upcomingNotes };
})(globalThis);
