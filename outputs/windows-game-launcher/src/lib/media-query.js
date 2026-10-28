import { episodeLabel } from './media-detail-model.js';
const titleCollator = new Intl.Collator('zh-CN', { numeric: true });

export const mediaCategories = [
  ['all', '全部影视', 'library'],
  ['recent', '最近观看', 'history'],
  ['added', '最近添加', 'library-add'],
  ['favorites', '我的收藏', 'star'],
  ['movies', '电影', 'film'],
  ['series', '电视剧', 'tv'],
  ['watched', '已观看', 'circle-check'],
  ['unwatched', '未观看', 'circle-play'],
  ['other', '未识别', 'file-question'],
  ['files', '本地影视', 'folder-video'],
  ['history', '观看记录', 'chart'],
];
export const mediaKindIcon = (kind) =>
  ({ movie: 'film', series: 'tv', season: 'tv', episode: 'circle-play' })[kind] || 'file-question';
export const mediaKindName = (kind) =>
  ({
    movie: '电影',
    series: '电视剧',
    season: '电视剧',
    episode: '电视剧',
    other: '类型未知',
  })[kind] || '类型未知';

export function recognitionLabel(item) {
  if (item.recognized) return '';
  return ({ unmatched: '未匹配', failed: '识别失败' })[item.recognitionStatus] || '待识别';
}

export function mediaGenres(item, items = []) {
  const seen = new Set();
  let current = item;
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    const genres = Array.isArray(current.genres)
      ? [
          ...new Set(
            current.genres
              .filter((value) => typeof value === 'string')
              .map((value) => value.trim())
              .filter(Boolean)
          ),
        ]
      : [];
    if (genres.length) return genres;
    current = items.find((value) => value.id === current.parentId);
  }
  return [];
}
export function mediaItemTitle(item, items = [], byId = null) {
  const find = (id) => byId ? byId.get(id) : items.find((value) => value.id === id);
  if (item.kind === 'episode') {
    const parent = find(item.parentId);
    const series =
      parent?.kind === 'season' ? find(parent.parentId) : parent;
    const title = series?.title || parent?.title;
    return title
      ? `${title} · ${episodeLabel({ ...item, seasonNumber: item.seasonNumber ?? parent?.seasonNumber })} · ${item.title || ''}`
      : item.title || '';
  }
  const parent = item.kind === 'season' && find(item.parentId);
  if (parent && parent.seasonCount === 1 && item.seasonNumber === 1) return parent.title;
  return parent ? `${parent.title} · ${item.title}` : item.title || '';
}
export function secondsLabel(value = 0) {
  const seconds = Math.max(0, Math.floor(Number(value) || 0));
  return seconds >= 3600
    ? `${Math.floor(seconds / 3600)}:${String(Math.floor(seconds / 60) % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
    : `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
export function durationLabel(value = 0) {
  const seconds = Math.max(0, Number(value) || 0);
  if (!seconds) return '0 分钟';
  if (seconds < 60) return '不足 1 分钟';
  const minutes = Math.floor(seconds / 60);
  return minutes >= 60
    ? `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`
    : `${minutes} 分钟`;
}
export function sizeLabel(value = 0) {
  const bytes = Number(value) || 0;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return bytes < 1024 ** 3
    ? `${(bytes / 1024 ** 2).toFixed(1)} MB`
    : `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}
export function progressPercent(item) {
  const duration = Number(item.durationSeconds),
    position = Number(item.positionSeconds || 0);
  return Number.isFinite(duration) && duration > 0 && Number.isFinite(position)
    ? Math.min(100, Math.max(0, (position / duration) * 100))
    : 0;
}
export function safeImageUrl(value) {
  if (typeof value !== 'string') return '';
  try {
    const url = new URL(value);
    return ['https:', 'http:', 'asset:'].includes(url.protocol) ? value : '';
  } catch {
    return '';
  }
}
export function libraryRoots(items = []) {
  const ids = new Set(items.map((item) => item.id));
  const parents = new Set(items.map((item) => item.parentId));
  // 季是片库的展示单位；整剧只组织各季，单集留在季内或显式记录入口。
  return items.filter((item) => {
    if (item.kind === 'season') return true;
    if (item.kind === 'series') return !item.recognized && !parents.has(item.id);
    return !item.parentId || !ids.has(item.parentId);
  });
}
export function inMediaCategory(item, category) {
  if (category === 'recent') return !!item.lastPlayed;
  if (category === 'favorites') return !!item.favorite;
  if (category === 'movies') return item.kind === 'movie';
  if (category === 'series') return ['series', 'season', 'episode'].includes(item.kind);
  if (category === 'other') return !item.recognized;
  if (category === 'watched') return !!item.watched;
  if (category === 'unwatched') return !item.watched;
  return true;
}
export function queryMedia(items = [], options = {}, collections = []) {
  const {
    category = 'all',
    search = '',
    sort = 'az',
    onlyRecognized: requestedRecognized = false,
    onlyPlayable = false,
    collectionId = '',
    tagIds = [],
    watchedFilter: requestedWatched = 'all',
    itemIndex = null,
  } = options;
  // 分类已经固定的维度只有一个有效值，不能被隐藏的筛选状态抵消。
  const onlyRecognized = category !== 'other' && requestedRecognized;
  const watchedFilter = ['watched', 'unwatched'].includes(category) ? 'all' : requestedWatched;
  const collection = collections.find((item) => item.id === collectionId);
  const selected = new Set(collection?.itemIds || []);
  let byId = itemIndex;
  const titles = new Map();
  const title = (item) => {
    if (!titles.has(item.id)) {
      byId ||= new Map(items.map((value) => [value.id, value]));
      titles.set(item.id, mediaItemTitle(item, items, byId));
    }
    return titles.get(item.id);
  };
  const candidates = collectionId
    ? items.filter((item) => selected.has(item.id))
    : category === 'watched'
      ? libraryRoots(items.filter((item) => item.watched))
      : ['favorites', 'other'].includes(category)
        ? items
        : libraryRoots(items);
  const term = search.trim().toLocaleLowerCase();
  const result = candidates.filter(
    (item) =>
      inMediaCategory(item, category) &&
      (!onlyRecognized || !!item.recognized) &&
      (!onlyPlayable || !!item.playable) &&
      (watchedFilter === 'all' || !!item.watched === (watchedFilter === 'watched')) &&
      (!term ||
        `${title(item)} ${item.title} ${item.year || ''}`
          .toLocaleLowerCase()
          .includes(term)) &&
      (!tagIds.length || tagIds.every((id) => (item.tags || []).some((tag) => tag.id === id)))
  );
  const effectiveSort = category === 'recent' ? 'recent' : category === 'added' ? 'added' : sort;
  if (sort === 'none') return result;
  return result.sort((a, b) => {
    if (effectiveSort === 'recent') {
      const order = (b.lastPlayed || '').localeCompare(a.lastPlayed || '');
      if (order) return order;
    }
    if (effectiveSort === 'added') {
      const order = (b.createdAt || '').localeCompare(a.createdAt || '');
      if (order) return order;
    }
    if (effectiveSort === 'rating') {
      const order = (b.rating || 0) - (a.rating || 0);
      if (order) return order;
    }
    const order = titleCollator.compare(title(a), title(b));
    if (a.kind === 'season' && b.kind === 'season' && a.parentId && a.parentId === b.parentId) {
      const seasons = (a.seasonNumber ?? 0) - (b.seasonNumber ?? 0);
      return effectiveSort === 'za' ? -seasons : seasons;
    }
    return effectiveSort === 'za' ? -order : order;
  });
}
export function resourceTechnical(resource) {
  const tech = resource.technical || {};
  const videos = tech.video || [];
  const audios = tech.audio || [];
  const subtitles = tech.subtitles || [];
  const video = videos[0] || {};
  const { width, height, codec } = video;
  const resolution = width && height ? `${width} × ${height}` : '';
  const format = tech.container || '';
  return {
    tech,
    videos,
    audios,
    subtitles,
    video,
    resolution,
    codec,
    format,
    summary: [
      height >= 2000 ? '4K' : height >= 1000 ? '1080p' : height >= 700 ? '720p' : resolution,
      codec?.toUpperCase(),
      video.hdr,
      format,
    ]
      .filter(Boolean)
      .join(' · '),
  };
}
export function resourceOwnerId(item, resource, items = []) {
  const owners = resource.itemIds || [];
  if (owners.includes(item.id)) return item.id;
  if (!['series', 'season'].includes(item.kind)) return '';
  const byId = new Map(items.map((value) => [value.id, value]));
  return (
    owners.find((id) => {
      const seen = new Set();
      let parent = byId.get(id)?.parentId;
      while (parent && !seen.has(parent)) {
        if (parent === item.id) return true;
        seen.add(parent);
        parent = byId.get(parent)?.parentId;
      }
      return false;
    }) || ''
  );
}
