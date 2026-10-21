// Builds the site as `just docs` does (sync.mjs, then Zola) and gives the tests each document: its Markdown, front
// matter, headings, and the HTML Zola wrote for it, with links made site-relative (/architecture/…/#section).
import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { sync, siteRoot, repositoryRoot } from '../sync.mjs';

export { repositoryRoot };
export const outputRoot = path.join(siteRoot, 'dist');
const zola = path.join(siteRoot, '.zola/zola');
const site = html => html.replaceAll('href="http://localhost:8000/', 'href="/');

export async function build() {
  const index = await sync();
  execFileSync(zola, ['--root', siteRoot, 'build'], { stdio: 'pipe' });
  const documents = [];
  for (const doc of index) {
    const file = /\//.test(doc.source) || ['product.md', 'tech.md', 'project-rules.md'].includes(doc.source) ? path.join(repositoryRoot, 'docs', doc.source) : path.join(repositoryRoot, doc.source);
    const page = site(await readFile(path.join(outputRoot, doc.id, 'index.html'), 'utf8'));
    const html = page.match(/<article id="document" class="prose">([\s\S]*?)<\/article>/)[1];
    documents.push({ ...doc, markdown: await readFile(file, 'utf8'), html, page });
  }
  return { documents, outputRoot, index: site(await readFile(path.join(outputRoot, 'index.html'), 'utf8')) };
}
