import presentation from './media-preview-presentation.json' with { type: 'json' };
import metadata from './media-preview-metadata.json' with { type: 'json' };
import { mediaItemTitle } from './media-query.js';
import { mediaPathMatches } from './media-path.js';
export function createMediaPreview() {
  const now = new Date().toISOString();
  const before = (days) => new Date(Date.now() - days * 86400000).toISOString();
  let sequence = 0;
  const base = {
    parentId: '',
    year: '',
    overview: '',
    posterUrl: '',
    backdropUrl: '',
    genres: [],
    recognized: false,
    independent: false,
    createdAt: now,
    watched: false,
    favorite: false,
    rating: null,
    comment: '',
    markedAt: '',
    lastPlayed: '',
    playable: false,
    resourceCount: 0,
    positionSeconds: 0,
    durationSeconds: 0,
    tags: [],
    collectionIds: [],
    externalIds: [],
  };
  const items = [
    {
      id: 'preview-movie-1',
      kind: 'movie',
      title: '星际穿越',
      year: '2014',
      overview: '一群探险家穿越虫洞，前往遥远星系，为人类寻找新的家园。',
      genres: ['科幻', '冒险'],
      posterUrl: 'https://image.tmdb.org/t/p/w780/gEU2QniE6E77NI6lCU6MxlNBvIx.jpg',
      backdropUrl: 'https://image.tmdb.org/t/p/w1280/xJHokMbljvjADYdit5fK5VQsXEG.jpg',
      recognized: true,
      playable: true,
      resourceCount: 2,
      favorite: true,
      rating: 5,
      watched: true,
      markedAt: before(2).slice(0, 10),
      lastPlayed: before(1),
      positionSeconds: 2520,
      durationSeconds: 10140,
    },
    {
      id: 'preview-movie-2',
      kind: 'movie',
      title: '姥姥的外孙',
      year: '2024',
      overview: '一段祖孙之间的日常，关于陪伴、选择与亲情。',
      genres: ['剧情', '家庭'],
      recognized: true,
      playable: true,
      resourceCount: 1,
      createdAt: before(1),
    },
    {
      id: 'preview-movie-3',
      kind: 'movie',
      title: '海上钢琴师',
      year: '1998',
      overview: '一个从未离开轮船的钢琴家，与音乐相伴的一生。',
      genres: ['剧情', '音乐'],
      recognized: true,
      independent: true,
      watched: true,
      rating: 4,
      comment: '愿每一次重看，都有新的感受。',
      markedAt: before(5).slice(0, 10),
    },
    {
      id: 'preview-series',
      kind: 'series',
      title: '法医秦明',
      year: '2016',
      overview: '法医与刑警携手，在细节之中寻找真相。',
      recognized: true,
      playable: true,
      resourceCount: 3,
      genres: ['悬疑', '犯罪'],
      lastPlayed: before(1),
    },
    {
      id: 'preview-season',
      parentId: 'preview-series',
      kind: 'season',
      title: '第一季',
      seasonNumber: 1,
      recognized: true,
      playable: true,
      resourceCount: 3,
    },
    ...[1, 2, 3].map((number) => ({
      id: 'preview-episode-' + number,
      parentId: 'preview-season',
      kind: 'episode',
      title: '第 ' + number + ' 集',
      seasonNumber: 1,
      episodeNumber: number,
      recognized: true,
      playable: true,
      resourceCount: 1,
      watched: number === 1,
      markedAt: number === 1 ? before(1).slice(0, 10) : '',
      lastPlayed: number === 1 ? before(1) : '',
      durationSeconds: 2700,
      positionSeconds: number === 2 ? 620 : 0,
    })),
    {
      id: 'preview-unidentified',
      kind: 'other',
      title: '未命名视频.1080p.mkv',
      playable: true,
      resourceCount: 1,
      createdAt: before(3),
    },
  ].map((item) => ({ ...base, ...item }));
  const applyMetadata = (item, raw, type = raw.kind === 'movie' ? 'movie' : 'tv') =>
    Object.assign(item, raw, {
      externalIds: [
        {
          provider: 'tmdb',
          providerType: type,
          externalId: String(raw.tmdbId),
        },
      ],
    });
  for (let index = 0; index < 3; index++) applyMetadata(items[index], metadata.movies[index]);
  applyMetadata(
    items.find((item) => item.id === 'preview-series'),
    metadata.originalSeries
  );
  for (let index = 3; index < metadata.movies.length; index++)
    items.push({
      ...base,
      id: 'preview-film-' + index,
      recognized: true,
      playable: true,
      resourceCount: 1,
      favorite: index === 4,
      createdAt: before(index),
      ...metadata.movies[index],
      externalIds: [
        {
          provider: 'tmdb',
          providerType: 'movie',
          externalId: String(metadata.movies[index].tmdbId),
        },
      ],
    });
  // 保留原有法医秦明示例与 ID；新增完整两季的 TMDB 作品，展示整剧与分集。
  const series = {
    ...base,
    ...metadata.series,
    id: 'preview-slow-horses',
    seasonCount: 2,
    recognized: true,
    playable: true,
    resourceCount: 12,
    lastPlayed: now,
    positionSeconds: 1040,
    durationSeconds: 3240,
    favorite: true,
    resumeResourceId: 'preview-file-preview-slow-s1e2',
    currentEpisodeTitle: 'S01E02 · ' + metadata.seasons[0].episodes[1].title,
  };
  applyMetadata(series, metadata.series);
  items.push(series);
  for (const raw of metadata.seasons) {
    const season = {
      ...base,
      ...raw,
      id: 'preview-slow-season-' + raw.seasonNumber,
      parentId: series.id,
      recognized: true,
      playable: true,
      resourceCount: raw.episodes.length - (raw.seasonNumber === 2 ? 1 : 0),
      positionSeconds: raw.seasonNumber === 1 ? 1040 : 0,
      lastPlayed: raw.seasonNumber === 1 ? now : '',
      durationSeconds: 3240,
      favorite: raw.seasonNumber === 1,
      backdropUrl: metadata.series.backdropUrl,
      genres: metadata.series.genres,
    };
    applyMetadata(season, raw, 'season');
    delete season.episodes;
    items.push(season);
    for (const rawEpisode of raw.episodes) {
      const number = rawEpisode.episodeNumber;
      const seasonNumber = raw.seasonNumber;
      const episode = {
        ...base,
        ...rawEpisode,
        id: `preview-slow-s${seasonNumber}e${number}`,
        parentId: season.id,
        recognized: true,
        playable: !(seasonNumber === 2 && [5, 6].includes(number)),
        resourceCount: seasonNumber === 2 && number === 5 ? 0 : 1,
        watched: seasonNumber === 1 && number === 1,
        positionSeconds: seasonNumber === 1 && number === 2 ? 1040 : 0,
        durationSeconds: 3240,
        lastPlayed: seasonNumber === 1 && number === 2 ? now : '',
        backdropUrl: metadata.series.backdropUrl,
      };
      applyMetadata(episode, rawEpisode, 'episode');
      items.push(episode);
    }
  }
  const sources = [
    {
      id: 'preview-source',
      path: String.raw`\\?\L:\youji`,
      label: '我的影视',
      kind: 'auto',
      smartFolders: true,
      available: true,
      lastScan: now,
      error: '',
    },
    {
      id: 'preview-offline-source',
      path: String.raw`\\?\UNC\NAS\共享影视\家庭影院与电视剧`,
      label: '家庭影音（示例）',
      kind: 'auto',
      smartFolders: true,
      available: false,
      lastScan: '',
      error: '示例媒体源暂不可访问',
    },
  ];
  const resources = [
    {
      id: 'preview-resource-1',
      sourceId: sources[0].id,
      path: 'L:\\youji\\星际穿越.2160p.mkv',
      name: '星际穿越.2160p.mkv',
      size: 18 * 1024 ** 3,
      modifiedAt: now,
      available: true,
      missing: false,
      itemIds: ['preview-movie-1'],
      durationSeconds: 10140,
      positionSeconds: 2520,
      technical: {
        container: 'mkv',
        video: [
          {
            codec: 'HEVC',
            width: 3840,
            height: 2160,
            bitrate: 14000000,
            hdr: 'HDR10',
          },
        ],
        audio: [
          { codec: 'TrueHD', channels: 8, language: 'eng' },
          { codec: 'AAC', channels: 2, language: 'zho' },
        ],
        subtitles: [{ codec: 'subrip', language: 'zho' }],
      },
    },
    {
      id: 'preview-resource-2',
      sourceId: sources[0].id,
      path: 'L:\\youji\\星际穿越.1080p.mkv',
      name: '星际穿越.1080p.mkv',
      size: 5 * 1024 ** 3,
      available: true,
      missing: false,
      itemIds: ['preview-movie-1'],
      durationSeconds: 10140,
      positionSeconds: 0,
      technical: {
        container: 'mkv',
        video: [{ codec: 'H.264', width: 1920, height: 1080 }],
      },
    },
  ];
  for (const item of items.filter(
    (item) =>
      item.playable && !item.id.endsWith('movie-1') && !['series', 'season'].includes(item.kind)
  ))
    resources.push({
      id: 'preview-file-' + item.id,
      sourceId: sources[0].id,
      path: 'L:\\youji\\' + item.title + '.mkv',
      name: item.title + '.mkv',
      size: 1024 ** 3,
      available: true,
      missing: false,
      itemIds: [item.id],
      positionSeconds: item.positionSeconds,
      durationSeconds: item.durationSeconds || 7200,
      technical: {
        container: 'mkv',
        video: [{ codec: 'H.264', width: 1920, height: 1080 }],
      },
    });
  for (const item of items) {
    const resource = resources.find((value) => value.itemIds.includes(item.id));
    if (resource) item.resumeResourceId = resource.id;
  }
  const originalSeries = items.find((item) => item.id === 'preview-series');
  originalSeries.seasonCount = 1;
  originalSeries.resumeResourceId = 'preview-file-preview-episode-2';
  const originalSeason = items.find((item) => item.id === 'preview-season');
  originalSeason.year = originalSeries.year;
  // 与桌面端一致：季用剧编号:季号，单集用剧编号:季号:集号。
  for (const kind of ['season', 'episode'])
    for (const item of items.filter((item) => item.kind === kind)) {
      const parent = items.find((parent) => parent.id === item.parentId);
      const identity = parent?.externalIds.find((source) => source.provider === 'tmdb');
      if (!identity) continue;
      const number = kind === 'season' ? item.seasonNumber : item.episodeNumber;
      item.externalIds = [
        {
          provider: 'tmdb',
          providerType: kind,
          externalId: `${identity.externalId}:${number}`,
        },
      ];
    }
  originalSeason.resumeResourceId = originalSeries.resumeResourceId;
  originalSeries.positionSeconds = 620;
  originalSeries.durationSeconds = 2700;
  originalSeries.currentEpisodeTitle = 'S01E02 · 第 2 集';
  // 示例包含离线末集，不把不存在的视频文件伪造为可观看。
  const offline = items.find((item) => item.id === 'preview-slow-s2e6');
  resources.push({
    id: 'preview-offline-episode',
    sourceId: sources[0].id,
    path: 'M:\\TV\\Slow.Horses.S02E06.mkv',
    name: 'Slow.Horses.S02E06.mkv',
    itemIds: [offline.id],
    available: false,
    missing: false,
    positionSeconds: 0,
    durationSeconds: 3240,
  });
  const collections = [
    {
      id: 'preview-collection',
      name: '周末电影',
      sortOrder: 0,
      itemIds: ['preview-movie-1', 'preview-movie-2'],
    },
  ];
  const tags = [{ id: 'preview-tag', name: '值得重看' }];
  items[0].tags = [tags[0]];
  const settings = {
    hasApiKey: false,
    hasReadAccessToken: false,
    playerKind: 'potplayer',
    playerPath: '',
    ffprobePath: '',
    ffmpegPath: '',
    potplayerPollIntervalMs: 500,
    autoScanIntervalMinutes: 30,
    doubanAutoUpdate: false,
    doubanUpdateIntervalMinutes: 360,
  };
  const jobs = [];
  // 预览只使用随源码提供的公开示例；完整生产索引由后台元数据任务建立。
  const people = new Map();
  for (const item of items.filter((value) => ['movie', 'series'].includes(value.kind))) {
    const source = item.externalIds.find((value) => value.provider === 'tmdb');
    for (const person of presentation[source ? `${source.providerType}:${source.externalId}` : '']
      ?.people || []) {
      if (!people.has(person.id))
        people.set(person.id, {
          id: person.id,
          name: person.name,
          profileUrl: person.profileUrl,
          aliases: [person.name],
          departments: [],
          credits: [],
        });
      const indexed = people.get(person.id);
      if (!indexed.departments.includes(person.department))
        indexed.departments.push(person.department);
      indexed.credits.push({
        itemId: item.id,
        scope: source.providerType,
        roles: [person.role],
        departments: [person.department],
      });
    }
  }
  const clone = (value) => structuredClone(value);
  const ignored = [];
  const isIgnored = (sourceId, path) =>
    ignored.some(
      (rule) => rule.sourceId === sourceId && mediaPathMatches(rule.path, path, rule.directory)
    );
  const find = (id) => {
    const item = items.find((item) => item.id === id);
    if (!item) throw new Error('示例条目不存在');
    return item;
  };
  const manualRecords = new Set();
  const recorded = (item) =>
    manualRecords.has(item.id) ||
    item.independent ||
    item.watched ||
    item.favorite ||
    item.rating != null ||
    item.comment?.trim() ||
    item.markedAt ||
    item.lastPlayed ||
    item.tags?.length ||
    collections.some((collection) => collection.itemIds.includes(item.id)) ||
    item.externalIds.some((source) => source.provider !== 'tmdb') ||
    resources.some(
      (resource) => resource.itemIds.includes(item.id) && resource.positionSeconds > 0
    );
  const descendants = (root) => {
    const ids = new Set([root]);
    for (let size = 0; size !== ids.size;) {
      size = ids.size;
      for (const item of items) if (ids.has(item.parentId)) ids.add(item.id);
    }
    return ids;
  };
  const updateResourceState = () => {
    for (const item of items) {
      const ids = descendants(item.id);
      const linked = resources.filter((resource) => resource.itemIds.some((id) => ids.has(id)));
      item.resourceCount = linked.length;
      item.playable = linked.some((resource) => resource.available && !resource.missing);
    }
  };
  const cleanResources = (affected) => {
    const affectedIds = new Set(affected.map((resource) => resource.id));
    const roots = new Set();
    for (const resource of affected) {
      for (const id of resource.itemIds) {
        let item = find(id);
        while (item.kind === 'episode' && item.parentId) item = find(item.parentId);
        roots.add(item.id);
      }
      resource.available = false;
      resource.missing = true;
    }
    for (const root of roots) {
      if (!items.some((item) => item.id === root)) continue;
      const tree = descendants(root);
      const keep =
        items.some((item) => tree.has(item.id) && recorded(item)) ||
        resources.some(
          (resource) =>
            !affectedIds.has(resource.id) &&
            !resource.ignored &&
            resource.itemIds.some((id) => tree.has(id))
        );
      if (keep) continue;
      const parentId = find(root).parentId;
      for (let index = items.length - 1; index >= 0; index--)
        if (tree.has(items[index].id)) items.splice(index, 1);
      const parent = items.find((item) => item.id === parentId);
      if (
        parent &&
        !recorded(parent) &&
        !items.some((item) => item.parentId === parentId) &&
        !resources.some(
          (resource) => !affectedIds.has(resource.id) && resource.itemIds.includes(parentId)
        )
      )
        items.splice(items.indexOf(parent), 1);
    }
    for (let index = resources.length - 1; index >= 0; index--) {
      const resource = resources[index];
      resource.itemIds = resource.itemIds.filter((id) => items.some((item) => item.id === id));
      if (
        affectedIds.has(resource.id) &&
        !items.some((item) => resource.itemIds.includes(item.id) && recorded(item))
      )
        resources.splice(index, 1);
    }
    updateResourceState();
  };
  return async function mediaPreview(name, args = {}) {
    const input = args.input || args;
    if (name === 'media_cleanup') throw new Error('清理条目需要在 Windows 桌面程序中使用。');
    if (name === 'media_match_season') throw new Error('单季目录匹配需要在 Windows 桌面程序中使用。');
    if (name === 'media_settings_snapshot')
      return clone({
        sources: sources.map((source) => ({
          ...source,
          ignoredCount: ignored.filter((rule) => rule.sourceId === source.id).length,
        })),
        jobs,
        settings,
      });
    const peopleIndex = () => {
      const count = items.filter((item) => ['movie', 'series'].includes(item.kind) && item.recognized).length;
      return { total: count, completed: count, failed: 0 };
    };
    if (name === 'media_people_snapshot') return clone({ people: [...people.values()], peopleIndex: peopleIndex() });
    if (name === 'media_snapshot')
      return clone({
        items,
        sources: sources.map((source) => ({
          ...source,
          ignoredCount: ignored.filter((rule) => rule.sourceId === source.id).length,
        })),
        collections,
        tags,
        jobs,
        settings,
        ...(input.includePeople === false ? {} : { people: [...people.values()] }),
        peopleIndex: peopleIndex(),
      });
    if (name === 'media_presentation') {
      const item = find(input.itemId);
      const source = item.externalIds.find((value) => value.provider === 'tmdb');
      return clone(
        presentation[source ? `${source.providerType}:${source.externalId}` : ''] || {
          logoUrl: '',
          people: [],
        }
      );
    }
    if (name === 'media_detail') {
      const item = find(input.itemId);
      const descendants = new Set([item.id]);
      let added = true;
      while (added) {
        added = false;
        for (const child of items)
          if (descendants.has(child.parentId) && !descendants.has(child.id)) {
            descendants.add(child.id);
            added = true;
          }
      }
      const parents = [];
      let cursor = item;
      while (cursor.parentId) {
        cursor = find(cursor.parentId);
        parents.unshift(cursor);
      }
      return clone({
        item,
        parents,
        children: items.filter((child) => child.parentId === item.id),
        resources: resources.filter((resource) =>
          resource.itemIds.some((id) => descendants.has(id))
        ),
      });
    }
    if (name === 'media_set_record') {
      Object.assign(find(input.itemId), input.patch);
      manualRecords.add(input.itemId);
      return {};
    }
    if (name === 'media_create_item') {
      const candidate = input.candidate;
      if (candidate?.provider !== 'tmdb' || !['movie', 'season'].includes(candidate.kind))
        throw new Error('请搜索并选择 TMDB 电影或具体季后添加');
      const type = candidate.kind === 'season' ? 'season' : 'movie';
      const externalId =
        candidate.kind === 'season'
          ? `${candidate.externalId}:${candidate.seasonNumber}`
          : candidate.externalId;
      const item = items.find((item) =>
        item.externalIds.some(
          (source) =>
            source.provider === 'tmdb' &&
            source.providerType === type &&
            source.externalId === externalId
        )
      );
      if (!item) throw new Error('浏览器示例仅支持添加搜索结果中的公开影视');
      item.independent = true;
      return clone(item);
    }
    if (name === 'media_ignored_paths') {
      return clone(
        ignored
          .filter((rule) => !input.sourceId || rule.sourceId === input.sourceId)
          .map((rule) => {
            const source = sources.find((source) => source.id === rule.sourceId);
            return {
              ...rule,
              sourceLabel: source.label,
              sourcePath: source.path,
              covered: ignored.some(
                (other) =>
                  other !== rule && mediaPathMatches(other.path, rule.path, other.directory)
              ),
            };
          })
      );
    }
    if (name === 'media_ignore_path') {
      const source = sources.find((source) => source.id === input.sourceId);
      if (!source) throw new Error('媒体目录不存在');
      if (input.remove) {
        const index = ignored.findIndex(
          (rule) =>
            rule.sourceId === input.sourceId && mediaPathMatches(rule.path, input.path, false)
        );
        if (index >= 0) ignored.splice(index, 1);
      } else {
        if (
          !mediaPathMatches(source.path, input.path) ||
          mediaPathMatches(source.path, input.path, false)
        )
          throw new Error('请选择媒体目录内的文件或子目录');
        if (!isIgnored(input.sourceId, input.path))
          ignored.push({
            sourceId: input.sourceId,
            path: input.path,
            directory: !!input.directory,
          });
      }
      const affected = [];
      for (const resource of resources) {
        resource.ignored = isIgnored(resource.sourceId, resource.path);
        if (resource.ignored) affected.push(resource);
      }
      cleanResources(affected);
      return {};
    }
    if (name === 'media_browse') {
      const source = sources.find((source) => source.id === input.sourceId);
      if (!source) throw new Error('媒体源不存在');
      if (!source.available) throw new Error('示例媒体目录暂不可访问');
      const path = input.path || source.path;
      if (isIgnored(source.id, path)) throw new Error('该目录已忽略，可在忽略列表中取消');
      const inSeries = path !== source.path;
      const inSlowHorses = path.endsWith('\\流人');
      let entries = inSlowHorses
        ? items
            .filter((item) => item.kind === 'season' && item.parentId === 'preview-slow-horses')
            .map((item) => ({
              id: item.id,
              kind: 'item',
              name: item.title,
              path: path + '\\' + item.title,
              directory: true,
              item,
            }))
        : inSeries
          ? items
              .filter(
                (item) =>
                  item.kind === 'episode' &&
                  item.parentId === 'preview-season' &&
                  item.resourceCount
              )
              .map((item) => ({
                id: item.id,
                kind: 'item',
                name: item.title,
                path: path + '\\' + item.title + '.mkv',
                directory: false,
                item,
              }))
          : [
              {
                id: 'preview-extras-folder',
                kind: 'folder',
                name: '花絮与样片',
                path: source.path + '\\花絮与样片',
                directory: true,
              },
              {
                id: 'preview-folder',
                kind: 'folder',
                name: '法医秦明',
                path: source.path + '\\法医秦明',
                directory: true,
              },
              {
                id: 'preview-slow-folder',
                kind: 'folder',
                name: '流人',
                path: source.path + '\\流人',
                directory: true,
              },
              ...items
                .filter((item) => item.playable && !item.parentId && item.kind !== 'series')
                .map((item) => ({
                  id: item.id,
                  kind: 'item',
                  name: item.title,
                  path: source.path + '\\' + item.title + '.mkv',
                  directory: false,
                  item,
                })),
            ];
      if (path.endsWith('\\花絮与样片'))
        entries = ['Sample.mkv', 'Extras.mkv'].map((name) => ({
          kind: 'file',
          name,
          path: path + '\\' + name,
          directory: false,
          resource: {
            id: 'preview-extras-' + name,
            sourceId: source.id,
            name,
            path: path + '\\' + name,
            available: true,
            technical: {},
          },
        }));
      return clone({
        source,
        path,
        parentPath: inSeries ? source.path : null,
        entries: entries.filter((entry) => !isIgnored(source.id, entry.path)),
      });
    }
    if (name === 'media_add_source') {
      let source = sources.find((source) => source.path === input.path);
      if (source) Object.assign(source, input);
      else
        sources.push({
          id: 'preview-source-' + ++sequence,
          available: true,
          lastScan: now,
          error: '',
          ...input,
        });
      return {};
    }
    if (name === 'media_scan') {
      jobs.push({
        id: 'preview-job-' + ++sequence,
        kind: 'scan',
        state: 'done',
        completed: items.length,
        total: items.length,
        message: '示例扫描已完成',
        createdAt: now,
      });
      return {};
    }
    if (name === 'media_remove_source') {
      cleanResources(resources.filter((resource) => resource.sourceId === input.sourceId));
      const index = sources.findIndex((source) => source.id === input.sourceId);
      if (index >= 0) sources.splice(index, 1);
      for (let index = ignored.length - 1; index >= 0; index--)
        if (ignored[index].sourceId === input.sourceId) ignored.splice(index, 1);
      return {};
    }
    if (name === 'media_update_settings') {
      const next = { ...input.settings };
      if (next.tmdbApiKey) settings.hasApiKey = true;
      if (next.tmdbReadAccessToken) settings.hasReadAccessToken = true;
      delete next.tmdbApiKey;
      delete next.tmdbReadAccessToken;
      Object.assign(settings, next);
      return clone(settings);
    }
    if (name === 'media_search_metadata')
      return clone(
        items
          .filter(
            (item) =>
              item.recognized &&
              ['movie', 'season'].includes(item.kind) &&
              (input.kind !== 'movie' || item.kind === 'movie') &&
              (!['series', 'tv'].includes(input.kind) || item.kind === 'season') &&
              (!input.query || mediaItemTitle(item, items).includes(input.query))
          )
          .map((item) => {
            const source = item.externalIds.find((source) => source.provider === 'tmdb');
            return {
              ...item,
              title: mediaItemTitle(item, items),
              provider: 'tmdb',
              providerType: item.kind === 'movie' ? 'movie' : 'tv',
              externalId: source?.externalId.split(':')[0],
            };
          })
      );
    if (name === 'media_identify') {
      const item = find(input.itemId);
      if (input.unlink) {
        item.recognized = false;
        item.externalIds = item.externalIds.filter(source => source.provider === 'douban');
      } else if (input.targetItemId) {
        const target = find(input.targetItemId);
        if (target === item) return { itemId: target.id };
        if (item.recognized || !target.recognized) throw new Error('仅允许将未识别条目合并到已识别作品');
        if (item.externalIds.some(left => target.externalIds.some(right => left.provider === right.provider
          && left.providerType === right.providerType && left.externalId !== right.externalId)))
          throw new Error('双方已有冲突的来源身份，不能合并');
        if (manualRecords.delete(item.id)) manualRecords.add(target.id);
        for (const field of ['watched', 'favorite', 'independent']) target[field] ||= item[field];
        for (const field of ['rating', 'comment', 'markedAt', 'lastPlayed'])
          if (!target[field]) target[field] = item[field];
        target.tags = [
          ...new Map([...target.tags, ...item.tags].map((tag) => [tag.id, tag])).values(),
        ];
        for (const collection of collections)
          collection.itemIds = [
            ...new Set(collection.itemIds.map((id) => (id === item.id ? target.id : id))),
          ];
        for (const resource of resources)
          resource.itemIds = [
            ...new Set(resource.itemIds.map((id) => (id === item.id ? target.id : id))),
          ];
        for (const child of items) if (child.parentId === item.id) child.parentId = target.id;
        for (const person of people.values())
          for (const credit of person.credits)
            if (credit.itemId === item.id) credit.itemId = target.id;
        for (const source of item.externalIds)
          if (
            !target.externalIds.some(
              (other) =>
                other.provider === source.provider &&
                other.providerType === source.providerType &&
                other.externalId === source.externalId
            )
          )
            target.externalIds.push(source);
        items.splice(items.indexOf(item), 1);
        updateResourceState();
        return { itemId: target.id };
      } else if (input.candidate) {
        const id = item.id;
        Object.assign(item, input.candidate, { id, recognized: true });
      }
      return clone(item);
    }
    if (name === 'media_refresh_metadata') return clone(find(input.itemId));
    if (name === 'media_probe') {
      const resource = resources.find((resource) => resource.id === input.resourceId);
      if (!resource) throw new Error('示例资源不存在');
      return clone(resource);
    }
    if (name === 'media_play')
      return {
        message: '浏览器预览不启动实际播放器，请在 Windows 桌面应用中播放。',
      };
    if (name === 'media_player_status' || name === 'media_player_stop')
      return { state: 'idle', active: false };
    if (name === 'media_douban_status')
      return { connected: false, message: '浏览器预览不保存豆瓣登录会话。' };
    if (['media_douban_begin_login', 'media_douban_sync', 'media_douban_disconnect', 'media_douban_metadata'].includes(name))
      throw new Error('请在 Windows 桌面应用中连接豆瓣。');
    if (name === 'media_organization') {
      const id = input.collectionId || input.id;
      if (input.action === 'createCollection')
        collections.push({
          id: 'preview-collection-' + ++sequence,
          name: input.name,
          itemIds: [],
          sortOrder: collections.length,
        });
      if (input.action === 'renameCollection') {
        const target = collections.find((value) => value.id === id);
        if (target) target.name = input.name;
      }
      if (input.action === 'deleteCollection') {
        const index = collections.findIndex((value) => value.id === id);
        if (index >= 0) collections.splice(index, 1);
      }
      if (input.action === 'setCollectionItems') {
        const target = collections.find((value) => value.id === id);
        if (target)
          target.itemIds =
            input.mode === 'remove'
              ? target.itemIds.filter((value) => !input.itemIds.includes(value))
              : [...new Set([...target.itemIds, ...input.itemIds])];
      }
      if (input.action === 'createTag')
        tags.push({ id: 'preview-tag-' + ++sequence, name: input.name });
      if (input.action === 'setTags')
        for (const itemId of input.itemIds || []) {
          const item = find(itemId);
          const ids =
            input.mode === 'add'
              ? [...new Set([...(item.tags || []).map((tag) => tag.id), ...input.tagIds])]
              : input.tagIds;
          item.tags = tags.filter((tag) => ids.includes(tag.id));
        }
      return {};
    }
    if (name === 'media_statistics') {
      const days =
        input.period === 'week'
          ? 7
          : input.period === 'year'
            ? 365
            : input.period === 'all'
              ? 30
              : 30;
      const to = now.slice(0, 10),
        from = before(days - 1).slice(0, 10);
      const records = items
        .filter((item) => item.watched || item.rating || item.comment)
        .filter(
          (item) =>
            input.kind === 'all' ||
            !input.kind ||
            item.kind === input.kind ||
            (input.kind === 'series' && ['season', 'episode'].includes(item.kind))
        )
        .map((item) => ({
          itemId: item.id,
          title: item.title,
          kind: item.kind,
          markedAt: item.markedAt,
          activityAt: item.markedAt,
          activityDates: [item.markedAt].filter(Boolean),
          watched: !!item.watched,
          watchedInRange: !!item.watched,
          rating: item.rating,
          comment: item.comment,
          source: item.independent ? 'douban' : 'local',
          lastPlayed: item.lastPlayed,
          playedSeconds: 0,
        }))
        .filter(
          (record) =>
            (input.source === 'all' || !input.source || record.source === input.source) &&
            (input.period === 'all' || record.markedAt >= from)
        );
      const rated = records.filter((record) => record.rating != null);
      const genres = new Map();
      for (const record of records)
        for (const name of find(record.itemId).genres || [])
          genres.set(name, (genres.get(name) || 0) + 1);
      const daily = Array.from({ length: Math.min(days, 365) }, (_, index) => {
        const date = before(days - 1 - index).slice(0, 10);
        return {
          date,
          count: records.filter((record) => record.markedAt === date).length,
          seconds: 0,
        };
      });
      return {
        from,
        to,
        summary: {
          watchedMovies: records.filter((record) => record.kind === 'movie').length,
          watchedSeries: records.filter((record) => record.kind === 'series').length,
          watchedSeasons: records.filter((record) => record.kind === 'season').length,
          watchedEpisodes: records.filter((record) => record.kind === 'episode').length,
          activeDays: new Set(records.map((record) => record.markedAt).filter(Boolean)).size,
          averageRating: rated.length
            ? rated.reduce((sum, record) => sum + record.rating, 0) / rated.length
            : null,
          playedSeconds: 0,
          recordCount: records.length,
        },
        daily,
        ratingDistribution: [1, 2, 3, 4, 5].map((rating) => ({
          rating,
          count: rated.filter((record) => record.rating === rating).length,
        })),
        genreDistribution: [...genres].map(([name, count]) => ({
          name,
          count,
        })),
        records,
        sessions: [],
      };
    }
    throw new Error('影视预览尚未实现此操作：' + name);
  };
}
