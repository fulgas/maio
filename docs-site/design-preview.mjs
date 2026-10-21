// Draws DESIGN.md's token blocks at build time: colour swatches with their contrast, type specimens, corners,
// spaces and each component built from its tokens, on a dark and a light stage. The picture comes from the same
// YAML as the values, so the two cannot disagree.
const escape = text => String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const hex = value => /^#[0-9a-f]{6}$/i.test(value) ? value : undefined;
const dimension = value => /^-?\d+(\.\d+)?(px|em|rem)$/.test(value) ? value : undefined;

/** The subset of YAML the format uses: nested maps, two spaces per level, scalar values, optionally quoted. */
export function parseTokens(text) {
  const root = {}, stack = [{ indent: -1, map: root }];
  for (const line of text.split('\n')) {
    const m = line.match(/^( *)([\w-]+):\s*(.*)$/);
    if (!m) continue;
    const [, spaces, key, raw] = m;
    while (stack.at(-1).indent >= spaces.length) stack.pop();
    const parent = stack.at(-1).map;
    if (raw === '') stack.push({ indent: spaces.length, map: parent[key] = {} });
    else parent[key] = raw.replace(/^"(.*)"$/, '$1');
  }
  return root;
}

/** Every ```yaml block in a DESIGN.md, merged into one token tree. */
export const tokensOf = markdown => Object.assign({}, ...[...markdown.matchAll(/```yaml\n([\s\S]*?)```/g)].map(m => parseTokens(m[1])));

function luminance(colour) {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(colour.slice(i, i + 2), 16) / 255).map(c => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
export const contrast = (a, b) => { const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x); return (hi + 0.05) / (lo + 0.05); };

/** A `{group.name}` reference; on the light stage a colour prefers its `-light` value. */
function resolve(tokens, value, light) {
  const m = typeof value === 'string' && value.match(/^\{([\w-]+)\.([\w-]+)\}$/);
  if (!m) return value;
  const [, group, name] = m;
  return (light && group === 'colors' && tokens.colors?.[`${name}-light`]) || tokens[group]?.[name];
}

function typeStyle(level) {
  if (!level || typeof level !== 'object') return '';
  return [
    level.fontFamily && `font-family:'${level.fontFamily.replace(/[^\w ]/g, '')}',sans-serif`,
    dimension(level.fontSize) && `font-size:${level.fontSize}`,
    /^\d{3}$/.test(level.fontWeight) && `font-weight:${level.fontWeight}`,
    /^\d+(\.\d+)?$/.test(level.lineHeight ?? '') ? `line-height:${level.lineHeight}` : dimension(level.lineHeight) && `line-height:${level.lineHeight}`,
    dimension(level.letterSpacing) && `letter-spacing:${level.letterSpacing}`,
  ].filter(Boolean).join(';');
}

function colours(tokens) {
  const all = tokens.colors, darkBg = hex(all.bg) ?? '#000000', lightBg = hex(all['bg-light']) ?? '#FFFFFF';
  const cards = Object.keys(all).filter(name => !name.endsWith('-light')).map(name => {
    const dark = hex(all[name]), light = hex(all[`${name}-light`]);
    const chip = (value, bg, label) => value ? `<span class="swatch-chip" style="background:${value}"></span><span class="swatch-value">${label ? `${label} ` : ''}${value.toUpperCase()} ${contrast(value, bg) >= 1.05 ? ` · ${contrast(value, bg).toFixed(1)}:1` : ''}</span>` : '';
    return `<figure class="design-swatch" data-token="colors.${escape(name)}"><div class="swatch-colours">${chip(dark, darkBg, light ? 'dark' : '')}${chip(light, lightBg, 'light')}</div><figcaption>${escape(name)}</figcaption></figure>`;
  });
  return `<div class="design-swatches">${cards.join('')}</div><p class="design-note">Contrast is against the page: <code>bg</code> for dark values, <code>bg-light</code> for light ones.</p>`;
}

function typography(tokens) {
  return `<div class="design-type">${Object.entries(tokens.typography).map(([name, level]) =>
    `<figure data-token="typography.${escape(name)}"><div class="type-sample" style="${escape(typeStyle(level))}">Late coding vibes</div><figcaption><code>${escape(name)}</code> ${escape([level.fontFamily, level.fontSize, level.fontWeight].filter(Boolean).join(' · '))}</figcaption></figure>`).join('')}</div>`;
}

function corners(tokens) {
  return `<div class="design-corners">${Object.entries(tokens.rounded).map(([name, value]) =>
    `<figure data-token="rounded.${escape(name)}"><div class="corner-sample" style="border-radius:${escape(dimension(value) ?? '0')}"></div><figcaption><code>${escape(name)}</code> ${escape(value)}</figcaption></figure>`).join('')}</div>`;
}

function spaces(tokens) {
  return `<div class="design-spaces">${Object.entries(tokens.spacing).map(([name, value]) =>
    `<figure data-token="spacing.${escape(name)}"><div class="space-sample" style="width:${escape(dimension(value) ?? '0')}"></div><figcaption><code>${escape(name)}</code> ${escape(value)}</figcaption></figure>`).join('')}</div>`;
}

const playIcon = '<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path d="M8 5v14l11-7z" fill="currentColor"/></svg>';
const labels = { 'button-primary': 'New playlist', 'button-make': 'Make', tile: 'Late coding', band: 'Late coding', chip: 'calm', tab: 'Songs', 'live-switch': 'Live', 'live-switch-on': 'Live', card: 'A card' };

function component(tokens, name, props, light) {
  const value = key => resolve(tokens, props[key], light);
  const size = dimension(value('size'));
  const style = [
    hex(value('backgroundColor')) && `background:${value('backgroundColor')}`,
    hex(value('textColor')) && `color:${value('textColor')}`,
    typeStyle(value('typography')),
    dimension(value('rounded')) && `border-radius:${value('rounded')}`,
    dimension(value('padding')) && `padding:0 ${value('padding')}`,
    (size || dimension(value('height'))) && `height:${size || value('height')}`,
    (size || dimension(value('width'))) && `width:${size || value('width')}`,
    !hex(value('backgroundColor')) && `border:1px solid ${hex(resolve(tokens, '{colors.line2}', light)) ?? 'currentColor'}`,
  ].filter(Boolean).join(';');
  const content = name.startsWith('icon-button') ? playIcon : escape(labels[name] ?? name);
  return `<div class="component-sample" data-token="components.${escape(name)}" style="${escape(style)}">${content}</div>`;
}

function components(tokens) {
  const stage = light => {
    const bg = (light ? hex(tokens.colors['bg-light']) : hex(tokens.colors.bg)) ?? '#000000';
    const ink = resolve(tokens, '{colors.ink}', light);
    return `<div class="design-stage" style="background:${bg};color:${ink}"><p class="stage-label">${light ? 'Light' : 'Dark'}</p>${Object.entries(tokens.components).map(([name, props]) =>
      `<figure>${component(tokens, name, props, light)}<figcaption>${escape(name)}</figcaption></figure>`).join('')}</div>`;
  };
  return `<div class="design-stages">${stage(false)}${stage(true)}</div>`;
}

const drawers = { colors: colours, typography, rounded: corners, spacing: spaces, components };

/** A ```yaml block of DESIGN.md: its picture, then the YAML itself. */
export function renderDesignTokens(text, tokens) {
  const groups = Object.keys(parseTokens(text)).filter(group => drawers[group] && tokens[group]);
  const picture = groups.map(group => drawers[group](tokens)).join('');
  return `${picture ? `<div class="design-preview">${picture}</div>` : ''}<details class="design-yaml"><summary>Tokens</summary><pre><code class="language-yaml">${escape(text)}</code></pre></details>`;
}
