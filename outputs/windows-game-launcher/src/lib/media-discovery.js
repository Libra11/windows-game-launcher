import { queryMedia } from './media-query.js';
import { createMediaSearchIndex, mediaItemContext, searchMediaIndex } from './media-search-index.js';

const normalized = (value) =>
  String(value || '')
    .normalize('NFKC')
    .trim()
    .toLocaleLowerCase();
export function personDepartment(person) {
  const departments = person.departments || [person.department];
  return [departments.includes('cast') && '演员', departments.includes('crew') && '主创']
    .filter(Boolean)
    .join(' / ');
}
export function personWorks(person, items = [], options = {}, context = mediaItemContext(items)) {
  const credits = new Map((person?.credits || []).map((credit) => [credit.itemId, credit]));
  // 仅在明确属于该层级的作品上筛选，整剧演员关系不扩散到季和集。
  const candidates = [];
  for (const id of credits.keys()) {
    const item = context.byId.get(id);
    if (!item || !['movie', 'season'].includes(item.kind)) continue;
    // 完整片库中有父级的电影不属于展示根，不能因只传候选而改变这一规则。
    if (item.kind === 'movie' && item.parentId && context.byId.has(item.parentId)) continue;
    candidates.push(item);
  }
  candidates.sort((a, b) => context.order.get(a.id) - context.order.get(b.id));
  return queryMedia(candidates, {
    category: options.kind === 'movie' ? 'movies' : options.kind === 'series' ? 'series' : 'all',
    sort: options.sort || 'az',
    watchedFilter: options.watchedFilter || 'all',
    onlyPlayable: !!options.onlyPlayable,
    itemIndex: context.byId,
  })
    .filter((item) => ['movie', 'season'].includes(item.kind) && credits.has(item.id))
    .map((item) => ({ ...item, personCredit: credits.get(item.id) }));
}
export function searchMedia(data, query) {
  if (!normalized(query)) return { items: [], people: [] };
  return searchMediaIndex(createMediaSearchIndex(data), query);
}

export function peopleIndexMessage(index = {}) {
  const pending = Math.max(0, (index.total || 0) - (index.completed || 0));
  if (!pending && !index.failed) return '';
  return [
    pending && `还有 ${pending} 部作品的演职员资料待补齐`,
    index.failed && `${index.failed} 部暂未获取，稍后自动重试`,
  ]
    .filter(Boolean)
    .join('；');
}
