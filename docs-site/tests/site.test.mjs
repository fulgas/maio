import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, access } from 'node:fs/promises';
import path from 'node:path';
import { build, repositoryRoot } from './site.mjs';
import { elements, views } from '../static/model.mjs';

const result = await build();
const docs = new Map(result.documents.map(d => [d.id, d]));

test('every arc42 section and decision record is published, grouped for the reader', () => {
  assert.deepEqual(result.documents.filter(d => d.group === 'project').map(d => d.id), ['overview', 'project-rules', 'contributing', 'security', 'code-of-conduct']);
  assert.equal(docs.get('overview').source, 'README.md', 'the README is the project overview');
  const arc42 = result.documents.filter(d => d.group === 'architecture');
  assert.equal(arc42.length, 12);
  assert.deepEqual(arc42.map(d => d.title.match(/^\d+/)[0]), ['1', '2', '3', '4', '5', '6', '7', '8', '9', '10', '11', '12']);
  assert.equal(result.documents.filter(d => /^adr\/\d/.test(d.id)).length, 19);
  for (const id of ['overview', 'project-rules']) assert.ok(docs.get(id), `missing ${id}`);
  assert.ok(!docs.has('adr/README'), 'section 9 is the decision index');
  assert.ok(!docs.has('index') && !docs.has('ARCHITECTURE'), 'no separate index pages');
  assert.match(docs.get('overview').html, /href="\/project-rules\/"/, 'the overview links the rules');
  assert.match(docs.get('overview').html, /href="\/architecture\/01-introduction-and-goals\/"/, 'the overview links the architecture');
});

test('links between documents, links to views, and the model\'s references all resolve', () => {
  const check = (id, section, source) => {
    const target = docs.get(id);
    assert.ok(target, `${source}: missing document ${id}`);
    if (section) assert.ok(target.headings.some(h => h.slug === decodeURIComponent(section)), `${source}: missing section ${id}#${section}`);
  };
  for (const doc of docs.values()) {
    for (const m of doc.html.matchAll(/href="\/((?:architecture|adr|tutorials|how-to)\/[^"#\/]+|(?!explore\/)[a-z-]+)\/(?:#([^"]+))?"/g)) check(m[1], m[2], doc.id);
    for (const m of doc.html.matchAll(/href="\/explore\/#view\/([^"]+)"/g)) assert.ok(views[m[1]], `${doc.id}: missing C4 view ${m[1]}`);
    assert.ok(!/href="\/#view\//.test(doc.html), `${doc.id}: a view link that misses the explorer`);
    assert.ok(!/href="(?!https?:)[^"#]*\.md(#[^"]*)?"/.test(doc.html), `${doc.id}: a link still points at a .md file`);
  }
  for (const item of Object.values(elements).filter(e => e.doc)) {
    const [id, query] = item.doc.split('?');
    check(id, new URLSearchParams(query).get('section'), item.name);
  }
});

test('the arc42 sections draw each C4 level from the model, linked to the explorer', () => {
  const previews = id => [...docs.get(id).html.matchAll(/<figure class="c4-preview" data-c4-view="([^"]+)">/g)].map(m => m[1]);
  assert.deepEqual(previews('architecture/03-system-scope-and-context'), ['context', 'containers']);
  assert.deepEqual(previews('architecture/05-building-block-view'), ['components', 'live', 'webapp']);
  assert.match(docs.get('architecture/05-building-block-view').html, /href="\/explore\/#view\/live"/);
});

test('the site, its fonts, the explorer and Mermaid are written; the repository keeps the Markdown', async () => {
  for (const asset of ['index.html', 'app.mjs', 'model.mjs', 'style.css', 'reader.js', 'documents.json', 'fonts/unbounded.woff2', 'fonts/albert-sans.woff2', 'vendor/mermaid.min.js']) await access(path.join(result.outputRoot, asset));
  assert.equal(JSON.parse(await readFile(path.join(result.outputRoot, 'documents.json'), 'utf8')).length, result.documents.length, 'the explorer knows every document');
  for (const doc of result.documents) assert.ok(!doc.page.includes('Markdown source'), `${doc.id}: no download link`);
});

test('every page link in the site itself (header, buttons, the explorer) reaches a real document or view', async () => {
  const shell = [result.index, docs.get('overview').page, await readFile(path.join(result.outputRoot, 'app.mjs'), 'utf8')];
  for (const text of shell) {
    for (const m of text.matchAll(/#doc\/([A-Za-z0-9_\/.-]+)/g)) assert.ok(docs.get(m[1]), `#doc/${m[1]} is no document`);
    for (const m of text.matchAll(/href="\/((?:architecture|adr)\/[^"#\/]+)\/"/g)) assert.ok(docs.get(m[1]), `/${m[1]}/ is no document`);
    for (const m of text.matchAll(/#view\/([a-z]+)/g)) assert.ok(views[m[1]], `#view/${m[1]} is no view`);
  }
});

test('the C4 levels read Context, Containers, Components', async () => {
  const html = result.index;
  assert.match(html, /01 <span>Context<\/span>[\s\S]*02 <span>Containers<\/span>[\s\S]*03 <span>Components<\/span>/);
  const crumbs = html.match(/<nav id="breadcrumbs"[\s\S]*?<\/nav>/)[0];
  assert.equal([...crumbs.matchAll(/data-view=/g)].length, 3, 'only the three C4 levels; deeper views are reached by drilling in');
  assert.equal(views.components.title, 'Components');
});

test('the runtime view draws its scenarios as sequence diagrams, and the site carries Mermaid to render them', async () => {
  const runtime = docs.get('architecture/06-runtime-view');
  const diagrams = [...runtime.html.matchAll(/<code data-lang="mermaid">([\s\S]*?)<\/code>/g)].map(m => m[1]);
  assert.ok(diagrams.length >= 5, `only ${diagrams.length} diagrams`);
  for (const d of diagrams) assert.match(d, /^sequenceDiagram/);
  assert.ok(!/[│─┼]/.test(runtime.markdown), 'no hand-drawn box diagrams left');
  await access(path.join(result.outputRoot, 'vendor/mermaid.min.js'));
  assert.match(runtime.page, /reader\.js/, 'the reader draws them');
});

test('no section is drawn with box characters: shapes, nodes and lifecycles are Mermaid diagrams', () => {
  const kinds = id => [...docs.get(id).html.matchAll(/<code data-lang="mermaid">([\w-]+)/g)].map(m => m[1]);
  for (const doc of result.documents.filter(d => d.group === 'architecture')) assert.ok(!/[│┌└├─┼]/.test(doc.markdown), `${doc.id} still has a box drawing`);
  assert.ok(kinds('architecture/03-system-scope-and-context').includes('stateDiagram-v2'), 'engine health');
  for (const doc of result.documents) assert.ok(!kinds(doc.id).includes('flowchart'), `${doc.id}: structure is drawn by the C4 model, not Mermaid`);
  assert.match(docs.get('architecture/07-deployment-view').html, /<figure class="c4-preview" data-c4-view="deployment">/);
  assert.ok(kinds('architecture/08-crosscutting-concepts').includes('stateDiagram-v2'), 'a job\'s lifecycle');
});

test('section 9 is the decision index: it links every ADR with its status', () => {
  const index = docs.get('architecture/09-architecture-decisions');
  const adrs = result.documents.filter(d => /^adr\/\d/.test(d.id));
  for (const adr of adrs) assert.match(index.html, new RegExp(`href="/${adr.id}/"`), `${adr.id} is not in section 9`);
  assert.match(index.markdown, /\| Status \|/);
});

test('decision records say what was decided and why in words: no code, paths, commands or HTML', () => {
  for (const doc of result.documents.filter(d => /^adr\/\d/.test(d.id))) {
    const body = doc.markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
    assert.doesNotMatch(body, /`/, `${doc.id}: a code span`);
    assert.doesNotMatch(body, /<\/?[a-z][\w-]*[\s>]/i, `${doc.id}: an HTML tag`);
  }
});

test('decision records follow MADR 4.0.0, and the site shows their status instead of raw front matter', () => {
  const index = docs.get('architecture/09-architecture-decisions');
  for (const doc of result.documents.filter(d => /^adr\/\d/.test(d.id))) {
    assert.match(doc.markdown, /^---\nstatus: [a-z]/, `${doc.id}: MADR front matter first`);
    assert.match(doc.meta?.date ?? '', /^\d{4}-\d{2}-\d{2}$/, `${doc.id}: a date`);
    assert.equal(doc.meta['decision-makers'], 'fulgas', `${doc.id}: who decided`);
    const headings = doc.headings.map(h => h.text);
    for (const h of ['Context and Problem Statement', 'Considered Options', 'Decision Outcome', 'Consequences', 'Confirmation']) assert.ok(headings.includes(h), `${doc.id}: missing ${h}`);
    assert.ok(headings.indexOf('Considered Options') < headings.indexOf('Decision Outcome'), `${doc.id}: options before the outcome`);
    assert.match(doc.markdown, /Chosen option: "/, `${doc.id}: names the chosen option`);
    assert.match(doc.html, /<p class="adr-meta">/, `${doc.id}: the status line`);
    assert.ok(!/<hr>/.test(doc.html.slice(0, 200)) && !/status: /.test(doc.html), `${doc.id}: front matter shown raw`);
    const row = index.markdown.split('\n').find(line => line.startsWith('| [') && line.includes(`(../adr/${doc.source.slice(4)})`));
    assert.match(row, new RegExp(`\\| ${doc.meta.status[0].toUpperCase()}${doc.meta.status.slice(1)} \\|$`), `${doc.id}: section 9 shows another status`);
  }
  assert.ok(docs.get('adr/template'), 'a template to start from');
  assert.match(index.html, /href="\/adr\/template\/"/);
});


test('a ```tree block is drawn as a folder tree: one row per entry, nested, with its note', () => {
  const html = docs.get('architecture/05-building-block-view').html;
  assert.match(html, /<ul class="file-tree"[^>]*>/);
  assert.match(html, /<li class="tree-dir"><span class="tree-name">src\/<\/span>[\s\S]*?<ul>[\s\S]*?<span class="tree-name">domain\/<\/span><span class="tree-note">types and rules/);
  assert.match(html, /<li class="tree-file"><span class="tree-name">main\.rs<\/span>/);
  assert.ok(!/<pre><code class="language-text">src\//.test(html), '5.1 is still plain text');
});

test('no Mermaid diagram has a semicolon: Mermaid reads it as the end of a statement', () => {
  for (const doc of result.documents) for (const [, body] of doc.markdown.matchAll(/```mermaid\n([\s\S]*?)```/g))
    assert.equal(body.split('\n').find(line => line.includes(';')), undefined, `${doc.id} has a semicolon in a Mermaid diagram`);
});

test('a Mermaid diagram opens in a zoom dialog: the browser\'s own <dialog>, with zoom out, zoom in and fit', async () => {
  const html = docs.get('architecture/06-runtime-view').page;
  const dialog = html.match(/<dialog id="diagram-zoom"[\s\S]*?<\/dialog>/)?.[0];
  assert.ok(dialog, 'a zoom dialog');
  for (const label of ['Zoom out', 'Zoom in', 'Fit', 'Close']) assert.match(dialog, new RegExp(`aria-label="${label}"|>${label}<`), label);
  const app = await readFile(path.join(result.outputRoot, 'reader.js'), 'utf8');
  assert.match(app, /setAttribute\('role', 'button'\)[\s\S]*?Zoom the diagram/, 'the diagram itself opens the dialog');
  assert.ok(!app.includes("'Zoom')"), 'no separate Zoom button');
});

test('product, design and tech lead into arc42, and arc42 points back to them', () => {
  const links = id => [...docs.get(id).html.matchAll(/href="\/([^"#]+?)\/(?:#[^"]*)?"/g)].map(m => m[1]);
  assert.deepEqual(['product', 'design', 'tech'].filter(id => !links('architecture/01-introduction-and-goals').includes(id)), [], '1.5 names all three');
  assert.ok(links('architecture/08-crosscutting-concepts').includes('design'), '8.5 points to the design');
  assert.deepEqual(['product', 'design', 'tech'].filter(id => !links('overview').includes(id)), [], 'the README names all three');
  for (const id of ['product', 'design', 'tech']) assert.ok(links(id).some(target => target.startsWith('architecture/')), `${id} leads into arc42`);
});

test('DESIGN.md follows the DESIGN.md format: its sections in order, and a token for every colour the web app uses', async () => {
  const design = docs.get('design');
  assert.equal(design.source, 'DESIGN.md', 'at the repository root, where tools look for it');
  const order = ['Overview', 'Colors', 'Typography', 'Layout', 'Elevation & Depth', 'Shapes', 'Components', "Do's and Don'ts"];
  assert.deepEqual(design.headings.filter(h => h.depth === 2).map(h => h.text), order);
  assert.match(design.markdown, /^---\nversion: alpha\nname: /);
  const colors = design.markdown.match(/```yaml\ncolors:\n([\s\S]*?)```/)?.[1] ?? '';
  const token = name => colors.match(new RegExp(`^  ${name}: "(#[0-9A-Fa-f]{6})"$`, 'm'))?.[1]?.toUpperCase();
  assert.ok(token('primary'), 'the format asks for a primary colour');
  const css = await readFile(path.join(repositoryRoot, 'style/main.css'), 'utf8');
  const [dark, light] = [/:root \{([\s\S]*?)\}/, /:root\[data-theme="light"\] \{([\s\S]*?)\}/].map(r => css.match(r)[1]);
  const wrong = [];
  for (const [block, suffix] of [[dark, ''], [light, '-light']])
    for (const [, name, hex] of block.matchAll(/--([a-z0-9-]+):\s*(#[0-9A-Fa-f]{6})/g))
      if (token(name + suffix) !== hex.toUpperCase()) wrong.push(`${name}${suffix} should be ${hex}`);
  assert.deepEqual(wrong, []);
  assert.ok(!/<p class="adr-meta">/.test(design.html), 'no decision-record status line');
});

test('tech names every just recipe', async () => {
  const justfile = await readFile(path.join(repositoryRoot, 'justfile'), 'utf8');
  const recipes = [...justfile.matchAll(/^([a-z][\w-]*):/gm)].map(m => m[1]);
  assert.deepEqual(recipes.filter(r => !docs.get('tech').markdown.includes(`\`just ${r}\``)), []);
});


test('DESIGN.md is drawn from its tokens: every colour, type level, corner, space and component, components in both themes', () => {
  const design = docs.get('design');
  const tokens = {};
  for (const [, block] of design.markdown.matchAll(/```yaml\n([\s\S]*?)```/g)) {
    let group;
    for (const line of block.split('\n')) {
      const top = line.match(/^([a-z]+):\s*$/), entry = line.match(/^  ([\w-]+):/);
      if (top) { group = top[1]; tokens[group] = []; } else if (entry) tokens[group].push(entry[1]);
    }
  }
  const drawn = id => [...design.html.matchAll(new RegExp(`data-token="${id.replace('.', '\\.')}"`, 'g'))].length;
  const colours = tokens.colors.filter(name => !name.endsWith('-light'));
  for (const name of colours) assert.ok(drawn(`colors.${name}`) >= 1, `colors.${name}`);
  for (const group of ['typography', 'rounded', 'spacing']) for (const name of tokens[group]) assert.ok(drawn(`${group}.${name}`) >= 1, `${group}.${name}`);
  for (const name of tokens.components) assert.equal(drawn(`components.${name}`), 2, `components.${name} in dark and light`);
  assert.match(design.html, /class="language-yaml"/, 'the tokens are still shown');
  assert.match(design.html, /data-token="colors\.muted"[\s\S]*?\d+\.\d:1/, 'a swatch shows its contrast');
});

test('section 5 says what hexagonal means, and 5.2 lists every port with its adapter and its fake', async () => {
  const doc = docs.get('architecture/05-building-block-view');
  assert.match(doc.markdown, /## What hexagonal means/);
  for (const word of ['driving', 'driven', 'inward']) assert.match(doc.markdown, new RegExp(word, 'i'), word);
  const ports = await readFile(path.join(repositoryRoot, 'src/usecases/ports.rs'), 'utf8');
  const fakes = await readFile(path.join(repositoryRoot, 'src/usecases/testing.rs'), 'utf8');
  const table = doc.markdown.match(/## 5\.2[\s\S]*?(?=\n## )/)[0];
  assert.match(table, /\| Port \| Does \| Adapter \| Fake in tests \|/);
  for (const [, trait] of ports.matchAll(/^pub trait (\w+)/gm)) {
    const row = table.split('\n').find(line => line.includes(`\`${trait}\``));
    assert.ok(row, `${trait} is not in 5.2`);
    const fake = fakes.match(new RegExp(`^impl (?:crate::usecases::ports::)?${trait} for (\\w+)`, 'm'))?.[1];
    assert.ok(fake && row.includes(`\`${fake}\``), `${trait}: 5.2 should name its fake ${fake}`);
  }
  const glossary = docs.get('architecture/12-glossary').markdown;
  for (const term of ['Hexagon', 'Port', 'Adapter', 'Driving adapter', 'Driven adapter']) assert.match(glossary, new RegExp(`\\| \\*\\*${term}\\*\\* \\|`), term);
});

test('the README is a proper README: what it is, requirements, quick start, ports, configuration, docs, and the community files', () => {
  const readme = docs.get('overview');
  const sections = readme.headings.filter(h => h.depth === 2).map(h => h.text);
  for (const s of ['Features', 'Requirements', 'Quick start', 'Engines and API keys', 'Ports', 'Configuration', 'Development', 'Documentation', 'Contributing', 'Credits', 'License']) assert.ok(sections.includes(s), `README has no ${s} section`);
  assert.match(readme.markdown, /^# Maio\n\n\*\*M\*\*usic \+ \*\*AI\*\* \+ \*\*O\*\*utput\./, 'the name says what it is');
  for (const id of ['contributing', 'security', 'code-of-conduct']) assert.match(readme.html, new RegExp(`href="/${id}/"`), `README links ${id}`);
  assert.match(docs.get('code-of-conduct').markdown, /Contributor Covenant/);
  assert.match(docs.get('security').markdown, /## Reporting a vulnerability/);
  for (const step of ['uv sync', 'ACE_STEP_API_KEY', 'api_key_env', 'ANTHROPIC_API_KEY', '[[text_engine]]', '--force-recreate']) assert.ok(readme.markdown.includes(step), `README does not explain ${step}`);
});

test('Maio is MIT licensed, and the fonts it ships carry their own licence', async () => {
  assert.match(await readFile(path.join(repositoryRoot, 'LICENSE-MIT.md'), 'utf8'), /^MIT License\n\nCopyright \(c\) \d{4} Nelson Silva/);
  assert.match(await readFile(path.join(repositoryRoot, 'LICENSE-APACHE.md'), 'utf8'), /Apache License\s+Version 2\.0[\s\S]*Copyright \d{4} Nelson Silva/);
  assert.match(await readFile(path.join(repositoryRoot, 'Cargo.toml'), 'utf8'), /^license = "MIT OR Apache-2\.0"$/m);
  assert.ok(docs.get('overview').html.includes('href="https://github.com/fulgas/maio/blob/main/LICENSE-MIT.md"'), 'the licence opens on GitHub');
  assert.match(docs.get('overview').markdown, /## License\n\nEither \[Apache 2\.0\]\(LICENSE-APACHE\.md\) or \[MIT\]\(LICENSE-MIT\.md\), at your option\./);
  for (const font of ['unbounded', 'albert-sans']) assert.match(await readFile(path.join(repositoryRoot, `public/fonts/${font}-OFL.txt`), 'utf8'), /SIL OPEN FONT LICENSE/);
});

test('releases follow Semantic Versioning, worked out from Conventional Commits', () => {
  const rules = docs.get('project-rules').markdown;
  assert.match(rules, /## Releases/);
  for (const words of ['Semantic Versioning 2.0.0', 'BREAKING CHANGE', '`feat`', '`fix`', 'v0.1.0', 'Cargo.toml']) assert.ok(rules.includes(words), `the release rules do not mention ${words}`);
});

test('the commit-msg hook takes Conventional Commits and refuses anything else', async () => {
  const { spawnSync } = await import('node:child_process');
  const { writeFileSync, mkdtempSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(path.join(tmpdir(), 'hook-'));
  const verdict = message => {
    const file = path.join(dir, 'MSG');
    writeFileSync(file, message);
    return spawnSync('sh', [path.join(repositoryRoot, '.githooks/commit-msg'), file]).status === 0;
  };
  for (const ok of ['feat(player): Pause turns to Play\n', 'fix: a typo\n\nbody\n', 'refactor(live)!: drop the old route\n\nBREAKING CHANGE: gone\n', 'docs(docs-site): no downloads\n', '# a comment\nchore: tidy\n', 'fixup! feat: x\n'])
    assert.ok(verdict(ok), `refused: ${ok}`);
  for (const bad of ['Added a thing\n', 'feature: x\n', 'feat(Player): x\n', 'feat:x\n', 'fix: \n'])
    assert.ok(!verdict(bad), `accepted: ${bad}`);
});


test('every page carries the favicon inline, whole', async () => {
  const html = docs.get('overview').page;
  const icon = html.match(/<link rel="icon" href="data:image\/svg\+xml,([^"]*)">/);
  assert.ok(icon, 'an icon link whose address has no raw quote in it');
  assert.equal(decodeURIComponent(icon[1]).trim(), (await readFile(path.join(repositoryRoot, 'public/favicon.svg'), 'utf8')).trim());
});

test('DESIGN.md shows the favicon itself, inlined', () => {
  const design = docs.get('design');
  assert.match(design.markdown, /!\[The favicon\]\(public\/favicon\.svg\)/);
  assert.match(design.html, /<img src="data:image\/svg\+xml,[^"]+" alt="The favicon"/);
});

test('ACE-Step is not tied to a Mac: the docs send people to its own setup instructions', () => {
  for (const doc of result.documents) assert.ok(!/\bMetal\b/.test(doc.markdown), `${doc.id} says ACE-Step needs Metal`);
  assert.match(docs.get('overview').markdown, /CUDA/);
  assert.match(docs.get('overview').markdown, /\[ACE-Step's own instructions\]\(https:\/\/github\.com\/ace-step\/ACE-Step-1\.5/);
});


test('the header shows the version Cargo.toml carries, and claims nothing about a running app', async () => {
  const version = (await readFile(path.join(repositoryRoot, 'Cargo.toml'), 'utf8')).match(/^version = "([^"]+)"/m)[1];
  for (const [file, html] of [['index.html', result.index], ['a document', docs.get('overview').page]]) {
    assert.match(html, new RegExp(`<span class="baseline">v${version.replaceAll('.', '\\.')}</span>`), file);
    assert.ok(!/running/.test(html.match(/<div class="header-actions">[\s\S]*?<\/div>/)[0]), `${file}: no "running"`);
  }
});

test('the docs follow Diátaxis: a tutorial, how-to guides, explanation and reference', () => {
  const group = g => result.documents.filter(d => d.group === g).map(d => d.id);
  assert.ok(group('tutorials').length >= 1, 'a tutorial');
  assert.ok(group('how-to').length >= 5, 'how-to guides');
  assert.ok(group('how-to').includes('how-to/06-make-a-themed-playlist-with-the-api'), 'making playlists through the API is a how-to');
  assert.deepEqual(group('reference'), ['reference', 'tech', 'design']);
  assert.deepEqual(group('explanation'), ['product']);
  for (const id of [...group('tutorials'), ...group('how-to')]) {
    assert.match(docs.get(id).html, /<ol>/, `${id}: steps in order`);
    assert.match(docs.get(id).html, /href="\/architecture\//, `${id}: leads into the architecture`);
  }
  for (const link of ['architecture/03-system-scope-and-context/#35-the-http-api', 'architecture/07-deployment-view/#73-config', 'architecture/12-glossary/', 'design/', 'tech/'])
    assert.ok(docs.get('reference').html.includes(`href="/${link}"`), `the reference points to ${link}`);
});

test('the compass is the home page and leads to the four corners of Diátaxis, each listing its pages', async () => {
  const compass = result.compass;
  const corners = [...compass.matchAll(/<div class="quadrant quadrant-([a-z-]+)">/g)].map(m => m[1]);
  assert.deepEqual(corners, ['tutorials', 'how-to', 'explanation', 'reference'], 'Tutorials and How-to on top, Explanation and Reference below');
  for (const axis of ['Doing', 'Understanding', 'Learning', 'Working']) assert.match(compass, new RegExp(`class="axis[^"]*">${axis}<`), axis);
  for (const id of ['tutorials/01-your-first-song-on-air', 'how-to/02-add-a-text-engine', 'product', 'architecture/01-introduction-and-goals', 'architecture/09-architecture-decisions', 'reference', 'design'])
    assert.ok(compass.includes(`href="/${id}/"`), `the compass links ${id}`);
  for (const id of ['overview', 'project-rules', 'contributing', 'security', 'code-of-conduct']) assert.ok(compass.includes(`href="/${id}/"`), `the project row links ${id}`);
  assert.match(compass, /data-tab="docs" class="active"/, 'the Docs tab is the compass');
  assert.match(compass, /href="\/explore\/#view\/context">Explore the C4 model</, 'the explorer is part of the explanation');
  assert.deepEqual([...compass.matchAll(/data-tab="([a-z]+)"/g)].map(m => m[1]), ['docs', 'view', 'decisions'], 'Docs first');
  assert.match(compass, /<small>DOCS<\/small>/);
  assert.match(compass, /#view\/[\s\S]*\/explore\//, 'old links to a view reach the explorer');
});

test('inside a page the sidebar shows only its corner, with a switch to the other three and the compass', () => {
  const sidebar = id => docs.get(id).page.match(/<nav id="docs-nav"[\s\S]*?<\/details><\/aside>/)[0];
  const howto = sidebar('how-to/01-run-ace-step-on-another-machine');
  assert.match(howto, /<nav class="quadrant-switch"/);
  for (const corner of ['Tutorials', 'How-to guides', 'Explanation', 'Reference']) assert.match(howto, new RegExp(`>${corner}</a>`), corner);
  assert.match(howto, /href="\/" class="compass-link"/, 'back to the compass');
  assert.match(howto, /href="\/how-to\/05-back-up-the-library\/"/, 'its own pages');
  assert.ok(!howto.includes('href="/architecture/05-building-block-view/"'), 'no other corner\'s pages');
  assert.match(sidebar('architecture/05-building-block-view'), /href="\/product\/"[\s\S]*href="\/architecture\/12-glossary\/"/, 'explanation: product and arc42');
  assert.match(docs.get('how-to/01-run-ace-step-on-another-machine').page, /<span id="doc-label">HOW-TO GUIDE<\/span>/);
  assert.match(docs.get('product').page, /<span id="doc-label">EXPLANATION<\/span>/);
});

test('every page with the theme button can change the theme', async () => {
  const pages = { 'the documentation home': 'index.html', 'the explorer': 'explore/index.html', 'a document': 'architecture/05-building-block-view/index.html' };
  for (const [name, file] of Object.entries(pages)) {
    const html = await readFile(path.join(result.outputRoot, file), 'utf8');
    assert.ok(html.includes('id="theme"'), `${name} has the button`);
    const handled = /reader\.js|app\.mjs/.test(html) || /getElementById\('theme'\)\.addEventListener\('click'/.test(html);
    assert.ok(handled, `${name}: nothing handles the theme button`);
  }
});

test('every page links to the repository on GitHub, by an icon with a name', async () => {
  for (const file of ['index.html', 'explore/index.html', 'architecture/05-building-block-view/index.html']) {
    const html = await readFile(path.join(result.outputRoot, file), 'utf8');
    assert.match(html, /<a class="icon-button" href="https:\/\/github\.com\/fulgas\/maio" aria-label="Maio on GitHub"[^>]*><svg[^>]*aria-hidden="true"/, file);
  }
});
