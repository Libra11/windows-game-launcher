export function el(tag, className = '', text = '') {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = text;
  return node;
}
const paths = {
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7"/>',
  star: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9L12 3Z"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  screen: '<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8m-4-4v4M5 8V6h3m8 0h3v2M5 12v2h3m8 0h3v-2"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  moon: '<path d="M20.5 13.3A9 9 0 0 1 10.7 3.5a9 9 0 1 0 9.8 9.8Z"/>',
  library: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  game: '<path d="M7 7h10a4 4 0 0 1 4 3l1 7a2 2 0 0 1-3.4 1.7L15 16H9l-3.6 2.7A2 2 0 0 1 2 17l1-7a4 4 0 0 1 4-3Z"/><path d="M7 10v4m-2-2h4m7-1h.01m3 3h.01"/>',
  steam: '<circle cx="16" cy="8" r="5"/><circle cx="16" cy="8" r="2"/><circle cx="7" cy="17" r="3"/><path d="m9 15 4-4M1 14l5 3m3 2 7-6"/>',
  epic: '<path d="M5 3h14v15l-7 3-7-3V3Z"/><path d="M10 7H8v6h2m-2-3h2m3 3V7h2a1.5 1.5 0 0 1 0 3h-2"/>',
  trophy: '<path d="M8 3h8v7a4 4 0 0 1-8 0V3Zm0 2H4v3a4 4 0 0 0 4 4m8-7h4v3a4 4 0 0 1-4 4m-4 2v6m-4 1h8"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  arrow: '<path d="M5 12h14m-6-6 6 6-6 6"/>',
  back: '<path d="M19 12H5m6-6-6 6 6 6"/>',
  play: '<path d="m8 4 12 8-12 8V4Z"/>',
  refresh: '<path d="M20 7a9 9 0 0 0-15-2L2 8m0-6v6h6m-4 9a9 9 0 0 0 15 2l3-3m0 6v-6h-6"/>',
  settings: '<path d="m10 3-1 3-3 1-3 3 2 2-1 4 3 2 3-1 3 3 3-1 1-3 3-2-1-3 1-3-3-2-3 1-2-3Z"/><circle cx="12" cy="12" r="3"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
  folder: '<path d="M3 7V5h6l2 2h10v13H3V7Z"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10h.01"/>',
  chevron: '<path d="m9 5 7 7-7 7"/>',
};
export function icon(name, className = '') {
  const node = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  node.setAttribute('viewBox', '0 0 24 24');
  node.setAttribute('class', `icon ${className}`);
  node.setAttribute('fill', 'none');
  node.setAttribute('stroke', 'currentColor');
  node.setAttribute('stroke-width', '1.7');
  node.setAttribute('stroke-linecap', 'round');
  node.setAttribute('stroke-linejoin', 'round');
  node.setAttribute('aria-hidden', 'true');
  node.innerHTML = paths[name] || paths.game;
  return node;
}
export function button(text, className, onClick, glyph) {
  const node = el('button', className);
  node.type = 'button';
  if (glyph) node.append(icon(glyph));
  if (text) { node.append(el('span', '', text)); node.setAttribute('aria-label', text); }
  node.addEventListener('click', async () => {
    if (node.disabled) return;
    node.disabled = true;
    try { await onClick(); } catch { /* 错误由统一提示处理。 */ }
    finally { node.disabled = false; }
  });
  return node;
}
export function metadata(game) {
  try { return JSON.parse(game.metadataJson || '{}'); } catch { return {}; }
}
const failed = new Set();
export function artwork(host, game, wide = false) {
  host.append(el('span', 'art-letter', game.title.slice(0, 1).toUpperCase()));
  const info = metadata(game);
  const base = /^\d+$/.test(game.appid) ? `https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}` : '';
  const sources = [...new Set([base && `${base}/${wide ? 'library_hero' : 'library_600x900'}.jpg`, info.cover, base && `${base}/header.jpg`, info.icon].filter(Boolean))];
  const next = () => {
    const url = sources.shift();
    if (!url) return;
    if (failed.has(url)) return next();
    const image = el('img');
    image.alt = ''; image.loading = wide ? 'eager' : 'lazy';
    image.onload = () => host.classList.add('art-loaded');
    image.onerror = () => { failed.add(url); image.remove(); next(); };
    image.src = url; host.append(image);
  };
  next();
}
export const sourceName = game => ({ steam:'Steam', epic:'Epic' }[game.source] || '本地游戏');
export const sourceIcon = game => ({ steam:'steam', epic:'epic' }[game.source] || 'folder');
