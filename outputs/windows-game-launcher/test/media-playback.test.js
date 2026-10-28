import test from 'node:test';
import assert from 'node:assert/strict';
import { applyMediaPlayback } from '../src/lib/media-playback.js';
import { detailModel } from '../src/lib/media-detail-model.js';
import { queryMedia } from '../src/lib/media-query.js';

test('结束的旧播放状态不覆盖后来扫描重置的资源进度', () => {
  const item = { id: 'm', kind: 'movie', positionSeconds: 0, durationSeconds: 0 };
  const state = {
    data: { items: [item] },
    detail: { item, resources: [{ id: 'r', positionSeconds: 0 }] },
  };
  applyMediaPlayback(state, {
    active: false,
    state: 'idle',
    itemId: 'm',
    resourceId: 'r',
    positionSeconds: 45,
    durationSeconds: 90,
    sampleAt: 5,
  });
  applyMediaPlayback(state, state.playerStatus);
  assert.equal(item.positionSeconds, 0);
  assert.equal(state.detail.resources[0].positionSeconds, 0);
});

test('播放器切集更新资源与整剧续播，不改变正在浏览的季集和个人记录', () => {
  const root = { id: 's', kind: 'series' },
    season = { id: 'season', kind: 'season', parentId: 's' },
    first = {
      id: 'e1',
      kind: 'episode',
      parentId: 'season',
      title: '第一集',
      seasonNumber: 1,
      episodeNumber: 1,
      watched: true,
    },
    second = {
      id: 'e2',
      kind: 'episode',
      parentId: 'season',
      title: '第二集',
      seasonNumber: 1,
      episodeNumber: 2,
      watched: false,
    };
  const state = {
    data: { items: [root, season, first, second] },
    detail: {
      item: { ...root },
      resources: [{ id: 'r2', positionSeconds: 0, durationSeconds: 120 }],
    },
    detailEpisodeId: 'e1',
    detailResourceId: 'r1',
  };
  assert.equal(
    applyMediaPlayback(state, {
      active: true,
      state: 'playing',
      itemId: 'e2',
      resourceId: 'r2',
      positionSeconds: 8.125,
      durationSeconds: 120,
      sampleAt: 5,
    }),
    true
  );
  assert.equal(root.positionSeconds, 8.125);
  assert.equal(root.resumeResourceId, 'r2');
  assert.equal(root.currentEpisodeTitle, 'S01E02 · 第二集');
  assert.equal(state.detail.resources[0].positionSeconds, 8.125);
  assert.equal(state.detailEpisodeId, 'e1');
  assert.equal(state.detailResourceId, 'r1');
  assert.equal(second.watched, false);
  assert.equal(first.watched, true);
  assert.equal(
    applyMediaPlayback(state, {
      active: true,
      itemId: 'e1',
      resourceId: 'r1',
      positionSeconds: 50,
      sampleAt: 4,
    }),
    false
  );
  assert.equal(root.resumeResourceId, 'r2');
});
test('未识别按识别状态收集电影、剧集和子条目', () => {
  const items = [
    { id: 's', kind: 'series', recognized: true },
    { id: 'e', kind: 'episode', parentId: 's', recognized: false },
    { id: 'm', kind: 'movie', recognized: false },
    { id: 'o', kind: 'other', recognized: true },
  ];
  assert.deepEqual(
    queryMedia(items, { category: 'other' }).map((v) => v.id),
    ['e', 'm']
  );
});
test('播放版本可显式选择，不继承另一版本的进度，离线选择不覆盖可用目标', () => {
  const detail = {
    item: { id: 'm', kind: 'movie', resumeResourceId: 'a' },
    resources: [
      { id: 'a', available: true, positionSeconds: 60 },
      { id: 'b', available: true, positionSeconds: 0 },
    ],
  };
  assert.equal(detailModel(detail, [], { resourceId: 'b' }).resource.positionSeconds, 0);
  detail.resources[1].available = false;
  assert.equal(detailModel(detail, [], { resourceId: 'b' }).resource.id, 'a');
});
