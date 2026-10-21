import test from 'node:test';
import assert from 'node:assert/strict';
import { elements, views, connectedIds, edgeGeometry, boundsFor, flowChannelsFor } from '../static/model.mjs';

test('every view places real elements, drills into real views and has a parent on the way up', () => {
  for (const [id, view] of Object.entries(views)) {
    const ids = new Set(view.nodes.map(n => n.id));
    for (const node of view.nodes) {
      assert.ok(elements[node.id], `${id}: missing element ${node.id}`);
      if (elements[node.id].drill) assert.ok(views[elements[node.id].drill], `${id}: ${node.id} drills into nothing`);
    }
    for (const e of view.edges) assert.ok(ids.has(e.from) && ids.has(e.to), `${id}: ${e.from} → ${e.to} leaves the view`);
    assert.equal(ids.size, view.nodes.length, `${id}: an element placed twice`);
    if (view.parent) assert.ok(views[view.parent]);
    assert.ok(view.crumb, `${id}: no breadcrumb`);
  }
  assert.equal(elements.maio.drill, 'containers');
  assert.equal(elements.server.drill, 'components');
  assert.equal(elements.live.drill, 'live');
  assert.equal(elements.webapp.drill, 'webapp');
});

test('adapters implement ports the use cases own, and the domain depends on nothing', () => {
  const view = views.components;
  for (const id of ['sqlite', 'engine', 'text', 'ffmpeg', 'live']) assert.ok(view.edges.some(e => e.from === id && e.to === 'usecases' && e.dashed), `${id}`);
  assert.ok(!view.edges.some(e => e.from === 'domain'));
  assert.ok(!view.edges.some(e => e.from === 'usecases' && ['sqlite', 'engine', 'text', 'ffmpeg', 'live'].includes(e.to)));
});

test('animated flows follow edges of their view', () => {
  for (const [id, view] of Object.entries(views)) for (const flow of view.flows ?? []) for (const leg of flow.legs) {
    assert.ok(view.edges.some(e => e.from === leg.from && e.to === leg.to), `${id}: flow leg ${leg.from} → ${leg.to} has no edge`);
  }
  assert.deepEqual(flowChannelsFor(views.context).map(f => f.id), ['input', 'output']);
  assert.deepEqual(flowChannelsFor(views.containers).map(f => f.id), ['input', 'output'], 'the containers carry the same requests and audio');
  assert.deepEqual(flowChannelsFor({ ...views.context, edges: [] }), []);
});

test('selection highlights only the element and its direct neighbours', () => {
  assert.deepEqual([...connectedIds(views.components, 'web')].sort(), ['usecases', 'web']);
});

test('edges meet node borders, boxes do not overlap, and the fit bounds hold everything', () => {
  for (const [id, view] of Object.entries(views)) {
    const nodes = new Map(view.nodes.map(n => [n.id, n])), bounds = boundsFor(view);
    for (const n of view.nodes) {
      assert.ok(n.x >= bounds.x && n.y >= bounds.y && n.x + n.width <= bounds.x + bounds.width && n.y + n.height <= bounds.y + bounds.height);
      for (const m of view.nodes) if (m !== n) assert.ok(n.x + n.width <= m.x || m.x + m.width <= n.x || n.y + n.height <= m.y || m.y + m.height <= n.y, `${id}: ${n.id} overlaps ${m.id}`);
    }
    for (const e of view.edges) {
      const line = edgeGeometry(nodes.get(e.from), nodes.get(e.to));
      for (const [point, original] of [[line.start, nodes.get(e.from)], [line.end, nodes.get(e.to)]]) {
        const box = elements[original.id].type === 'Person' ? { ...original, x: original.x + 30, y: original.y + 52, width: 190, height: 96 } : original;
        const onSide = Math.abs(point.x - box.x) < .001 || Math.abs(point.x - box.x - box.width) < .001;
        const onTop = Math.abs(point.y - box.y) < .001 || Math.abs(point.y - box.y - box.height) < .001;
        assert.ok(onSide || onTop, `${id}: ${e.from} → ${e.to} misses a border`);
      }
    }
  }
});

test('the deployment view puts Docker compose on any Docker host, and ACE-Step on the machine with its GPU', () => {
  const view = views.deployment;
  assert.ok(view, 'a deployment view');
  const [host, docker, engine] = view.boundaries;
  const inside = (n, b) => n.x >= b.x && n.y >= b.y && n.x + n.width <= b.x + b.width && n.y + n.height <= b.y + b.height;
  assert.match(host.label, /Docker host/);
  assert.match(engine.label, /ACE-Step machine/);
  assert.ok(!/Metal|Mac/.test(engine.label + view.description), 'ACE-Step is not tied to a Mac');
  assert.ok(inside(docker, host), 'Docker compose sits on the Docker host');
  assert.ok(!inside(docker, engine) && !inside(engine, host), 'the Docker host is drawn apart from the ACE-Step machine');
  const at = id => view.nodes.find(n => n.id === id);
  for (const id of ['deployServer', 'deployMediamtx', 'deployData']) assert.ok(inside(at(id), docker), `${id} inside Docker`);
  assert.ok(inside(at('deployAcestep'), engine) && !inside(at('deployAcestep'), host), 'ACE-Step on its machine, off the Docker host');
  assert.ok(!/on the Mac/.test(views.containers.boundary.label), 'the containers do not need a Mac');
});


test('the C4 model writes the name maio, in lower case, as the wordmark does', async () => {
  const { readFile } = await import('node:fs/promises');
  const source = await readFile(new URL('../static/model.mjs', import.meta.url), 'utf8');
  assert.deepEqual(source.match(/\bMaio\b/g) ?? [], []);
});
