import test from 'node:test';
import assert from 'node:assert/strict';
import {
  detailModel,
  seriesContext,
  metadataSource,
  episodeLabel,
  episodeAvailability,
} from '../src/lib/media-detail-model.js';
import { createMediaPreview } from '../src/lib/media-preview.js';

async function fixture() {
  const command = createMediaPreview();
  const data = await command('media_snapshot');
  const detail = await command('media_detail', {
    itemId: 'preview-slow-horses',
  });
  return { command, data, detail };
}

test('完整集目保留无资源单集，筛选资源仍保留离线集，切季不会借用其他季资源', async () => {
  const { data, detail } = await fixture();
  const selection = { seasonId: 'preview-slow-season-2', episodeId: 'preview-slow-s2e5' };
  const model = detailModel(detail, data.items, selection);
  assert.equal(model.visibleEpisodes.length, 6);
  assert.equal(model.episode.id, selection.episodeId);
  assert.equal(model.resource, undefined);
  assert.equal(episodeAvailability(model.episode), '暂无资源');
  const filtered = detailModel(detail, data.items, { ...selection, onlyWithResources: true });
  assert.equal(filtered.visibleEpisodes.length, 5);
  assert.ok(filtered.visibleEpisodes.some((v) => v.id === 'preview-slow-s2e6'));
  const emptySeason = { id: 'empty', kind: 'season', parentId: detail.item.id };
  const empty = detailModel(detail, [...data.items, emptySeason], { seasonId: emptySeason.id });
  assert.equal(empty.target, emptySeason);
  assert.equal(empty.resource, undefined);
});

test('未播出由明确日期判断，无资源和离线不会被当成可播放', () => {
  const today = new Date(2026, 9, 26, 12);
  assert.equal(episodeAvailability({ airDate: '2026-10-27' }, today), '未播出 · 2026-10-27');
  assert.equal(episodeAvailability({ airDate: '2026-10-26' }, today), '暂无资源');
  assert.equal(episodeAvailability({ airDate: '2027-01-01', resourceCount: 1 }, today), '资源离线');
  assert.equal(episodeAvailability({ playable: true }, today), '可播放');
});

test('整剧详情直接取得两季与分集资源，默认继续实际资源所属的那一集', async () => {
  const { data, detail } = await fixture();
  const model = detailModel(detail, data.items);
  assert.equal(model.seasons.length, 2);
  assert.equal(model.episodes.length, 12);
  assert.equal(detail.resources.length, 11);
  assert.equal(model.episode.id, 'preview-slow-s1e2');
  assert.equal(model.resource.id, detail.item.resumeResourceId);
  assert.deepEqual(model.resource.itemIds, [model.target.id]);
  assert.equal(model.resource.positionSeconds, 1040);
});

test('切季不跳页，也不会误播其他季的续播资源', async () => {
  const { data, detail } = await fixture();
  const model = detailModel(detail, data.items, {
    seasonId: 'preview-slow-season-2',
  });
  assert.equal(model.episode.id, 'preview-slow-s2e1');
  assert.equal(model.visibleEpisodes.length, 6);
  assert.deepEqual(model.resource.itemIds, ['preview-slow-s2e1']);
  const selected = detailModel(detail, data.items, {
    seasonId: model.season.id,
    episodeId: 'preview-slow-s2e4',
  });
  assert.deepEqual(selected.resource.itemIds, ['preview-slow-s2e4']);
});

test('从某季进入优先续播这一季的资源，不受其他季最近播放的影响', async () => {
  const { data, detail } = await fixture();
  const season = data.items.find((v) => v.id === 'preview-slow-season-2');
  const resource = detail.resources.find((v) => v.itemIds.includes('preview-slow-s2e4'));
  season.resumeResourceId = resource.id;
  detail.children.find((value) => value.id === season.id).resumeResourceId = resource.id;
  resource.positionSeconds = 200;
  const model = detailModel(detail, data.items, { seasonId: season.id });
  assert.equal(model.episode.id, 'preview-slow-s2e4');
  assert.equal(model.resource.positionSeconds, 200);
});

test('选择离线集保留单集信息，不偷换为其他可播放的集', async () => {
  const { data, detail } = await fixture();
  const model = detailModel(detail, data.items, {
    seasonId: 'preview-slow-season-2',
    episodeId: 'preview-slow-s2e6',
  });
  assert.equal(model.target.id, 'preview-slow-s2e6');
  assert.equal(model.resource, undefined);
  assert.equal(model.versions.length, 1);
  assert.equal(model.versions[0].available, false);
});

test('收藏或文件页的季／单集入口解析到整剧，孤立单集和循环关系安全结束', async () => {
  const { data } = await fixture();
  const episode = data.items.find((item) => item.id === 'preview-slow-s2e4');
  assert.equal(seriesContext(episode, data.items).id, 'preview-slow-horses');
  const orphan = { id: 'orphan', kind: 'episode', parentId: 'missing' };
  assert.equal(seriesContext(orphan, []), orphan);
  const a = { id: 'a', kind: 'season', parentId: 'b' },
    b = { id: 'b', kind: 'series', parentId: 'a' };
  assert.ok(seriesContext(a, [a, b]));
});

test('影片版本选择沿用实际续播指针，失效资源不成为播放目标', async () => {
  const { data, command } = await fixture();
  const detail = await command('media_detail', { itemId: 'preview-movie-1' });
  assert.equal(detailModel(detail, data.items).resource.id, 'preview-resource-1');
  detail.resources[0].available = false;
  assert.equal(detailModel(detail, data.items).resource.id, 'preview-resource-2');
  detail.resources[1].missing = true;
  assert.equal(detailModel(detail, data.items).resource, undefined);
});

test('首次播放顺序按季集数值排列，特别篇0与资料缺失不混淆', () => {
  const root = { id: 's', kind: 'series' },
    season = { id: 's0', kind: 'season', parentId: 's', seasonNumber: 0 };
  const episodes = [10, 2, 1].map((n) => ({
    id: 'e' + n,
    kind: 'episode',
    parentId: 's0',
    seasonNumber: 0,
    episodeNumber: n,
    playable: true,
  }));
  const model = detailModel({ item: root, resources: [] }, [root, season, ...episodes]);
  assert.equal(model.target.id, 'e1');
  assert.deepEqual(
    model.episodes.map((item) => item.episodeNumber),
    [1, 2, 10]
  );
  assert.equal(episodeLabel(episodes[0]), 'S00E10');
});

test('作品来源以TMDB优先，个人评分不冒充资料源评分', () => {
  assert.equal(
    metadataSource({
      externalIds: [{ provider: 'douban' }, { provider: 'tmdb' }],
      rating: 5,
    }),
    'TMDB'
  );
  assert.equal(metadataSource({ externalIds: [{ provider: 'douban' }] }), '豆瓣导入');
  assert.equal(metadataSource({}), '本地视频');
});

test('按季切换个人记录，单季豆瓣已看不扩展为整剧或逐集已看', () => {
  const item = { id: 'series', kind: 'series', watched: false };
  const first = {
    id: 'season1',
    kind: 'season',
    parentId: item.id,
    seasonNumber: 1,
    watched: true,
    rating: 4,
    comment: '第一季短评',
  };
  const second = {
    id: 'season2',
    kind: 'season',
    parentId: item.id,
    seasonNumber: 2,
    watched: false,
  };
  const items = [item, first, second];
  const a = detailModel({ item }, items, { seasonId: first.id });
  const b = detailModel({ item }, items, { seasonId: second.id });
  assert.equal(a.recordItem, first);
  assert.equal(a.recordItem.rating, 4);
  assert.equal(b.recordItem, second);
  assert.equal(item.watched, false);
  assert.deepEqual(a.episodes, []);
});
