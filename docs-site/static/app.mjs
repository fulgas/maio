import { elements, views, resolveView, connectedIds, boundsFor, boundariesOf, relationshipGeometry, wrapText, flowChannelsFor, nodeShape } from './model.mjs';

const $ = id => document.getElementById(id);
const ns = 'http://www.w3.org/2000/svg';
function svg(tag, attrs = {}, text) {
  const node = document.createElementNS(ns, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  if (text !== undefined) node.textContent = text;
  return node;
}
function html(tag, attrs = {}, text) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  if (text !== undefined) node.textContent = text;
  return node;
}
const state = { view: 'context', selected: null, x: 0, y: 0, scale: 1, dragging: null, moved: false };
let currentView, documents = [], mermaidReady, navigationBeforeSearch;
const diagram = $('diagram'), viewport = $('viewport');
const announce = message => { $('status').textContent = message; };
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const flow = { playing: false, elapsed: 0, started: 0, frame: null };
const flowSpeed = 30; // SVG units per second; keep the illustration calm at every zoom.

function pauseFlow() {
  if (flow.frame !== null) cancelAnimationFrame(flow.frame);
  flow.frame = null; flow.playing = false;
}
function resetFlow() {
  pauseFlow(); flow.elapsed = 0;
  viewport.querySelector('.flow-overlay')?.remove();
}
function positionFlow(elapsed) {
  const distance = elapsed / 1000 * flowSpeed;
  for (const connector of viewport.querySelectorAll('.flow-overlay path')) {
    const period = connector.classList.contains('input') ? 18 : 11;
    connector.setAttribute('stroke-dashoffset', -(distance % period));
  }
}
function drawFlowChannels() {
  const channels = flowChannelsFor(currentView);
  if (!channels.length) return;
  viewport.querySelector('.flow-overlay')?.remove();
  const overlay = svg('g', { class: 'flow-overlay', 'aria-hidden': 'true' });
  for (const channel of channels) {
    for (const leg of channel.legs) {
      const { start, end } = relationshipGeometry(currentView, leg);
      overlay.append(svg('path', { class: channel.id, d: `M${start.x},${start.y} L${end.x},${end.y}`, 'data-from': leg.from, 'data-to': leg.to, 'marker-end': `url(#flow-${channel.id}-arrow)` }));
    }
  }
  viewport.append(overlay); positionFlow(flow.elapsed);
}
function animateFlow(now) {
  if (!flow.playing) return;
  flow.elapsed = now - flow.started;
  positionFlow(flow.elapsed);
  flow.frame = requestAnimationFrame(animateFlow);
}
function syncFlow() {
  const supported = Boolean(currentView && flowChannelsFor(currentView).length);
  $('flow-key').hidden = !supported;
  if (!supported || reducedMotion.matches || document.hidden || $('explorer').hidden || $('canvas-panel').querySelector(':focus-visible')) {
    pauseFlow();
    return;
  }
  if (flow.playing) return;
  flow.playing = true; flow.started = performance.now() - flow.elapsed;
  drawFlowChannels();
  flow.frame = requestAnimationFrame(animateFlow);
}
$('canvas-panel').addEventListener('pointerdown', () => queueMicrotask(syncFlow));
$('canvas-panel').addEventListener('focusin', syncFlow);
$('canvas-panel').addEventListener('focusout', () => queueMicrotask(syncFlow));
reducedMotion.addEventListener('change', () => { resetFlow(); syncFlow(); });
document.addEventListener('visibilitychange', syncFlow);

function graph() {
  return resolveView(state.view);
}
function applyTransform() {
  viewport.setAttribute('transform', `translate(${state.x} ${state.y}) scale(${state.scale})`);
  $('zoom-label').textContent = `${Math.round(state.scale * 100)}%`;
}
function fit() {
  if ($('explorer').hidden) return;
  const bounds = boundsFor(currentView ?? graph());
  const width = diagram.clientWidth, height = diagram.clientHeight;
  if (!width || !height) return;
  state.scale = Math.min((width - 40) / bounds.width, (height - 100) / bounds.height, 1.3);
  state.x = (width - bounds.width * state.scale) / 2 - bounds.x * state.scale;
  state.y = (height - bounds.height * state.scale) / 2 - bounds.y * state.scale;
  applyTransform();
}
function zoom(factor, x = diagram.clientWidth / 2, y = diagram.clientHeight / 2) {
  const next = Math.max(.18, Math.min(2.8, state.scale * factor));
  const ratio = next / state.scale;
  state.x = x - (x - state.x) * ratio;
  state.y = y - (y - state.y) * ratio;
  state.scale = next;
  applyTransform();
}
function drillTarget(item) {
  return item.drill;
}
function draw() {
  currentView = graph();
  resetFlow();
  viewport.replaceChildren();
  for (const b of boundariesOf(currentView)) {
    viewport.append(svg('rect', { x: b.x, y: b.y, width: b.width, height: b.height, rx: 16, class: 'c4-boundary' }));
    viewport.append(svg('text', { x: b.x + 20, y: b.y + 25, class: 'boundary-label' }, b.label));
  }
  for (const edge of currentView.edges) {
    const line = relationshipGeometry(currentView, edge);
    const group = svg('g', { class: `edge${edge.dashed ? ' dashed' : ''}${edge.channel ? ` channel-${edge.channel}` : ''}`, 'data-from': edge.from, 'data-to': edge.to });
    const attrs = { d: `M${line.start.x},${line.start.y} L${line.end.x},${line.end.y}`, 'marker-end': edge.channel ? `url(#flow-${edge.channel}-arrow)` : 'url(#arrow)' };
    if (edge.both) attrs['marker-start'] = 'url(#arrow)';
    group.append(svg('path', attrs), svg('text', { x: line.label.x, y: line.label.y, 'text-anchor': 'middle' }, edge.label));
    viewport.append(group);
  }
  for (const node of currentView.nodes) {
    const item = elements[node.id];
    const target = drillTarget(item);
    const { body, head } = nodeShape(node), person = Boolean(head);
    const textX = person ? body.x + body.width / 2 : 17;
    const titleLines = wrapText(item.name, person ? 21 : 27, 2);
    const personText = person ? { 'text-anchor': 'middle' } : {};
    const extraTitleHeight = (titleLines.length - 1) * 18;
    const group = svg('g', { class: `c4-node${person ? ' person' : ''}${item.external ? ' external' : ''}${item.optional ? ' optional' : ''}`, transform: `translate(${node.x} ${node.y})`, 'data-id': node.id, tabindex: 0, role: 'button', 'aria-label': `${item.name}. ${item.description}${target ? ' Press Enter to inspect or use the details button to explore inside.' : ''}`, 'aria-pressed': String(state.selected === node.id) });
    group.append(svg('title', {}, item.description));
    group.append(svg('rect', { ...body, class: 'node-body' }));
    if (person) group.append(svg('circle', { ...head, class: 'node-body person-head' }));
    group.append(svg('text', { x: textX, y: person ? body.y + 46 + extraTitleHeight : 24, class: 'node-type', ...personText }, person ? '[Person]' : item.type.toUpperCase()));
    if (!person) group.append(svg('text', { x: node.width - 17, y: 24, class: 'node-icon', 'text-anchor': 'end' }, target ? '↘' : '◇'));
    titleLines.forEach((text, i) => group.append(svg('text', { x: textX, y: (person ? body.y + 27 : 52) + i * (person ? 18 : 20), class: 'node-title', ...personText }, text)));
    const startY = person ? body.y + 64 + extraTitleHeight : titleLines.length > 1 ? 94 : 78;
    wrapText(item.description, person ? 23 : 34, person ? titleLines.length > 1 ? 1 : 2 : item.drill ? 2 : 3).forEach((text, i) => group.append(svg('text', { x: textX, y: startY + i * (person ? 16 : 17), class: 'node-description', ...personText }, text)));
    if (target) group.append(svg('text', { x: 17, y: node.height - 15, class: 'node-drill' }, 'Explore inside →'));
    group.addEventListener('click', event => { event.stopPropagation(); if (!state.moved) select(node.id); });
    group.addEventListener('dblclick', event => { event.stopPropagation(); if (target) location.hash = `view/${target}`; });
    group.addEventListener('keydown', event => {
      if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(node.id); }
      if (event.key === 'ArrowRight' && target) { event.preventDefault(); location.hash = `view/${target}`; }
    });
    viewport.append(group);
  }
  highlight();
  syncFlow();
}
function highlight() {
  const related = state.selected ? connectedIds(currentView, state.selected) : null;
  for (const node of viewport.querySelectorAll('.c4-node')) {
    const id = node.dataset.id;
    node.classList.toggle('selected', id === state.selected);
    node.classList.toggle('dimmed', related !== null && !related.has(id));
    node.setAttribute('aria-pressed', String(id === state.selected));
  }
  for (const edge of viewport.querySelectorAll('.edge')) edge.classList.toggle('dimmed', related !== null && edge.dataset.from !== state.selected && edge.dataset.to !== state.selected);
}
function select(id) {
  state.selected = id;
  highlight();
  const panel = $('details-content');
  panel.replaceChildren();
  $('clear-selection').disabled = !id;
  if (!id) {
    panel.append(html('div', { class: 'empty-icon', 'aria-hidden': 'true' }, '⌖'), html('h2', {}, 'Explore the architecture'), html('p', {}, 'Select an element to inspect its responsibilities and connections. Double-click a box marked "Explore inside" to move to the next C4 level.'), html('h3', {}, 'The C4 levels'), html('p', {}, 'Context → Containers → Components. Live stations, the Web app and the Deployment open from their boxes and from the architecture.'), html('p', { class: 'quiet' }, 'This model describes maio as it runs today. Each element links to the section of the architecture that describes it.'));
    return;
  }
  const item = elements[id];
  panel.append(html('span', { class: 'type-chip' }, item.type), html('h2', {}, item.name), html('p', {}, item.description));
  if (item.optional) panel.append(html('p', { class: 'quiet' }, 'Optional: used only when it is set up.'));
  const target = drillTarget(item);
  if (target) {
    const button = html('button', { class: 'primary' }, 'Explore inside →');
    button.addEventListener('click', () => { location.hash = `view/${target}`; });
    panel.append(button);
  }
  if (item.responsibilities?.length) {
    panel.append(html('h3', {}, 'Responsibilities'));
    const list = html('ul');
    item.responsibilities.forEach(text => list.append(html('li', {}, text)));
    panel.append(list);
  }
  const edges = currentView.edges.filter(e => e.from === id || e.to === id);
  if (edges.length) {
    panel.append(html('h3', {}, 'Connections'));
    for (const edge of edges) {
      const other = edge.from === id ? edge.to : edge.from;
      const button = html('button', { class: 'neighbor' });
      button.append(document.createTextNode(elements[other].name), html('small', {}, edge.label));
      button.addEventListener('click', () => select(other));
      panel.append(button);
    }
  }
  if (item.doc) panel.append(html('h3', {}, 'Related documentation'), html('a', { class: 'text-link', href: `#doc/${item.doc}` }, 'Read it in the architecture ↗'));
  announce(`Selected ${item.name}`);
}
function showView(id) {
  state.view = views[id] ? id : 'context';
  state.selected = null;
  $('explorer').hidden = false;
  $('reader').hidden = true;
  const view = views[state.view];
  $('view-level').textContent = view.level;
  $('view-title').textContent = view.title;
  $('view-description').textContent = view.description;
  $('view-hint').textContent = view.hint;
  $('scope').textContent = view.scope;
  for (const button of $('breadcrumbs').querySelectorAll('button')) {
    const active = button.dataset.view === view.crumb;
    button.classList.toggle('active', active && !view.deeper);
    button.setAttribute('aria-current', active && !view.deeper ? 'step' : 'false');
  }
  // A view below the three levels shows on the path: Components › Live stations.
  $('breadcrumbs').querySelectorAll('.crumb-deeper').forEach(n => n.remove());
  if (view.deeper) {
    const at = [...$('breadcrumbs').querySelectorAll('button')].find(b => b.dataset.view === view.crumb);
    at.after(html('span', { class: 'crumb-divider crumb-deeper', 'aria-hidden': 'true' }, '›'), html('span', { class: 'crumb-here crumb-deeper', 'aria-current': 'step' }, view.title));
  }
  document.title = `${view.title} · Maio docs`;
  draw(); select(null); requestAnimationFrame(fit);
}
function showDocument(id, section) {
  resetFlow();
  const doc = documents.find(d => d.id === id);
  $('reader').hidden = false;
  $('explorer').hidden = true;
  if (!doc) {
    $('document').replaceChildren(html('h1', {}, 'Document not found'), html('a', { href: '#doc/architecture/01-introduction-and-goals' }, 'Open the architecture'));
    $('toc').replaceChildren(); $('doc-label').textContent = 'Unknown document';
    return;
  }
  // HTML is generated at build time from trusted repository Markdown, not user input.
  $('document').innerHTML = doc.html;
  $('doc-label').textContent = { decisions: 'ARCHITECTURE DECISION RECORD', architecture: 'ARCHITECTURE · ARC42', project: 'PROJECT' }[doc.group];
  $('toc').replaceChildren(...doc.headings.filter(h => h.depth === 2 || h.depth === 3).map(h => html('a', { href: `#doc/${doc.id}?section=${encodeURIComponent(h.slug)}`, class: h.depth === 3 ? 'subheading' : '' }, h.text)));
  for (const link of $('docs-nav').querySelectorAll('a')) {
    const active = link.dataset.id === doc.id;
    link.classList.toggle('active', active);
    if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
  }
  revealCurrentDocument();
  document.title = `${doc.title} · Maio docs`;
  requestAnimationFrame(() => {
    const destination = section && [...$('document').querySelectorAll('[id]')].find(n => n.id === section);
    if (destination) destination.scrollIntoView({ block: 'start' }); else window.scrollTo(0, 0);
  });
  renderMermaid();
}
async function renderMermaid() {
  const nodes = [...$('document').querySelectorAll('.mermaid')];
  if (!nodes.length) return;
  try {
    if (!mermaidReady && window.mermaid) mermaidReady = Promise.resolve();
    if (!mermaidReady) mermaidReady = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = new URL('vendor/mermaid.min.js', import.meta.url); script.onload = resolve; script.onerror = reject;
      document.head.append(script);
    });
    await mermaidReady;
    if (!nodes.every(n => n.isConnected)) return;
    const styles = getComputedStyle(document.documentElement);
    const color = name => styles.getPropertyValue(`--${name}`).trim();
    window.mermaid.initialize({
      startOnLoad: false, theme: 'base', securityLevel: 'strict',
      fontFamily: styles.fontFamily,
      themeVariables: {
        darkMode: document.documentElement.dataset.theme !== 'light',
        background: color('panel'), primaryColor: color('node'), primaryTextColor: color('text'), primaryBorderColor: color('node-line'),
        secondaryColor: color('external'), secondaryTextColor: color('text'), secondaryBorderColor: color('external-line'),
        tertiaryColor: color('panel-raised'), tertiaryTextColor: color('text'), tertiaryBorderColor: color('line'),
        lineColor: color('edge'), textColor: color('text'), edgeLabelBackground: color('panel'),
        mainBkg: color('node'), nodeBorder: color('node-line'), clusterBkg: color('panel'), clusterBorder: color('line'),
        actorBkg: color('node'), actorBorder: color('node-line'), actorTextColor: color('text'), actorLineColor: color('edge'),
        signalColor: color('text'), signalTextColor: color('text'), labelBoxBkgColor: color('panel'), labelBoxBorderColor: color('line'), labelTextColor: color('text'),
        noteBkgColor: color('accent-soft'), noteTextColor: color('text'), noteBorderColor: color('node-line'),
        activationBkgColor: color('accent-soft'), activationBorderColor: color('node-line'),
      },
      flowchart: { curve: 'linear', htmlLabels: false, nodeSpacing: 36, rankSpacing: 60, padding: 18 },
    });
    await window.mermaid.run({ nodes });
    for (const node of nodes) if (node.isConnected) addZoom(node);
  } catch { announce('A documentation diagram could not be rendered. Its source remains in the document.'); }
}
// A Mermaid diagram opens in the browser's own dialog, where it can be zoomed and panned.
const zoomDialog = $('diagram-zoom'), zoomCanvas = $('diagram-zoom-canvas');
let zoomScale = 1, zoomWidth = 0;
// The diagram itself is the control: a click, or Enter or Space when it has focus.
function addZoom(node) {
  const heading = [...$('document').querySelectorAll('h2, h3')].filter(h => h.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING).at(-1);
  const title = heading?.textContent ?? 'Diagram';
  Object.assign(node, { tabIndex: 0, title: 'Click to zoom' });
  node.setAttribute('role', 'button');
  node.setAttribute('aria-label', `Zoom the diagram: ${title}`);
  node.addEventListener('click', () => openZoom(node.querySelector('svg'), title));
  node.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); openZoom(node.querySelector('svg'), title); } });
}
function setZoom(scale) {
  zoomScale = Math.min(4, Math.max(0.25, scale));
  const svg = zoomCanvas.querySelector('svg');
  svg.style.width = `${zoomWidth * zoomScale}px`;
  $('diagram-zoom-level').textContent = `${Math.round(zoomScale * 100)}%`;
}
function fitZoom() { setZoom((zoomCanvas.clientWidth - 32) / zoomWidth); }
function openZoom(svg, title) {
  const copy = svg.cloneNode(true);
  copy.removeAttribute('style'); copy.style.maxWidth = 'none'; copy.style.height = 'auto';
  zoomWidth = svg.viewBox.baseVal?.width || svg.getBoundingClientRect().width;
  $('diagram-zoom-title').textContent = title;
  zoomCanvas.replaceChildren(copy);
  zoomDialog.showModal();
  fitZoom();
  zoomCanvas.focus();
}
zoomDialog.querySelector('.zoom-actions').addEventListener('click', event => {
  const action = event.target.closest('button')?.dataset.zoom;
  if (action === 'in') setZoom(zoomScale * 1.25);
  if (action === 'out') setZoom(zoomScale / 1.25);
  if (action === 'fit') fitZoom();
  if (action === 'close') zoomDialog.close();
});
zoomCanvas.addEventListener('wheel', event => {
  if (!event.ctrlKey && !event.metaKey) return;
  event.preventDefault();
  setZoom(zoomScale * (event.deltaY < 0 ? 1.1 : 1 / 1.1));
}, { passive: false });
zoomCanvas.addEventListener('keydown', event => {
  if (event.key === '+' || event.key === '=') setZoom(zoomScale * 1.25);
  if (event.key === '-') setZoom(zoomScale / 1.25);
  if (event.key === '0') fitZoom();
});
let drag;
zoomCanvas.addEventListener('pointerdown', event => { drag = { x: event.clientX, y: event.clientY, left: zoomCanvas.scrollLeft, top: zoomCanvas.scrollTop }; zoomCanvas.setPointerCapture(event.pointerId); });
zoomCanvas.addEventListener('pointermove', event => { if (drag) { zoomCanvas.scrollLeft = drag.left - (event.clientX - drag.x); zoomCanvas.scrollTop = drag.top - (event.clientY - drag.y); } });
zoomCanvas.addEventListener('pointerup', () => { drag = undefined; });
zoomDialog.addEventListener('click', event => { if (event.target === zoomDialog) zoomDialog.close(); });
function route() {
  const hash = location.hash.slice(1);
  const [path, query] = hash.split('?');
  const docRoute = path.startsWith('doc/');
  const id = path.slice(4);
  for (const link of document.querySelectorAll('[data-tab]')) {
    const decision = id.startsWith('adr/') || id === 'architecture/09-architecture-decisions';
    const active = docRoute ? (decision ? link.dataset.tab === 'decisions' : link.dataset.tab === 'architecture') : link.dataset.tab === 'view';
    link.classList.toggle('active', active);
    if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
  }
  if (docRoute) showDocument(id, new URLSearchParams(query).get('section'));
  else showView(path.startsWith('view/') ? path.slice(5) : 'context');
}

for (const button of $('breadcrumbs').querySelectorAll('button')) button.addEventListener('click', () => { location.hash = `view/${button.dataset.view}`; });
$('fit').addEventListener('click', fit);
$('zoom-in').addEventListener('click', () => zoom(1.2));
$('zoom-out').addEventListener('click', () => zoom(1 / 1.2));
$('clear-selection').addEventListener('click', () => select(null));
$('fullscreen').addEventListener('click', async () => {
  try { if (document.fullscreenElement) await document.exitFullscreen(); else await $('explorer').requestFullscreen(); }
  catch { announce('Fullscreen is unavailable in this browser.'); }
});
diagram.addEventListener('wheel', event => { event.preventDefault(); const r = diagram.getBoundingClientRect(); zoom(Math.exp(-event.deltaY * .0015), event.clientX - r.left, event.clientY - r.top); }, { passive: false });
diagram.addEventListener('pointerdown', event => {
  if (event.button !== 0 || event.target.closest('.c4-node')) return;
  state.dragging = { x: event.clientX, y: event.clientY, originX: state.x, originY: state.y };
  state.moved = false; diagram.setPointerCapture(event.pointerId); diagram.classList.add('panning');
});
diagram.addEventListener('pointermove', event => {
  if (!state.dragging) return;
  const dx = event.clientX - state.dragging.x, dy = event.clientY - state.dragging.y;
  if (Math.abs(dx) + Math.abs(dy) > 4) state.moved = true;
  state.x = state.dragging.originX + dx; state.y = state.dragging.originY + dy; applyTransform();
});
function endPan(event) { if (!state.dragging) return; state.dragging = null; diagram.classList.remove('panning'); if (diagram.hasPointerCapture(event.pointerId)) diagram.releasePointerCapture(event.pointerId); setTimeout(() => { state.moved = false; }, 0); }
diagram.addEventListener('pointerup', endPan); diagram.addEventListener('pointercancel', endPan);
diagram.addEventListener('click', event => { if (!state.moved && !event.target.closest('.c4-node')) select(null); });
document.addEventListener('keydown', event => {
  if (event.target.closest('input,textarea,select') || $('explorer').hidden) return;
  if (event.key === 'Escape') select(null);
  if (event.key.toLowerCase() === 'f' && !event.ctrlKey && !event.metaKey) { event.preventDefault(); fit(); }
  if (event.key === '+' || event.key === '=') zoom(1.2);
  if (event.key === '-') zoom(1 / 1.2);
});
new ResizeObserver(() => { if (!$('explorer').hidden) fit(); }).observe($('canvas-panel'));
$('export').addEventListener('click', () => {
  const clone = diagram.cloneNode(true), bounds = boundsFor(currentView);
  clone.setAttribute('viewBox', `${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}`);
  clone.setAttribute('width', bounds.width); clone.setAttribute('height', bounds.height);
  clone.removeAttribute('tabindex');
  clone.querySelector('#viewport').removeAttribute('transform');
  clone.querySelector('.flow-overlay')?.remove();
  const style = svg('style');
  const vars = getComputedStyle(document.documentElement);
  const variableNames = ['text', 'muted', 'line', 'accent', 'node', 'node-line', 'external', 'external-line', 'edge', 'canvas', 'grid', 'flow-input', 'flow-output'];
  let rules = `svg{${variableNames.map(n => `--${n}:${vars.getPropertyValue(`--${n}`)};`).join('')}font-family:'Albert Sans',system-ui,sans-serif;background:var(--canvas)}`;
  for (const sheet of document.styleSheets) { try { rules += [...sheet.cssRules].filter(r => r.selectorText && /c4-|node-|edge|boundary|dimmed/.test(r.selectorText)).map(r => r.cssText).join('\n'); } catch {} }
  style.textContent = rules; clone.prepend(style);
  const download = URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(clone)], { type: 'image/svg+xml;charset=utf-8' }));
  const link = html('a', { href: download, download: `maio-${state.view}.svg` }); link.click(); setTimeout(() => URL.revokeObjectURL(download), 1000);
  announce('Diagram exported as SVG');
});
function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $('theme').setAttribute('aria-label', theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme');
  try { localStorage.setItem('maio-architecture-theme', theme); } catch {}
}
try { setTheme(localStorage.getItem('maio-architecture-theme') ?? 'dark'); } catch { setTheme('dark'); }
$('theme').addEventListener('click', () => { setTheme(document.documentElement.dataset.theme === 'light' ? 'dark' : 'light'); if (!$('reader').hidden) route(); });
const documentNavigationLayout = matchMedia('(min-width:701px)');
const syncDocumentNavigation = () => { $('document-navigation').open = documentNavigationLayout.matches; };
documentNavigationLayout.addEventListener('change', syncDocumentNavigation);
syncDocumentNavigation();
function revealCurrentDocument() {
  const active = $('docs-nav').querySelector('a[aria-current="page"]');
  if (!active || active.hidden) return;
  for (let branch = active.closest('details'); branch && $('docs-nav').contains(branch); branch = branch.parentElement.closest('details')) branch.open = true;
}
function filterDocuments() {
  const query = $('doc-search').value.toLowerCase().trim();
  const branches = [...$('docs-nav').querySelectorAll('details')];
  if (query && !navigationBeforeSearch) navigationBeforeSearch = new Map(branches.map(branch => [branch, branch.open]));
  for (const link of $('docs-nav').querySelectorAll('a')) {
    const topics = [];
    for (let branch = link.closest('details'); branch && $('docs-nav').contains(branch); branch = branch.parentElement.closest('details')) topics.push(branch.querySelector(':scope > summary .nav-topic-title')?.textContent ?? '');
    link.hidden = Boolean(query) && !`${link.textContent} ${topics.join(' ')}`.toLowerCase().includes(query);
  }
  for (const branch of branches.reverse()) {
    const matches = [...branch.querySelectorAll('a')].some(link => !link.hidden);
    branch.hidden = Boolean(query) && !matches;
    if (query) branch.open = matches;
    else if (navigationBeforeSearch) branch.open = navigationBeforeSearch.get(branch);
  }
  if (!query) { navigationBeforeSearch = undefined; revealCurrentDocument(); }
}
$('doc-search').addEventListener('input', filterDocuments);
window.addEventListener('hashchange', route);
async function start() {
  try {
    const embedded = $('documentation-data');
    if (embedded) documents = JSON.parse(embedded.textContent);
    else {
      const response = await fetch(new URL('documents.json', import.meta.url));
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      documents = await response.json();
    }
    const nav = $('docs-nav');
    const documentLink = doc => html('a', { href: `#doc/${doc.id}`, 'data-id': doc.id }, doc.navTitle ?? doc.title);
    const branch = (id, title, entries, cls) => {
      const menu = html('details', { id, ...(cls ? { class: cls } : {}) });
      const summary = html('summary');
      summary.append(html('span', { class: 'nav-topic-title' }, title), html('span', { class: 'nav-count' }, entries.length));
      menu.append(summary, ...entries.map(documentLink));
      return menu;
    };
    nav.append(html('p', { class: 'nav-group' }, 'PROJECT'));
    for (const doc of documents.filter(d => d.group === 'project')) nav.append(documentLink(doc));
    nav.append(html('p', { class: 'nav-group' }, 'ARCHITECTURE'));
    // Section 9 is the decision index: it opens to its records, each shown by number and title.
    const decisions = documents.filter(doc => /^adr\/\d/.test(doc.id));
    const recordLink = doc => {
      const link = html('a', { href: `#doc/${doc.id}`, 'data-id': doc.id, class: 'adr-link' });
      link.append(html('span', { class: 'adr-number' }, doc.id.slice(4, 8)), document.createTextNode(doc.title.replace(/^ADR \d+:\s*/, '')));
      return link;
    };
    for (const doc of documents.filter(d => d.group === 'architecture')) {
      if (doc.id !== 'architecture/09-architecture-decisions') { nav.append(documentLink(doc)); continue; }
      const menu = html('details', { id: 'adr-menu', class: 'nav-section' });
      const summary = html('summary');
      summary.append(documentLink(doc), html('span', { class: 'nav-count', title: `${decisions.length} decision records` }, decisions.length));
      const records = html('div', { class: 'adr-records', role: 'group', 'aria-label': 'Architecture decision records' });
      records.append(...decisions.map(recordLink));
      menu.append(summary, records);
      nav.append(menu);
    }
    $('adr-count').textContent = decisions.length;
    filterDocuments();
  } catch {
    $('docs-nav').append(html('p', {}, 'The documentation could not be loaded. Serve this folder over HTTP (just docs-serve) or open architecture.html.'));
    announce('Document loading failed. The C4 explorer is still available.');
  }
  route();
}
start();
