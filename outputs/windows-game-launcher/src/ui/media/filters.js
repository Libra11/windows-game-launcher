import { button } from '../../lib/dom.js';
import '../activity.css';

export function statusFilters(
  controls,
  query,
  onChange,
  { category = 'all', recognized = true } = {}
) {
  const choices = [];
  if (!['watched', 'unwatched'].includes(category))
    choices.push(['watchedFilter', '已观看', 'circle-check']);
  if (recognized && category !== 'other') choices.push(['onlyRecognized', '已识别', 'scan-check']);
  choices.push(['onlyPlayable', '可观看', 'circle-play']);
  for (const [key, label, glyph] of choices) {
    const active = key === 'watchedFilter' ? query[key] === 'watched' : !!query[key];
    const node = button(
      label,
      `installed-filter ${active ? 'active' : ''}`,
      () => {
        query[key] = key === 'watchedFilter' ? (active ? 'all' : 'watched') : !active;
        onChange();
      },
      glyph
    );
    node.setAttribute('aria-pressed', String(active));
    node.dataset.focusKey = `media-filter-${key}`;
    controls.append(node);
  }
}
