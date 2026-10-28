// 资料库仍保留剧／季／集的数据层级，观看界面只保留一个作品上下文。
export function seriesContext(item, items = []) {
  const byId = new Map(items.map((value) => [value.id, value]));
  const seen = new Set();
  let current = item;
  while (current?.parentId && !seen.has(current.id)) {
    seen.add(current.id);
    const parent = byId.get(current.parentId);
    if (!parent) break;
    current = parent;
  }
  return ['series', 'season'].includes(current?.kind) ? current : item;
}

export function episodeLabel(item) {
  const season = item.seasonNumber;
  return `${season != null ? `S${String(season).padStart(2, '0')}` : ''}E${String(item.episodeNumber ?? '?').padStart(2, '0')}`;
}

export function episodeAvailability(item, today = new Date()) {
  if (item.playable) return '可播放';
  if (item.resourceCount) return '资源离线';
  const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
  return /^\d{4}-\d{2}-\d{2}$/.test(item.airDate || '') && item.airDate > date
    ? `未播出 · ${item.airDate}`
    : '暂无资源';
}

export function detailModel(detail, items = [], selection = {}) {
  const item = detail.item;
  const catalog = new Map(
    [...items, ...(detail.parents || []), ...(detail.children || []), item].map((value) => [
      value.id,
      value,
    ])
  );
  const belongs = (value) => {
    const seen = new Set();
    while (value?.parentId && !seen.has(value.id)) {
      seen.add(value.id);
      if (value.parentId === item.id) return true;
      value = catalog.get(value.parentId);
    }
    return false;
  };
  const descendants = [...catalog.values()].filter(belongs);
  const episodes = descendants
    .filter((value) => value.kind === 'episode')
    .sort(
      (a, b) =>
        (a.seasonNumber ?? 0) - (b.seasonNumber ?? 0) ||
        (a.episodeNumber ?? 0) - (b.episodeNumber ?? 0)
    );
  const seasons = descendants
    .filter((value) => value.kind === 'season')
    .sort((a, b) => (a.seasonNumber ?? 0) - (b.seasonNumber ?? 0));
  if (item.kind === 'season') seasons.unshift(item);
  const resources = detail.resources || [];
  const available = (resource) => resource.available && !resource.missing;
  const forEpisode = (episode) =>
    resources.filter((resource) => resource.itemIds?.includes(episode.id));
  const requestedSeason = seasons.find((value) => value.id === selection.seasonId);
  const preferredEpisodes = requestedSeason
    ? episodes.filter((value) => value.parentId === requestedSeason.id)
    : episodes;
  const resumeOwner = requestedSeason || item;
  const preferred =
    preferredEpisodes.find((episode) =>
      forEpisode(episode).some(
        (resource) => available(resource) && resource.id === resumeOwner.resumeResourceId
      )
    ) ||
    [...preferredEpisodes]
      .filter(
        (episode) => episode.playable && Number(episode.positionSeconds) > 0 && !episode.watched
      )
      .sort((a, b) => (b.lastPlayed || '').localeCompare(a.lastPlayed || ''))[0] ||
    preferredEpisodes.find((episode) => episode.playable && !episode.watched) ||
    preferredEpisodes.find((episode) => episode.playable) ||
    preferredEpisodes[0];
  const selected = episodes.find((episode) => episode.id === selection.episodeId);
  const season =
    requestedSeason ||
    seasons.find((value) => value.id === (selected || preferred)?.parentId) ||
    seasons[0];
  const seasonEpisodes = season
    ? episodes.filter((episode) => episode.parentId === season.id)
    : episodes;
  const visibleEpisodes = selection.onlyWithResources
    ? seasonEpisodes.filter((value) => value.playable || value.resourceCount > 0)
    : seasonEpisodes;
  const episode =
    visibleEpisodes.find((value) => value.id === selected?.id) ||
    visibleEpisodes.find((value) => value.id === preferred?.id) ||
    visibleEpisodes.find((value) => value.playable && !value.watched) ||
    visibleEpisodes[0];
  const target = episode || season || item;
  const versions = episode
    ? forEpisode(episode)
    : season
      ? resources.filter((value) => value.itemIds?.includes(season.id))
      : resources;
  const resource =
    versions.find((value) => available(value) && value.id === selection.resourceId) ||
    versions.find((value) => available(value) && value.id === target.resumeResourceId) ||
    versions.find((value) => available(value) && Number(value.positionSeconds) > 0) ||
    versions.find(available);
  return {
    item,
    seasons,
    episodes,
    visibleEpisodes,
    seasonEpisodes,
    season,
    recordItem: season || item,
    episode,
    target,
    versions,
    resource,
  };
}

export function metadataSource(item) {
  const sources = item.externalIds || [];
  if (sources.some((source) => source.provider === 'tmdb')) return 'TMDB';
  if (sources.some((source) => source.provider === 'douban')) return item.recognized ? '豆瓣' : '豆瓣导入';
  return item.recognized ? '已识别资料' : '本地视频';
}

export function metadataSearchTitle(item, items = []) {
  const byId = new Map(items.map(value => [value.id, value]));
  const seen = new Set();
  let current = item;
  while (current?.parentId && !seen.has(current.id)) {
    seen.add(current.id);
    const parent = byId.get(current.parentId);
    if (!parent) break;
    current = parent;
  }
  return current?.title || item.title || '';
}
