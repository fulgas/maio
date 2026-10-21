import { elements, views, resolveView, boundsFor, boundariesOf, relationshipGeometry, wrapText, nodeShape } from './static/model.mjs';

const escape = text => String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');

export function renderC4Preview(id) {
  if (!Object.hasOwn(views, id)) throw new Error(`Unknown C4 preview view: ${id}`);
  const view = resolveView(id), bounds = boundsFor(view);
  const marker = `preview-${id}-arrow`;
  const parts = [`<defs><marker id="${marker}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="var(--edge)"/></marker></defs>`];
  for (const channel of ['input', 'output']) parts.push(`<defs><marker id="${marker}-${channel}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="var(--flow-${channel})"/></marker></defs>`);
  for (const b of boundariesOf(view)) {
    parts.push(`<rect x="${b.x}" y="${b.y}" width="${b.width}" height="${b.height}" rx="16" class="c4-boundary"/><text x="${b.x + 20}" y="${b.y + 25}" class="boundary-label">${escape(b.label)}</text>`);
  }
  for (const edge of view.edges) {
    const line = relationshipGeometry(view, edge), arrow = edge.channel ? `${marker}-${edge.channel}` : marker;
    parts.push(`<g class="edge${edge.dashed ? ' dashed' : ''}${edge.channel ? ` channel-${edge.channel}` : ''}"><path d="M${line.start.x},${line.start.y} L${line.end.x},${line.end.y}" marker-end="url(#${arrow})"${edge.both ? ` marker-start="url(#${marker})"` : ''}/><text x="${line.label.x}" y="${line.label.y}" text-anchor="middle">${escape(edge.label)}</text></g>`);
  }
  for (const node of view.nodes) {
    const item = elements[node.id];
    const { body, head } = nodeShape(node), person = Boolean(head);
    const textX = person ? body.x + body.width / 2 : 17;
    const title = wrapText(item.name, 18, 2);
    const anchor = person ? ' text-anchor="middle"' : '', extraTitleHeight = (title.length - 1) * 18;
    parts.push(`<g class="c4-node${person ? ' person' : ''}${item.external ? ' external' : ''}${item.optional ? ' optional' : ''}" data-id="${escape(node.id)}" transform="translate(${node.x} ${node.y})"><title>${escape(item.description)}</title><rect x="${body.x}" y="${body.y}" width="${body.width}" height="${body.height}" rx="${body.rx}" class="node-body"/>${person ? `<circle cx="${head.cx}" cy="${head.cy}" r="${head.r}" class="node-body person-head"/>` : ''}<text x="${textX}" y="${person ? body.y + 46 + extraTitleHeight : 27}" class="node-type"${anchor}>${escape(person ? '[Person]' : item.type)}</text>`);
    title.forEach((line, index) => parts.push(`<text x="${textX}" y="${(person ? body.y + 27 : 59) + index * (person ? 18 : 25)}" class="node-title"${anchor}>${escape(line)}</text>`));
    wrapText(item.description, person ? 23 : 28, person && title.length > 1 ? 1 : 2).forEach((line, index) => parts.push(`<text x="${textX}" y="${(person ? body.y + 64 + extraTitleHeight : title.length > 1 ? 114 : 98) + index * (person ? 16 : 17)}" class="node-description"${anchor}>${escape(line)}</text>`));
    parts.push('</g>');
  }
  return `<figure class="c4-preview" data-c4-view="${escape(id)}"><a href="#view/${escape(id)}" aria-label="Explore ${escape(view.title)} interactively"><svg xmlns="http://www.w3.org/2000/svg" viewBox="${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}" role="img" aria-label="${escape(view.title)}">${parts.join('')}</svg><span class="c4-preview-action">Explore ${escape(view.title)}<small>Pan, zoom, and inspect components</small></span></a><figcaption>${escape(view.description)}</figcaption></figure>`;
}
