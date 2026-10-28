import test from 'node:test';
import assert from 'node:assert/strict';
import {
  queryMedia,
  mediaItemTitle,
  libraryRoots,
  inMediaCategory,
  progressPercent,
  resourceTechnical,
  resourceOwnerId,
  safeImageUrl,
  durationLabel,
  mediaKindName,
  mediaGenres,
  recognitionLabel,
} from '../src/lib/media-query.js';
import { createMediaPreview } from '../src/lib/media-preview.js';

const items = [
  {
    id: 'movie',
    title: 'A 电影',
    kind: 'movie',
    recognized: true,
    playable: true,
    watched: true,
    lastPlayed: '2026-10-24T13:00:00Z',
    createdAt: '2026-10-01',
    rating: 5,
    tags: [{ id: 'tag' }],
  },
  {
    id: 'douban',
    title: 'B 导入电影',
    kind: 'movie',
    recognized: true,
    playable: false,
    watched: true,
    markedAt: '2020-01-01',
    createdAt: '2026-10-25',
  },
  {
    id: 'unknown',
    title: 'C 未识别.mkv',
    kind: 'other',
    recognized: false,
    playable: true,
    watched: false,
    createdAt: '2026-10-20',
  },
  {
    id: 'series',
    title: 'D 电视剧',
    kind: 'series',
    recognized: true,
    playable: true,
    watched: false,
  },
  {
    id: 'season',
    parentId: 'series',
    title: '第一季',
    kind: 'season',
    recognized: true,
    playable: true,
  },
  {
    id: 'episode',
    parentId: 'season',
    title: '第一集',
    kind: 'episode',
    recognized: true,
    playable: true,
    watched: true,
  },
];
const ids = (options) => queryMedia(items, options).map((item) => item.id);

test('影视配置快照只包含配置页需要的数据', async () => {
  const preview = createMediaPreview();
  const settings = await preview('media_settings_snapshot');
  const full = await preview('media_snapshot');
  assert.deepEqual(settings, { sources: full.sources, jobs: full.jobs, settings: full.settings });
  assert.equal(Object.hasOwn(settings, 'items'), false);
  assert.equal(Object.hasOwn(settings, 'people'), false);
});
test('影视库轻量快照不包含演职员，人员快照可按需单独读取', async () => {
  const preview = createMediaPreview();
  const full = await preview('media_snapshot');
  const light = await preview('media_snapshot', { input: { includePeople: false } });
  const people = await preview('media_people_snapshot');
  assert.equal(Object.hasOwn(light, 'people'), false);
  assert.deepEqual(light.items, full.items);
  assert.deepEqual(light.peopleIndex, full.peopleIndex);
  assert.deepEqual(people, { people: full.people, peopleIndex: full.peopleIndex });
});

test('对外类型统一为电影和电视剧，不将季或单集作为作品类型', () => {
  assert.equal(mediaKindName('movie'), '电影');
  for (const kind of ['series', 'season', 'episode']) assert.equal(mediaKindName(kind), '电视剧');
  assert.equal(mediaKindName('other'), '类型未知');
});

test('识别状态区分待识别、未匹配和临时失败，已识别不显示标志', () => {
  assert.equal(recognitionLabel({ recognized: false }), '待识别');
  assert.equal(recognitionLabel({ recognized: false, recognitionStatus: 'pending' }), '待识别');
  assert.equal(recognitionLabel({ recognized: false, recognitionStatus: 'unmatched' }), '未匹配');
  assert.equal(recognitionLabel({ recognized: false, recognitionStatus: 'failed' }), '识别失败');
  assert.equal(recognitionLabel({ recognized: true, recognitionStatus: 'failed' }), '');
});

test('空的季题材标签回退所属电视剧，具体题材优先且不改写原资料', () => {
  const series = { id: 'tv', genres: ['悬疑', '犯罪'] };
  const season = { id: 's', parentId: 'tv', genres: [] };
  const episode = { id: 'e', parentId: 's', genres: [] };
  const items = [series, season, episode];
  assert.deepEqual(mediaGenres(season, items), ['悬疑', '犯罪']);
  assert.deepEqual(mediaGenres(episode, items), ['悬疑', '犯罪']);
  assert.deepEqual(mediaGenres({ ...season, genres: [' 剧情 ', '剧情', '', null] }, items), [
    '剧情',
  ]);
  assert.deepEqual(season.genres, []);
  assert.deepEqual(mediaGenres({ id: 'a', parentId: 'b' }, [{ id: 'b', parentId: 'a' }]), []);
});

test('季和单集显示剧名与集号，本地目录内可按剧名搜索', () => {
  const values = [
    { id: 'tv', kind: 'series', title: '流人' },
    { id: 'season', kind: 'season', title: '第二季', parentId: 'tv', seasonNumber: 2 },
    {
      id: 'episode',
      kind: 'episode',
      title: '蝉',
      parentId: 'season',
      seasonNumber: 2,
      episodeNumber: 4,
    },
  ];
  assert.equal(mediaItemTitle(values[1], values), '流人 · 第二季');
  assert.equal(mediaItemTitle(values[2], values), '流人 · S02E04 · 蝉');
  assert.equal(
    mediaItemTitle(
      { id: 'orphan', kind: 'episode', title: '原始名称', parentId: 'missing' },
      values
    ),
    '原始名称'
  );
  assert.equal(mediaItemTitle({ ...values[2], parentId: 'tv' }, values), '流人 · S02E04 · 蝉');
});

test('只对资料确认的单季剧省略第一季，多季剧即使只收录一季仍带季名', () => {
  const season = { id: 's1', kind: 'season', parentId: 'tv', seasonNumber: 1, title: '第一季' };
  assert.equal(
    mediaItemTitle(season, [{ id: 'tv', title: '剧名', seasonCount: 1 }, season]),
    '剧名'
  );
  assert.equal(
    mediaItemTitle(season, [{ id: 'tv', title: '剧名', seasonCount: 3 }, season]),
    '剧名 · 第一季'
  );
  assert.equal(mediaItemTitle(season, [{ id: 'tv', title: '剧名' }, season]), '剧名 · 第一季');
});

test('多季按同一剧名及季号排序，季的独立状态决定筛选', () => {
  const values = [
    { id: 'tv', kind: 'series', title: '剧名', recognized: true, watched: true },
    ...[10, 2, 1].map((n) => ({
      id: 's' + n,
      kind: 'season',
      parentId: 'tv',
      title: '第' + n + '季',
      seasonNumber: n,
      watched: n === 2,
      playable: n === 1,
    })),
  ];
  assert.deepEqual(
    queryMedia(values).map((v) => v.id),
    ['s1', 's2', 's10']
  );
  assert.deepEqual(
    queryMedia(values, { category: 'watched' }).map((v) => v.id),
    ['s2']
  );
  assert.deepEqual(
    queryMedia(values, { onlyPlayable: true }).map((v) => v.id),
    ['s1']
  );
});

test('分类固定的条件覆盖对应隐藏筛选，不会出现未识别与已识别的矛盾', () => {
  assert.deepEqual(ids({ category: 'other', onlyRecognized: true }), ['unknown']);
  assert.deepEqual(ids({ category: 'unwatched', watchedFilter: 'watched' }), ['unknown', 'season']);
  assert.deepEqual(
    ids({ category: 'watched', watchedFilter: 'unwatched' }),
    ids({ category: 'watched' })
  );
});

test('已识别与可观看是独立筛选，两个开关同时开启取交集', () => {
  assert.deepEqual(ids({ onlyRecognized: true, onlyPlayable: true }), ['movie', 'season']);
  assert.deepEqual(ids({ onlyRecognized: true }), ['movie', 'douban', 'season']);
  assert.deepEqual(ids({ onlyPlayable: true }), ['movie', 'unknown', 'season']);
});
test('全部影视按电影和季展示，豆瓣无资源条目和未识别视频仍可见', () => {
  assert.deepEqual(
    libraryRoots(items).map((item) => item.id),
    ['movie', 'douban', 'unknown', 'season']
  );
  assert.ok(ids({}).includes('douban'));
  assert.ok(ids({}).includes('unknown'));
  assert.ok(!ids({}).includes('removed'));
});
test('最近观看只依据播放时间，不把豆瓣导入或标记日期当作一次播放', () => {
  assert.deepEqual(ids({ category: 'recent' }), ['movie']);
  assert.equal(ids({ category: 'added' })[0], 'douban');
  assert.equal(inMediaCategory({ ...items[0], positionSeconds: 200 }, 'watched'), true);
});
test('收藏夹内可以展示直接加入的单季或单集，并与类型和标签组合', () => {
  const collections = [{ id: 'list', itemIds: ['episode', 'movie'] }];
  assert.deepEqual(
    queryMedia(items, { collectionId: 'list' }, collections)
      .map((item) => item.id)
      .sort(),
    ['episode', 'movie']
  );
  assert.deepEqual(
    queryMedia(
      items,
      { collectionId: 'list', category: 'movies', tagIds: ['tag'] },
      collections
    ).map((item) => item.id),
    ['movie']
  );
  assert.deepEqual(ids({ category: 'unwatched', onlyPlayable: true }), ['unknown', 'season']);
});
test('我的收藏保留直接收藏的季和单集，不被作品聚合隐藏', () => {
  const catalog = items.map((item) => ({
    ...item,
    favorite: ['movie', 'season', 'episode'].includes(item.id),
  }));
  assert.deepEqual(
    queryMedia(catalog, { category: 'favorites' })
      .map((item) => item.id)
      .sort(),
    ['episode', 'movie', 'season']
  );
  assert.deepEqual(
    queryMedia(catalog, {
      category: 'favorites',
      onlyPlayable: true,
      onlyRecognized: true,
    })
      .map((item) => item.id)
      .sort(),
    ['episode', 'movie', 'season']
  );
});
test('剧季解除资源关联使用有效后代直接owner，不选无关条目', () => {
  const resource = { itemIds: ['movie', 'episode'] };
  assert.equal(
    resourceOwnerId(
      items.find((item) => item.id === 'series'),
      resource,
      items
    ),
    'episode'
  );
  assert.equal(
    resourceOwnerId(
      items.find((item) => item.id === 'season'),
      resource,
      items
    ),
    'episode'
  );
  assert.equal(
    resourceOwnerId(
      items.find((item) => item.id === 'movie'),
      resource,
      items
    ),
    'movie'
  );
  assert.equal(
    resourceOwnerId(
      items.find((item) => item.id === 'unknown'),
      resource,
      items
    ),
    ''
  );
  assert.equal(
    resourceOwnerId(
      items.find((item) => item.id === 'series'),
      { itemIds: ['movie'] },
      items
    ),
    ''
  );
});
test('进度仅由实际位置和时长计算，错误或未知值不生成虚假百分比', () => {
  assert.equal(progressPercent({ durationSeconds: 100, positionSeconds: 30 }), 30);
  assert.equal(progressPercent({ durationSeconds: 0, positionSeconds: 50 }), 0);
  assert.equal(progressPercent({ durationSeconds: 100, positionSeconds: 200 }), 100);
  assert.equal(progressPercent({ durationSeconds: 100, positionSeconds: 'unknown' }), 0);
  assert.equal(durationLabel(20), '不足 1 分钟');
});
test('技术信息保留多条音频和字幕轨道，不以总码率假装视频码率', () => {
  const info = resourceTechnical({
    technical: {
      container: 'matroska',
      totalBitrate: 9000000,
      video: [{ width: 3840, height: 2160, codec: 'hevc' }],
      audio: [{ codec: 'aac' }, { codec: 'truehd' }],
      subtitles: [{ codec: 'subrip' }],
    },
  });
  assert.equal(info.audios.length, 2);
  assert.equal(info.subtitles.length, 1);
  assert.equal(info.resolution, '3840 × 2160');
  assert.equal(info.video.bitrate, undefined);
  assert.equal(info.format, 'matroska');
  assert.equal(info.codec, 'hevc');
  assert.match(info.summary, /4K/);
});
test('海报地址禁止脚本和任意本地路径', () => {
  assert.equal(safeImageUrl('javascript:alert(1)'), '');
  assert.equal(safeImageUrl('C:\\private\\file.png'), '');
  assert.equal(safeImageUrl('file:///C:/private.png'), '');
  assert.equal(
    safeImageUrl('https://image.tmdb.org/t/p/w500/test.jpg'),
    'https://image.tmdb.org/t/p/w500/test.jpg'
  );
});
test('实际探测结果直接使用统一的video和audio数组', () => {
  const info = resourceTechnical({
    technical: {
      container: 'matroska',
      totalBitrate: 9000000,
      video: [{ codec: 'hevc', width: 1920, height: 1080 }],
      audio: [
        { codec: 'aac', channels: 2 },
        { codec: 'ac3', channels: 6 },
      ],
      subtitles: [{ codec: 'srt' }],
    },
  });
  assert.equal(info.videos.length, 1);
  assert.equal(info.audios.length, 2);
  assert.equal(info.subtitles.length, 1);
  assert.equal(info.codec, 'hevc');
  assert.equal(info.video.bitrate, undefined);
  assert.match(info.summary, /1080p/);
});
test('公开预览与桌面端共用媒体轨道和目录结构', async () => {
  const preview = createMediaPreview();
  const detail = await preview('media_detail', { itemId: 'preview-movie-1' });
  const info = resourceTechnical(detail.resources[0]);
  assert.equal(info.codec, 'HEVC');
  assert.equal(info.resolution, '3840 × 2160');
  assert.equal(info.audios.length, 2);
  assert.equal(info.subtitles.length, 1);
  assert.equal(info.video.bitrate, 14000000);
  assert.match(info.summary, /HDR10/);
  const source = (await preview('media_snapshot')).sources[0];
  assert.ok(
    (await preview('media_snapshot')).items.every((item) => !Object.hasOwn(item, 'visible'))
  );
  const browse = await preview('media_browse', { sourceId: source.id });
  for (const entry of browse.entries) assert.equal(entry.directory, entry.kind === 'folder');
});
test('预览移除媒体源真清理纯资源条目，保留无资源记录、独立来源和受保护季集', async () => {
  const preview = createMediaPreview();
  const before = await preview('media_snapshot');
  const pure = before.items.find((item) => item.id === 'preview-film-3');
  assert.equal(pure.recognized, true);
  await preview('media_set_record', {
    itemId: 'preview-film-5',
    patch: { watched: false },
  });
  await preview('media_remove_source', { sourceId: before.sources[0].id });
  const after = await preview('media_snapshot');
  assert.ok(!after.sources.some((source) => source.id === before.sources[0].id));
  assert.ok(!after.items.some((item) => item.id === pure.id));
  assert.ok(!after.items.some((item) => item.id === 'preview-unidentified'));
  await assert.rejects(preview('media_detail', { itemId: pure.id }), /不存在/);
  for (const id of [
    'preview-movie-1',
    'preview-movie-3',
    'preview-season',
    'preview-episode-3',
    'preview-film-5',
  ])
    assert.ok(after.items.some((item) => item.id === id));
  const retained = await preview('media_detail', { itemId: 'preview-movie-1' });
  assert.ok(retained.resources.length > 0);
  assert.ok(retained.resources.every((resource) => resource.missing && !resource.available));
  assert.equal(retained.resources[0].positionSeconds, 2520);
  assert.equal(retained.item.playable, false);
  assert.ok(queryMedia(after.items).some((item) => item.id === retained.item.id));
  assert.ok(after.items.every((item) => !Object.hasOwn(item, 'visible')));
});

test('预览关联已有条目合并记录与资源后删除原实体', async () => {
  const preview = createMediaPreview();
  await preview('media_set_record', {
    itemId: 'preview-unidentified',
    patch: { rating: 3, comment: '原条目观影记录' },
  });
  await preview('media_identify', {
    itemId: 'preview-unidentified',
    targetItemId: 'preview-movie-2',
  });
  const snapshot = await preview('media_snapshot');
  assert.ok(!snapshot.items.some((item) => item.id === 'preview-unidentified'));
  await assert.rejects(preview('media_detail', { itemId: 'preview-unidentified' }), /不存在/);
  const target = await preview('media_detail', { itemId: 'preview-movie-2' });
  assert.equal(target.item.rating, 3);
  assert.equal(target.item.comment, '原条目观影记录');
  assert.equal(target.resources.length, 2);
  assert.ok(
    target.resources.every((resource) => !resource.itemIds.includes('preview-unidentified'))
  );
});
test('浏览器预览不保存凭据值，也不将点击播放伪造为实际观看', async () => {
  const preview = createMediaPreview();
  const first = await preview('media_snapshot');
  await preview('media_update_settings', {
    input: { settings: { tmdbReadAccessToken: 'test-secret-do-not-store' } },
  });
  const next = await preview('media_snapshot');
  assert.equal(next.settings.hasReadAccessToken, true);
  assert.equal(JSON.stringify(next).includes('test-secret-do-not-store'), false);
  await preview('media_play', { input: { itemId: 'preview-movie-2' } });
  const last = await preview('media_snapshot');
  assert.equal(
    last.items.find((item) => item.id === 'preview-movie-2').lastPlayed,
    first.items.find((item) => item.id === 'preview-movie-2').lastPlayed
  );
  assert.deepEqual(await preview('media_player_status'), {
    state: 'idle',
    active: false,
  });
  await preview('media_player_stop');
  assert.deepEqual(await preview('media_snapshot'), last);
});
test('预览收藏夹与标签操作幂等，移除收藏夹不删除条目', async () => {
  const preview = createMediaPreview();
  const update = () =>
    preview('media_organization', {
      input: {
        action: 'setCollectionItems',
        collectionId: 'preview-collection',
        itemIds: ['preview-unidentified'],
        mode: 'add',
      },
    });
  await update();
  await update();
  let data = await preview('media_snapshot');
  assert.equal(data.collections[0].itemIds.filter((id) => id === 'preview-unidentified').length, 1);
  await preview('media_organization', {
    input: { action: 'deleteCollection', id: 'preview-collection' },
  });
  data = await preview('media_snapshot');
  assert.equal(data.collections.length, 0);
  assert.ok(data.items.some((item) => item.id === 'preview-unidentified'));
});

test('预览检索具体季后重复添加复用季身份，不创建空条目或改写个人记录', async () => {
  const preview = createMediaPreview();
  const first = await preview('media_snapshot');
  const candidates = await preview('media_search_metadata', {
    input: { query: '流人', kind: 'series' },
  });
  assert.equal(candidates.length, 2);
  const candidate = candidates.find((candidate) => candidate.seasonNumber === 2);
  const series = first.items.find((item) => item.id === 'preview-slow-horses');
  assert.equal(candidate.externalId, series.externalIds[0].externalId);
  assert.equal(candidate.kind, 'season');
  assert.equal((await preview('media_snapshot')).items.length, first.items.length);
  await preview('media_set_record', {
    input: { itemId: candidate.id, patch: { rating: 4, comment: '保留本季记录' } },
  });
  const added = await preview('media_create_item', { input: { candidate } });
  const again = await preview('media_create_item', { input: { candidate } });
  assert.equal(added.id, 'preview-slow-season-2');
  assert.equal(again.id, added.id);
  assert.equal(again.independent, true);
  assert.equal(again.rating, 4);
  assert.equal(again.comment, '保留本季记录');
  assert.equal((await preview('media_snapshot')).items.length, first.items.length);
  await assert.rejects(preview('media_create_item', { input: { title: '不能创建空影视' } }));
  assert.equal((await preview('media_snapshot')).items.length, first.items.length);
});

test('预览季与单集身份遵循桌面端复合编号规则', async () => {
  const preview = createMediaPreview();
  const { items } = await preview('media_snapshot');
  for (const item of items.filter((item) => ['season', 'episode'].includes(item.kind))) {
    const parent = items.find((parent) => parent.id === item.parentId);
    const number = item.kind === 'season' ? item.seasonNumber : item.episodeNumber;
    assert.deepEqual(item.externalIds, [
      {
        provider: 'tmdb',
        providerType: item.kind,
        externalId: `${parent.externalIds[0].externalId}:${number}`,
      },
    ]);
  }
});

test('全部影视按季展示，已观看只依据该季记录', () => {
  const root = { id: 'show', title: '某剧', kind: 'series', watched: false };
  const first = {
    id: 's1',
    title: '第一季',
    kind: 'season',
    parentId: root.id,
    watched: true,
  };
  const second = {
    id: 's2',
    title: '第二季',
    kind: 'season',
    parentId: root.id,
    watched: false,
  };
  const values = [root, first, second];
  assert.deepEqual(
    queryMedia(values, { category: 'watched' }).map((item) => item.id),
    ['s1']
  );
  assert.deepEqual(
    queryMedia(values).map((item) => item.id),
    ['s1', 's2']
  );
  assert.equal(mediaItemTitle(first, values), '某剧 · 第一季');
  assert.deepEqual(
    queryMedia(values, { category: 'watched', search: '某剧' }).map((item) => item.id),
    ['s1']
  );
  assert.equal(mediaItemTitle(root, values), '某剧');
});
