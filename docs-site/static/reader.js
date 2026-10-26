// The reader's behaviour on a Zola page, copied from static/app.mjs: Mermaid in our colours with the zoom
// dialog, the document filter, the open branch of the current document, the theme, and the folding navigation.
const $ = id => document.getElementById(id);
function html(tag, attrs = {}, text) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  if (text !== undefined) node.textContent = text;
  return node;
}

// Where this script is served from, so the site works under any path (GitHub Pages serves it under /maio/).
const here = document.currentScript.src;
let mermaidReady, navigationBeforeSearch;
const announce = message => { $('status').textContent = message; };
async function renderMermaid() {
  const nodes = [...$('document').querySelectorAll('.mermaid')];
  if (!nodes.length) return;
  try {
    if (!mermaidReady && window.mermaid) mermaidReady = Promise.resolve();
    if (!mermaidReady) mermaidReady = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = new URL('vendor/mermaid.min.js', here); script.onload = resolve; script.onerror = reject;
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
function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $('theme').setAttribute('aria-label', theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme');
  try { localStorage.setItem('maio-architecture-theme', theme); } catch {}
}
try { setTheme(localStorage.getItem('maio-architecture-theme') ?? 'dark'); } catch { setTheme('dark'); }
$('theme').addEventListener('click', () => { setTheme(document.documentElement.dataset.theme === 'light' ? 'dark' : 'light'); location.reload(); });

const documentNavigationLayout = matchMedia('(min-width:701px)');
const syncDocumentNavigation = () => { $('document-navigation').open = documentNavigationLayout.matches; };
documentNavigationLayout.addEventListener('change', syncDocumentNavigation);
syncDocumentNavigation();
revealCurrentDocument();
// Mermaid blocks arrive as code; the reader draws them as our site does.
for (const code of document.querySelectorAll('code[data-lang="mermaid"], code.language-mermaid')) {
  const pre = document.createElement('pre'); pre.className = 'mermaid'; pre.textContent = code.textContent; code.closest('pre').replaceWith(pre);
}
renderMermaid();

// Clips of the web app: a click, or Enter, opens one large in the browser's own dialog.
{
  let zoom;
  const open = clip => {
    if (!zoom) {
      zoom = document.createElement('dialog');
      zoom.className = 'clip-zoom';
      zoom.addEventListener('click', e => { if (e.target === zoom) zoom.close(); });
      document.body.append(zoom);
    }
    const close = document.createElement('button');
    close.className = 'clip-close'; close.textContent = 'Close';
    close.addEventListener('click', () => zoom.close());
    // The picture as shown, the still one too when less motion was asked for.
    const big = clip.closest('picture').cloneNode(true);
    big.querySelector('img').removeAttribute('class');
    zoom.setAttribute('aria-label', clip.alt || 'Clip');
    zoom.replaceChildren(close, big);
    zoom.showModal();
  };
  for (const clip of document.querySelectorAll('img.clip')) {
    clip.tabIndex = 0;
    clip.setAttribute('role', 'button');
    clip.addEventListener('click', () => open(clip));
    clip.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); open(clip); } });
  }
}
