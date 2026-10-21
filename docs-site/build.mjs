// Builds the architecture website into docs-site/dist: the app (index.html, app.mjs, model.mjs, style.css), every
// document in docs/ rendered to HTML in documents.json, and architecture.html, one file that
// works without a server. A link to a Markdown file that does not exist fails the build.
import { readFile, writeFile, mkdir, cp, readdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked, Renderer } from './vendor/marked.mjs';
import { renderC4Preview } from './c4-preview.mjs';
import { renderDesignTokens, tokensOf } from './design-preview.mjs';

export const siteRoot = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.dirname(siteRoot);
export const outputRoot = path.join(siteRoot, 'dist');
const escape = text => String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const entities = { amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'" };
const plain = text => text.replace(/<[^>]*>/g, '').replace(/&(amp|lt|gt|quot|#39);/g, (_, name) => entities[name]).replace(/&[^;]+;/g, '');
const slugify = text => plain(text).toLowerCase().replace(/[^\p{L}\p{N}_ -]/gu, '').replaceAll(' ', '-');
/** MADR front matter: `key: value` lines between two `---` at the top. */
function frontMatter(markdown) {
  const m = markdown.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return { meta: undefined, body: markdown };
  const meta = Object.fromEntries(m[1].split('\n').map(line => line.match(/^([\w-]+):\s*(.*)$/)).filter(Boolean).map(([, k, v]) => [k, v.replace(/^"|"$/g, '')]));
  return { meta, body: markdown.slice(m[0].length) };
}
/** A ```tree block: one entry per line, two spaces of indent per level, a note after two or more spaces. */
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
const markdownIn = async dir => (await readdir(path.join(repositoryRoot, 'docs', dir))).filter(name => /^\d.*\.md$/.test(name)).sort().map(name => `${dir}/${name}`);

/** Where a document sits in the reader's navigation. */
function groupOf(source) {
  if (source.startsWith('adr/')) return 'decisions';
  if (source.startsWith('architecture/')) return 'architecture';
  return 'project';
}

// Files at the repository root, read from docs/: the README is the project overview; DESIGN.md the design.
const rootIds = { '../README.md': 'overview', '../DESIGN.md': 'design' };

export async function build() {
  const docsRoot = path.join(repositoryRoot, 'docs');
  // The repository's README is the project overview: one file for GitHub and the site.
  const files = ['../README.md', 'product.md', '../DESIGN.md', 'tech.md', 'project-rules.md', ...(await markdownIn('architecture')), ...(await markdownIn('adr')), 'adr/template.md'];
  const knownFiles = new Set(files);
  const documents = [], broken = [];
  for (const source of files) {
    const markdown = await readFile(path.join(docsRoot, source), 'utf8');
    const id = rootIds[source] ?? source.replace(/\.md$/, '');
    const headings = [], slugs = new Map();
    const renderer = new Renderer();
    renderer.heading = function ({ tokens, depth }) {
      const text = this.parser.parseInline(tokens), base = slugify(text);
      const count = slugs.get(base) ?? 0; slugs.set(base, count + 1);
      const slug = count ? `${base}-${count}` : base;
      headings.push({ text: plain(text), depth, slug });
      return `<h${depth} id="${escape(slug)}">${text}</h${depth}>\n`;
    };
    renderer.link = function ({ href, title, tokens }) {
      let target = href;
      if (href && !/^[a-z][a-z\d+.-]*:/i.test(href) && !href.startsWith('//') && !href.startsWith('#view/') && !href.startsWith('#doc/')) {
        const [filename, section] = href.split('#');
        // The README sits above docs/: its docs/… links are the same files.
        const resolved = (filename ? path.posix.normalize(path.posix.join(path.posix.dirname(source), filename)) : source).replace(/^\.\.\/docs\//, '');
        if (knownFiles.has(resolved)) target = `#doc/${rootIds[resolved] ?? resolved.replace(/\.md$/, '')}${section ? `?section=${encodeURIComponent(section)}` : ''}`;
        else if (filename.endsWith('.md')) broken.push(`${source}: ${href}`);
      }
      const external = /^https?:/i.test(target ?? '');
      return `<a href="${escape(target ?? '')}"${title ? ` title="${escape(title)}"` : ''}${external ? ' target="_blank" rel="noopener noreferrer"' : ''}>${this.parser.parseInline(tokens)}</a>`;
    };
    // ```c4 <view>``` draws that view of the model, linked to the explorer; ```tree``` a folder tree; ```mermaid``` is drawn in the browser.
    const designTokens = source === '../DESIGN.md' ? tokensOf(markdown) : undefined;
    renderer.code = ({ text, lang }) => {
      if (lang === 'yaml' && designTokens) return renderDesignTokens(text, designTokens);
      if (lang === 'c4') return renderC4Preview(text.trim());
      if (lang === 'tree') return renderTree(text);
      if (lang === 'mermaid') return `<pre class="mermaid">${escape(text)}</pre>`;
      return `<pre><code${lang ? ` class="language-${escape(lang)}"` : ''}>${escape(text)}</code></pre>`;
    };
    const { meta, body } = frontMatter(markdown);
    let html = new Marked({ renderer, gfm: true }).parse(body);
    // A decision record's front matter becomes one line under its title: number, status, date, who decided.
    if (meta && source.startsWith('adr/')) {
      const number = source.match(/^adr\/(\d{4})/)?.[1];
      const status = meta.status ? meta.status[0].toUpperCase() + meta.status.slice(1) : '';
      const line = `<p class="adr-meta">${number ? `<span class="adr-number">ADR ${number}</span>` : ''}<span class="adr-status adr-status-${escape((meta.status ?? '').split(' ')[0])}">${escape(status)}</span>${meta.date ? `<span>${escape(meta.date)}</span>` : ''}${meta['decision-makers'] ? `<span>decided by ${escape(meta['decision-makers'])}</span>` : ''}</p>\n`;
      html = html.replace(/(<\/h1>\n)/, `$1${line}`);
    }
    const navTitle = { overview: 'Project overview' }[id];
    documents.push({ id, source: source.replace(/^\.\.\//, ''), markdown, meta, title: headings.find(h => h.depth === 1)?.text ?? id, navTitle, group: groupOf(source), headings, html });
  }
  if (broken.length) throw new Error(`Links to missing documents:\n  ${broken.join('\n  ')}`);

  // The directory is generated output owned by this build, never repository source.
  await mkdir(outputRoot, { recursive: true });
  for (const entry of await readdir(outputRoot)) await rm(path.join(outputRoot, entry), { recursive: true, force: true });
  await cp(path.join(siteRoot, 'src'), outputRoot, { recursive: true });
  await cp(path.join(repositoryRoot, 'public/fonts'), path.join(outputRoot, 'fonts'), { recursive: true });
  await mkdir(path.join(outputRoot, 'vendor'), { recursive: true });
  await cp(path.join(siteRoot, 'vendor/LICENSE.marked'), path.join(outputRoot, 'vendor/LICENSE.marked'));
  await cp(path.join(siteRoot, 'vendor/mermaid.min.js'), path.join(outputRoot, 'vendor/mermaid.min.js'));
  await writeFile(path.join(outputRoot, 'documents.json'), JSON.stringify(documents));
  const template = await readFile(path.join(siteRoot, 'src/index.html'), 'utf8');
  const css = await readFile(path.join(siteRoot, 'src/style.css'), 'utf8');
  const model = (await readFile(path.join(siteRoot, 'src/model.mjs'), 'utf8')).replace(/^export /gm, '');
  const application = (await readFile(path.join(siteRoot, 'src/app.mjs'), 'utf8')).replace(/^import .* from '\.\/model\.mjs';\n/m, '');
  const mermaid = await readFile(path.join(siteRoot, 'vendor/mermaid.min.js'), 'utf8');
  const safeScript = code => code.replace(/<\/script/gi, '<\\/script');
  const standalone = template
    .replace('<link rel="stylesheet" href="./style.css">', () => `<style>${css}</style>`)
    .replace('<script type="module" src="./app.mjs"></script>', () => `<script id="documentation-data" type="application/json">${JSON.stringify(documents).replaceAll('<', '\\u003c')}</script>\n<script>${safeScript(mermaid)}</script>\n<script type="module">${safeScript(model + '\n' + application)}</script>`);
  await writeFile(path.join(outputRoot, 'architecture.html'), standalone);
  console.log(`Built the architecture website: ${documents.length} documents → ${path.relative(process.cwd(), outputRoot) || outputRoot}`);
  return { documents, outputRoot };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await build();
