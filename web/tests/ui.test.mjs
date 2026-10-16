import { test } from 'node:test';
import assert from 'node:assert/strict';

await import('../ui.js');
const UI = globalThis.UI;

test('the_dial_shows_each_radio_once', () => {
  assert.deepEqual(UI.dialSlots(1, 0), [{ offset: 0, index: 0 }]);
  assert.deepEqual(UI.dialSlots(2, 0).map((s) => s.index), [0, 1]);
  assert.deepEqual(UI.dialSlots(4, 0).map((s) => [s.offset, s.index]), [[-1, 3], [0, 0], [1, 1], [2, 2]]);
  assert.deepEqual(UI.dialSlots(7, 6).map((s) => s.index), [4, 5, 6, 0, 1], 'wraps round both sides');
  assert.deepEqual(UI.dialSlots(3, -1).map((s) => s.index), [1, 2, 0], 'negative turns wrap too');
  assert.deepEqual(UI.dialSlots(0, 0), []);
});

test('a_missing_tuned_radio_falls_back_to_main', () => {
  const radios = [{ id: 'main' }, { id: 'lobby' }];
  assert.equal(UI.pickRadio(radios, 'lobby'), 'lobby');
  assert.equal(UI.pickRadio(radios, 'gone'), 'main');
  assert.equal(UI.pickRadio(radios, null), 'main');
  assert.equal(UI.pickRadio([{ id: 'x' }], 'gone'), 'x', 'without main, the first');
});

test('times read like speech', () => {
  const now = Date.parse('2026-10-16T12:00:00Z');
  assert.equal(UI.mmss(184.4), '3:04');
  assert.equal(UI.ago('2026-10-16T11:57:00Z', now), '3 min ago');
  assert.equal(UI.ago('2026-10-16T09:00:00Z', now), '3 h ago');
  assert.equal(UI.ago('2026-10-15T09:00:00Z', now), 'yesterday');
  assert.equal(UI.ago('2026-10-11T12:00:00Z', now), '5 days ago');
  assert.equal(UI.ago(null, now), 'never');
  assert.equal(UI.span(3480), '58 min');
  assert.equal(UI.span(6720), '1 h 52 min');
  assert.equal(UI.span(28), 'under 1 min');
});

test('playlists describe themselves', () => {
  assert.equal(UI.kindLabel({ kind: 'generative' }), 'Generative, makes new songs');
  assert.equal(UI.kindLabel({ kind: 'fixed', order: 'shuffle' }), 'Fixed, shuffled');
  assert.equal(UI.kindLabel({ kind: 'fixed', order: 'in_order' }), 'Fixed, in order');
  assert.equal(UI.kindLabel({ kind: 'smart' }), 'Smart, newest first');
  assert.deepEqual(UI.sizeLine({ count: 50, max_tracks: 50 }), { text: '50 of 50', full: true, pct: 100 });
  assert.deepEqual(UI.sizeLine({ count: 34, max_tracks: 50 }), { text: '34 of 50', full: false, pct: 68 });
  assert.deepEqual(UI.sizeLine({ count: 412, max_tracks: null }), { text: '412 songs', full: false, pct: 0 });
  assert.equal(UI.playlistAbout({ kind: 'generative', recipe: { prompt: 'lo-fi, warm Rhodes', mood: 'calm' } }), 'lo-fi, warm Rhodes; calm');
  assert.equal(UI.playlistAbout({ kind: 'smart', rule: { kept: true, mood: 'calm', max_seconds: 240 } }), 'Kept songs, calm, under 4 minutes');
  assert.equal(UI.playlistAbout({ kind: 'fixed' }), 'Picked by hand from the crate');
});

test('credits and askers', () => {
  assert.deepEqual(UI.credits({ artist: 'Maio', album: 'Late night', key: 'D minor', bpm: 84 }), { artist: 'Maio', album: 'Late night', extra: 'in D minor, 84 bpm' });
  assert.deepEqual(UI.credits({ artist: 'Maio', album: 'Requests' }), { artist: 'Maio', album: 'Requests', extra: '' });
  assert.equal(UI.asker('user:ana'), 'asked by ana');
  assert.equal(UI.asker('playlist'), '');
});

test('up next splits requests from the playlist and counts ahead', () => {
  const q = [
    { entry_id: 'q1', side: 'a', state: 'ready', title: 'Glass Harbour', seconds: 180, by: 'user:ana' },
    { entry_id: 'q2', side: 'a', state: 'generating', title: 'calm', seconds: 120, by: 'user:ana', eta_s: 40 },
    { entry_id: 'q3', side: 'b', state: 'ready', title: 'Cursor Blink', seconds: 178, by: 'playlist' },
  ];
  const u = UI.upNext(q, 112);
  assert.deepEqual(u.a.map((r) => [r.at, r.title, r.note, r.len]), [['+1:52', 'Glass Harbour', 'asked by ana', '3:00'], ['+4:52', 'calm', 'being made, about 40 s', '2:00']]);
  assert.deepEqual(u.b.map((r) => [r.at, r.note]), [['+6:52', 'from the playlist']]);
  assert.equal(u.minutes, 8, '478 s queued');
});

test('song notes on a playlist page', () => {
  const radios = [{ name: 'Main', now_playing: { track: { id: 't1' } } }];
  const now = Date.parse('2026-10-16T12:00:00Z');
  assert.deepEqual(UI.songNote({ id: 't1', favourite: true, created_at: '2026-01-01T00:00:00Z' }, radios, now), { text: 'playing now on Main', live: true });
  assert.deepEqual(UI.songNote({ id: 't2', favourite: true, created_at: '2026-01-01T00:00:00Z' }, radios, now), { text: 'kept', live: false });
  assert.deepEqual(UI.songNote({ id: 't3', favourite: false, created_at: '2026-10-16T08:00:00Z' }, radios, now), { text: 'new today', live: false });
});

test('playlist_form_bodies', () => {
  assert.deepEqual(UI.playlistBody({ name: ' Rainy ', kind: 'fixed', order: 'in_order', max_tracks: '' }), { name: 'Rainy', kind: 'fixed', order: 'in_order' });
  assert.deepEqual(UI.playlistBody({ name: 'Late', kind: 'generative', order: 'shuffle', max_tracks: '80', prompt: 'lo-fi', mood: 'calm', genre: '', variations: 'rain, jazzy chords', vocals: true, min_len: '2:30', max_len: '4' }),
    { name: 'Late', kind: 'generative', order: 'shuffle', max_tracks: 80, recipe: { prompt: 'lo-fi', mood: 'calm', variations: ['rain', 'jazzy chords'], vocals: true, seconds: [150, 240] } });
  assert.deepEqual(UI.playlistBody({ name: 'K', kind: 'smart', order: 'shuffle', max_tracks: '', mood: 'calm', kept: true, max_minutes: '4', days: '' }),
    { name: 'K', kind: 'smart', order: 'shuffle', rule: { mood: 'calm', kept: true, max_seconds: 240 } });
});

test('crate_query_strings', () => {
  assert.equal(UI.crateQuery({ q: ' rain ', from: '', sort: 'played', kept: false }, 0), 'limit=60&sort=played&q=rain');
  assert.equal(UI.crateQuery({ q: '', from: 'requests', sort: 'newest', kept: true }, 60), 'limit=60&sort=newest&playlist=requests&fav=true&cursor=60');
  assert.equal(UI.trackFrom({ made_by: 'pl_1' }, [{ id: 'pl_1', name: 'Late night' }]), 'from Late night');
  assert.equal(UI.trackFrom({ made_by: 'pl_gone' }, []), 'from a playlist no longer here');
  assert.equal(UI.trackFrom({ requested_by: 'user:ana' }, []), 'asked by ana');
});

test('listen_links', () => {
  const links = UI.listenLinks({ mp3: '/listen/k/stream.mp3', hls: '/listen/k/hls/index.m3u8', rtsp: 'rtsp://radio.lan:8555/listen/k' }, 'http://localhost:8080');
  assert.deepEqual(links, [
    { kind: 'MP3', url: 'http://localhost:8080/listen/k/stream.mp3' },
    { kind: 'HLS', url: 'http://localhost:8080/listen/k/hls/index.m3u8' },
    { kind: 'RTSP', url: 'rtsp://radio.lan:8555/listen/k' },
  ]);
});

test('a_deleted_tuned_radio_retunes', () => {
  assert.equal(UI.retuneTo([{ id: 'main' }, { id: 'lobby' }], 'lobby'), null, 'still there: stay');
  assert.equal(UI.retuneTo([{ id: 'main' }], 'lobby'), 'main', 'gone: main');
});

test('editing_keeps_recipe_and_rule_fields_the_form_does_not_show', () => {
  const p = { kind: 'generative', recipe: { prompt: 'old', mood: 'calm', seconds: [20, 30], library_share: 0.6, engine: 'ace-step', lyrics_by: 'claude', tempo: 'slow' } };
  const merged = UI.mergeEdit(p, { name: 'New', recipe: { prompt: 'new', mood: 'calm', seconds: [150, 240] } });
  assert.deepEqual(merged.recipe, { prompt: 'new', mood: 'calm', seconds: [150, 240], library_share: 0.6, engine: 'ace-step', lyrics_by: 'claude', tempo: 'slow' });
  const s = { kind: 'smart', rule: { side: 'b', from_playlist: 'pl_1', mood: 'calm' } };
  assert.deepEqual(UI.mergeEdit(s, { name: 'S', rule: { mood: 'tense' } }).rule, { side: 'b', from_playlist: 'pl_1', mood: 'tense' });
  assert.deepEqual(UI.mergeEdit({ kind: 'fixed' }, { name: 'F' }), { name: 'F' });
});

test('the_playlist_page_marks_what_plays_next', () => {
  const notes = UI.upcomingNotes({ playlist: { id: 'p' }, now_track_id: 't0', upcoming: ['t1', 't2'] }, 'Main');
  assert.deepEqual([notes.get('t1'), notes.get('t2'), notes.get('t9')], ['next on Main', 'then on Main', undefined]);
  assert.equal(UI.upcomingNotes(null, 'Main').size, 0);
});
