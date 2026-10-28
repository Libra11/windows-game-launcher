import { libraryRoots, mediaItemTitle } from './media-query.js';

const normalized = (value) => String(value || '').normalize('NFKC').trim().toLocaleLowerCase();
const collator = new Intl.Collator('zh-CN', { numeric: true });
// MessageChannel 让出 UI 线程，避免 Windows 上大量 0ms 定时器累积成秒级等待。
const pause = (() => {
  if (typeof window === 'undefined' && typeof setImmediate === 'function')
    return () => new Promise((resolve) => setImmediate(resolve));
  if (typeof MessageChannel !== 'undefined') {
    const channel = new MessageChannel();
    const queue = [];
    channel.port1.onmessage = () => queue.shift()?.();
    return () => new Promise((resolve) => { queue.push(resolve); channel.port2.postMessage(0); });
  }
  return () => new Promise((resolve) => setTimeout(resolve, 0));
})();
const CHUNK = 2000;

export function mediaItemContext(items = []) {
  return {
    byId: new Map(items.map((item) => [item.id, item])),
    order: new Map(items.map((item, index) => [item.id, index])),
  };
}

function emptyIndex(data) {
  const items = data.items || [];
  return { items: [], people: [], entries: new Map(), context: mediaItemContext(items) };
}
function addItem(index, item) {
  const title = mediaItemTitle(item, [], index.context.byId);
  const entry = { item, title, normalTitle: normalized(title), search: normalized(`${title} ${item.title} ${item.year || ''}`) };
  index.items.push(entry);
  index.entries.set(item, entry);
}
function addPerson(index, person) {
  const count = new Set((person.credits || []).filter((credit) => index.context.byId.has(credit.itemId)).map((credit) => credit.itemId)).size;
  if (!count) return;
  index.people.push({ person, count, name: normalized(person.name), names: [person.name, ...(person.aliases || [])].map(normalized) });
}
export function createMediaSearchIndex(data) {
  const index = emptyIndex(data);
  for (const item of data.items || []) addItem(index, item);
  for (const person of data.people || []) addPerson(index, person);
  return index;
}
export async function createMediaSearchIndexAsync(data, cancelled = () => false) {
  const index = emptyIndex(data);
  for (const values of [[data.items || [], addItem], [data.people || [], addPerson]]) {
    const [rows, add] = values;
    for (let i = 0; i < rows.length; i++) {
      if (i % CHUNK === 0) { await pause(); if (cancelled()) return null; }
      add(index, rows[i]);
    }
  }
  return cancelled() ? null : index;
}
function rank(value, term) { return value === term ? 0 : value.startsWith(term) ? 1 : 2; }
function finish(index, term, items, people) {
  const works = libraryRoots(items.map((entry) => entry.item)).map((item) => index.entries.get(item));
  works.sort((a, b) => rank(a.normalTitle, term) - rank(b.normalTitle, term) || collator.compare(a.title, b.title));
  people.sort((a, b) => rank(a.name, term) - rank(b.name, term) || b.count - a.count);
  return { items: works.map((entry) => entry.item), people: people.map((entry) => entry.person) };
}
export function searchMediaIndex(index, query) {
  const term = normalized(query);
  if (!term) return { items: [], people: [] };
  return finish(index, term, index.items.filter((entry) => entry.search.includes(term)),
    index.people.filter((entry) => entry.names.some((name) => name.includes(term))));
}
export async function searchMediaIndexAsync(index, query, cancelled = () => false) {
  const term = normalized(query);
  if (!term) return { items: [], people: [] };
  const items = [], people = [];
  for (const [rows, result, matches] of [
    [index.items, items, (entry) => entry.search.includes(term)],
    [index.people, people, (entry) => entry.names.some((name) => name.includes(term))],
  ]) {
    for (let i = 0; i < rows.length; i++) {
      if (i % CHUNK === 0) { await pause(); if (cancelled()) return null; }
      if (matches(rows[i])) result.push(rows[i]);
    }
  }
  if (cancelled()) return null;
  return finish(index, term, items, people);
}

// 延迟并分段执行，旧查询或离开页面时停止；同一快照的索引可复用。
export function createMediaSearchRunner() {
  let generation = 0, cachedData = null, cachedIndex = null, disposed = false;
  return {
    async search(data, query, inactive = () => false) {
      const current = ++generation;
      const cancelled = () => disposed || current !== generation || inactive();
      await pause();
      if (cancelled()) return null;
      let index = cachedData === data ? cachedIndex : null;
      if (!index) {
        index = await createMediaSearchIndexAsync(data, cancelled);
        if (!index || cancelled()) return null;
        cachedData = data;
        cachedIndex = index;
      }
      return searchMediaIndexAsync(index, query, cancelled);
    },
    cancel() { generation++; },
    clear() { generation++; cachedData = null; cachedIndex = null; },
    dispose() { disposed = true; generation++; cachedData = null; cachedIndex = null; },
  };
}
