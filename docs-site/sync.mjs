// Copies docs/ into Zola's content/ as Zola needs it, without changing the docs: front matter (title, order, the
// MADR keys under extra), heading ids that match our slugs, relative links turned into Zola's checked @/ links,
// and what our build draws at build time drawn here too (C4 previews, folder trees, DESIGN.md's tokens, the ADR
// status line, inlined SVG pictures). Mermaid stays a code block; the browser draws it, as on our site.
import { readFile, writeFile, mkdir, readdir, rm, cp } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { renderC4Preview } from './c4-preview.mjs';
import { renderDesignTokens, tokensOf } from './design-preview.mjs';

export const siteRoot = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.dirname(siteRoot);
const docsRoot = path.join(repositoryRoot, 'docs');
const out = path.join(siteRoot, 'content');

const escape = t => String(t).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const plain = t => t.replace(/`([^`]*)`/g, '$1').replace(/\*\*|__|\*|_/g, '').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/<[^>]*>/g, '');
const slugify = t => plain(t).toLowerCase().replace(/[^\p{L}\p{N}_ -]/gu, '').replaceAll(' ', '-');
const toml = s => `"${String(s).replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;

const rootIds = { '../README.md': 'overview', '../DESIGN.md': 'design', '../CONTRIBUTING.md': 'contributing', '../SECURITY.md': 'security', '../ACCESSIBILITY.md': 'accessibility', '../CODE_OF_CONDUCT.md': 'code-of-conduct' };
const markdownIn = async dir => (await readdir(path.join(docsRoot, dir))).filter(n => /^\d.*\.md$/.test(n)).sort().map(n => `${dir}/${n}`);
const idOf = source => rootIds[source] ?? source.replace(/\.md$/, '');
const known = new Set();
const navTitles = { overview: 'Project overview', 'code-of-conduct': 'Code of conduct' };

function renderTree(text) {
  const root = { children: [] }, stack = [{ depth: -1, node: root }];
  for (const line of text.split('\n').filter(l => l.trim())) {
    const depth = line.match(/^ */)[0].length / 2;
    const [, name, note = ''] = line.trim().match(/^(\S+)(?:\s{2,}(.*))?$/);
    const node = { name, note, children: [] };
    while (stack.at(-1).depth >= depth) stack.pop();
    stack.at(-1).node.children.push(node);
    stack.push({ depth, node });
  }
  const list = nodes => `<ul>${nodes.map(n => `<li class="tree-${n.name.endsWith('/') || n.children.length ? 'dir' : 'file'}"><span class="tree-name">${escape(n.name)}</span>${n.note ? `<span class="tree-note">${escape(n.note)}</span>` : ''}${n.children.length ? list(n.children) : ''}</li>`).join('')}</ul>`;
  return list(root.children).replace('<ul>', '<ul class="file-tree">');
}
// Raw HTML stays one block for the Markdown parser: no blank lines inside it.
const block = html => `\n${html.replace(/\n\s*\n/g, '\n')}\n`;

function frontMatter(markdown) {
  const m = markdown.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return { meta: undefined, body: markdown };
  const meta = Object.fromEntries(m[1].split('\n').map(l => l.match(/^([\w-]+):\s*(.*)$/)).filter(Boolean).map(([, k, v]) => [k, v.replace(/^"|"$/g, '')]));
  return { meta, body: markdown.slice(m[0].length) };
}

function link(href, source) {
  if (href.startsWith('#view/')) return `/explore/${href}`;
  if (/^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith('//') || href.startsWith('#')) return href;
  const [file, section] = href.split('#');
  const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(source), file)).replace(/^\.\.\/docs\//, '');
  // A repository file that is not a page here (the licences) opens on GitHub.
  if (!known.has(resolved)) return /\.md$/i.test(file) ? `https://github.com/fulgas/maio/blob/main/${resolved.replace(/^\.\.\//, '')}${section ? `#${section}` : ''}` : href;
  return `@/${idOf(resolved)}.md${section ? `#${section}` : ''}`;
}

/** Writes content/ (and the explorer's document index) for Zola, and copies the fonts into static/. */
export async function sync() {
  // In the order the compass and the sidebars show them: the project, then the four kinds of Diátaxis.
  const files = ['../README.md', 'project-rules.md', '../CONTRIBUTING.md', '../SECURITY.md', '../ACCESSIBILITY.md', '../CODE_OF_CONDUCT.md', ...(await markdownIn('tutorials')), ...(await markdownIn('how-to')), 'reference.md', 'tech.md', '../DESIGN.md', 'product.md', ...(await markdownIn('architecture')), ...(await markdownIn('adr')), 'adr/template.md'];
  known.clear(); for (const f of files) known.add(f);
  const version = (await readFile(path.join(repositoryRoot, 'Cargo.toml'), 'utf8')).match(/^version = "([^"]+)"/m)[1];
  await cp(path.join(repositoryRoot, 'public/fonts'), path.join(siteRoot, 'static/fonts'), { recursive: true });
  // The clips of the web app, recorded by SCREENS=1 just e2e, as the documents reference them.
  if (existsSync(path.join(docsRoot, 'images'))) await cp(path.join(docsRoot, 'images'), path.join(siteRoot, 'static/images'), { recursive: true });
  await rm(out, { recursive: true, force: true });
  for (const [dir, title, sort] of [['', 'maio', 'weight'], ['explore', 'Explore', 'weight'], ['tutorials', 'Tutorials', 'weight'], ['how-to', 'How-to guides', 'weight'], ['architecture', 'Architecture', 'weight'], ['adr', 'Decision records', 'weight']]) {
    await mkdir(path.join(out, dir), { recursive: true });
    // The root section carries the version Cargo.toml says, for the header.
    await writeFile(path.join(out, dir, '_index.md'), `+++\ntitle = ${toml(title)}\nsort_by = ${toml(sort)}\n${dir === 'explore' ? 'template = "index.html"\n' : ''}${dir ? '' : `template = "docs.html"\n[extra]\nversion = ${toml(version)}\n`}+++\n`);
  }
  
  let weight = 0;
  const index = [];
  for (const source of files) {
    const raw = await readFile(path.join(docsRoot, source), 'utf8');
    const id = idOf(source);
    const { meta, body } = frontMatter(raw);
    const designTokens = source === '../DESIGN.md' ? tokensOf(raw) : undefined;
    const lines = body.split('\n'), result = [], slugs = new Map(), headings = [];
    let title = id;
    for (let i = 0; i < lines.length; i++) {
      const fence = lines[i].match(/^```(\S*)\s*$/);
      if (fence) {
        const lang = fence[1], start = i;
        while (++i < lines.length && !/^```\s*$/.test(lines[i]));
        const text = lines.slice(start + 1, i).join('\n');
        // The preview's link opens the explorer.
      if (lang === 'c4') result.push(block(renderC4Preview(text.trim()).replaceAll('href="#view/', 'href="/explore/#view/')));
        else if (lang === 'tree') result.push(block(renderTree(text)));
        // A figure in docs-site/static/figures, framed in the page; the text around it says the same in words.
        else if (lang === 'figure') { const [name, ...title] = text.trim().split(/\s+/); result.push(block(`<iframe class="figure" src="${'../'.repeat(idOf(source).split('/').length)}figures/${escape(name)}.html" title="${escape(title.join(' '))}" loading="lazy"></iframe>`)); }
        else if (lang === 'yaml' && designTokens) result.push(block(renderDesignTokens(text, designTokens)));
        else result.push(...lines.slice(start, i + 1));
        continue;
      }
      let line = lines[i];
      const heading = line.match(/^(#{1,6}) (.+)$/);
      if (heading) {
        const base = slugify(heading[2]), count = slugs.get(base) ?? 0;
        slugs.set(base, count + 1);
        if (heading[1] === '#' && title === id) title = plain(heading[2]);
        const slug = count ? `${base}-${count}` : base;
      headings.push({ text: plain(heading[2]), depth: heading[1].length, slug });
      line = `${line} {#${slug}}`;
      }
      // The README's banner: an <h1> holding a <picture> of two local SVGs, dark (the source) and light (the img). The text
      // before the colon in its alt text is the page's title. On the site the two are inlined side by side and its
      // theme shows one: the site has its own theme switch, which a <picture> cannot follow.
      const banner = line.match(/^<h1><picture><source [^>]*srcset="([^"]+\.svg)"><img src="([^"]+\.svg)" alt="([^"]+)"/);
      if (banner) {
        const [, dark, light, alt] = banner, name = alt.split(':')[0].trim();
        const svg = href => `data:image/svg+xml,${encodeURIComponent(readFileSync(path.join(docsRoot, path.dirname(source), href), 'utf8').trim())}`;
        if (title === id) title = name;
        headings.push({ text: name, depth: 1, slug: slugify(name) });
        line = `<h1 class="banner"><img class="banner-dark" src="${svg(dark)}" alt="${escape(alt)}"><img class="banner-light" src="${svg(light)}" alt="${escape(alt)}"></h1>`;
      }
      // Pictures: a local SVG is inlined, as our build does.
      line = line.replace(/!\[([^\]]*)\]\(([^)\s]+\.svg)\)/g, (all, alt, href) => {
        const file = path.join(docsRoot, path.dirname(source), href);
        return existsSync(file) ? `<img src="data:image/svg+xml,${encodeURIComponent(readFileSync(file, 'utf8').trim())}" alt="${escape(alt)}">` : all;
      });
      // A clip of the web app: an animated PNG with a still of its last frame beside it (name-still.png), shown instead
      // to anyone who asked for less motion. GitHub shows the animation as it is.
      line = line.replace(/!\[([^\]]*)\]\(([^)\s]+)\.png\)/g, (all, alt, href) => {
        // As a path inside docs/, so the README (at the root) finds the same pictures.
        const local = path.relative(docsRoot, path.resolve(docsRoot, path.dirname(source), href));
        if (local.startsWith('..') || !existsSync(path.join(docsRoot, `${local}-still.png`))) return all;
        const up = '../'.repeat(idOf(source).split('/').length), png = readFileSync(path.join(docsRoot, `${local}.png`));
        return `<picture><source srcset="${up}${local}-still.png" media="(prefers-reduced-motion: reduce)"><img class="clip" src="${up}${local}.png" width="${png.readUInt32BE(16)}" height="${png.readUInt32BE(20)}" alt="${escape(alt)}" loading="lazy"></picture>`;
      });
      // Bare addresses become links, as GitHub and our renderer make them; Zola links only <…> ones. Code spans are left alone.
      line = line.split('`').map((part, i) => i % 2 ? part : part.replace(/(^|\s)(https?:\/\/[^\s<>()]*[^\s<>().,;:!?])/g, '$1<$2>')).join('`');
      line = line.replace(/(?<!!)\[([^\]]*)\]\(([^)\s]+)\)/g, (all, text, href) => `[${text}](${link(href, source)})`);
      result.push(line);
      // A decision record's status line sits under its title, as our build draws it.
      if (heading && heading[1] === '#' && meta && source.startsWith('adr/')) {
        const number = source.match(/^adr\/(\d{4})/)?.[1];
        const status = meta.status ? meta.status[0].toUpperCase() + meta.status.slice(1) : '';
        result.push('', `<p class="adr-meta">${number ? `<span class="adr-number">ADR ${number}</span>` : ''}<span class="adr-status adr-status-${escape((meta.status ?? '').split(' ')[0])}">${escape(status)}</span>${meta.date ? `<span>${escape(meta.date)}</span>` : ''}${meta['decision-makers'] ? `<span>decided by ${escape(meta['decision-makers'])}</span>` : ''}</p>`, '');
      }
    }
    // Diátaxis: tutorials, how-to guides and the reference sit beside the architecture (explanation) and the project.
  const group = source.startsWith('adr/') ? 'decisions' : source.startsWith('architecture/') ? 'architecture' : source.startsWith('tutorials/') ? 'tutorials' : source.startsWith('how-to/') ? 'how-to' : ['reference.md', 'tech.md', '../DESIGN.md'].includes(source) ? 'reference' : source === 'product.md' ? 'explanation' : 'project';
    const number = source.match(/^adr\/(\d{4})/)?.[1];
    const extra = { group, nav_title: navTitles[id] ?? title, ...(number ? { number, short: title } : {}), ...(meta ?? {}) };
    const fm = `+++\ntitle = ${toml(title)}\nweight = ${++weight}\n[extra]\n${Object.entries(extra).map(([k, v]) => `${k.replaceAll('-', '_')} = ${toml(v)}`).join('\n')}\n+++\n`;
    const dest = path.join(out, `${id}.md`);
    await mkdir(path.dirname(dest), { recursive: true });
    await writeFile(dest, fm + result.join('\n'));
    index.push({ id, source: source.replace(/^\.\.\//, ''), title, navTitle: navTitles[id], group, meta, headings });
  }
  // The explorer app counts and links documents from this index; the pages themselves are Zola's.
  await writeFile(path.join(siteRoot, 'static/documents.json'), JSON.stringify(index));
  return index;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) console.log(`synced ${(await sync()).length} documents into content/`);
