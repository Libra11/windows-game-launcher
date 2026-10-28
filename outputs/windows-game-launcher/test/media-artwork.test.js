import test from 'node:test';
import assert from 'node:assert/strict';
import { localArtworkKey, mediaArtworkSource, mediaPosterOwner } from '../src/lib/media-artwork-model.js';
import { loadMediaArtwork, retryMediaArtwork } from '../src/lib/media-artwork.js';

test('豆瓣预览携带令牌读取本地封面，更新预览不会复用旧请求', async () => {
  const previous = { window: globalThis.window, isTauri: globalThis.isTauri };
  const requests = [];
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: {
    invoke: async (command, args) => { requests.push([command, args]); return { path: `C:/cache/${args.input.previewToken}.webp` }; },
    convertFileSrc: path => 'asset:' + path,
  } };
  try {
    const base = { id: 'douban-preview', posterUrl: 'https://img3.doubanio.com/poster.webp' };
    const images = ['first', 'second'].map(artworkPreviewToken => {
      const image = { isConnected: true, style: {}, src: '', onerror: () => assert.fail('缓存应可用') };
      loadMediaArtwork(image, { ...base, artworkPreviewToken });
      image.onerror(); // 原站不带 Referer 返回 418，缓存完成前保留节点。
      return image;
    });
    await new Promise(setImmediate);
    assert.deepEqual(requests.map(([, args]) => args.input.previewToken), ['first', 'second']);
    assert.ok(requests.every(([command, args]) => command === 'media_artwork' && args.input.itemId === base.id));
    assert.equal(images[0].src, 'asset:C:/cache/first.webp');
    assert.equal(images[1].src, 'asset:C:/cache/second.webp');
  } finally {
    globalThis.window = previous.window;
    globalThis.isTauri = previous.isTauri;
  }
});

test('截图只用于未识别资源的封面和背景，不替代艺术字或人员照片', () => {
  const item = { recognized: false, localArtworkKey: 'version-a' };
  assert.equal(localArtworkKey(item, 'poster'), 'version-a');
  assert.equal(localArtworkKey(item, 'backdrop'), 'version-a');
  assert.equal(localArtworkKey(item, 'logo'), '');
  assert.equal(localArtworkKey(item, 'profile'), '');
  assert.equal(localArtworkKey({ ...item, recognized: true }), '');
  assert.equal(localArtworkKey({ recognized: false, resourceCount: 1 }), '');
});

test('正式图片保留优先级，文件版本更新切换截图请求身份', () => {
  const item = { recognized: false, localArtworkKey: 'version-a', posterUrl: 'https://image.tmdb.org/poster.jpg', backdropUrl: 'https://image.tmdb.org/backdrop.jpg' };
  assert.equal(mediaArtworkSource(item), item.posterUrl);
  assert.equal(mediaArtworkSource(item, 'backdrop'), item.backdropUrl);
  assert.notEqual(localArtworkKey(item), localArtworkKey({ ...item, localArtworkKey: 'version-b' }));
  assert.equal(mediaArtworkSource({ cover: 'https://example.com/cover.jpg' }), 'https://example.com/cover.jpg');
});

test('季没有海报时使用所属整剧，季海报和无关作品保持原优先级', () => {
  const series = { id: 'show', kind: 'series', recognized: true, posterUrl: 'https://image.tmdb.org/show.jpg' };
  const season = { id: 'season', kind: 'season', parentId: 'show', recognized: true, posterUrl: '' };
  const byId = new Map([[series.id, series]]);
  assert.equal(mediaPosterOwner(season, byId), series);
  const own = { ...season, posterUrl: 'https://image.tmdb.org/season.jpg' };
  assert.equal(mediaPosterOwner(own, byId), own);
  assert.equal(mediaPosterOwner(season, new Map()), season);
  assert.equal(mediaPosterOwner(season, new Map([['show', { ...series, kind: 'movie' }]])), season);
  assert.equal(mediaPosterOwner(season, new Map([['show', { ...series, posterUrl: '' }]])), season);
  const episode = { ...season, kind: 'episode' };
  assert.equal(mediaPosterOwner(episode, byId), episode);
});

test('回退整剧海报时缓存请求使用整剧身份', async () => {
  const previous = { window: globalThis.window, isTauri: globalThis.isTauri };
  const requests = [];
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: {
    invoke: async (command, args) => { requests.push([command, args]); return { path: 'C:/cache/show.jpg' }; },
    convertFileSrc: (path) => 'asset:' + path,
  } };
  try {
    const series = { id: 'fallback-series', kind: 'series', recognized: true, posterUrl: 'https://image.tmdb.org/show.jpg' };
    const season = { id: 'fallback-season', kind: 'season', parentId: series.id, recognized: true };
    const image = { isConnected: true, style: {}, src: '', onerror: () => assert.fail('整剧海报不应触发失败') };
    loadMediaArtwork(image, mediaPosterOwner(season, new Map([[series.id, series]])));
    await new Promise(setImmediate);
    assert.equal(requests[0][1].input.itemId, series.id);
    assert.equal(image.src, 'asset:C:/cache/show.jpg');
  } finally {
    globalThis.window = previous.window;
    globalThis.isTauri = previous.isTauri;
  }
});

test('没有远程图片地址时仍异步请求本地截图，加载失败保留占位内容', async () => {
  const previous = { window: globalThis.window, isTauri: globalThis.isTauri };
  const requests = [];
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: {
    invoke: async (command, args) => {
      requests.push([command, args]);
      return { path: args.input.itemId === 'frame-success' ? 'C:/cache/frame.jpg' : null };
    },
    convertFileSrc: (path) => 'asset:' + path,
  } };
  try {
    const image = { isConnected: true, style: {}, src: '', onerror: () => assert.fail('可用截图不应触发失败') };
    loadMediaArtwork(image, { id: 'frame-success', recognized: false, localArtworkKey: 'version-a' });
    await new Promise(setImmediate);
    assert.equal(requests[0][0], 'media_artwork');
    assert.equal(image.src, 'asset:C:/cache/frame.jpg');
    let failed = 0;
    const empty = { isConnected: true, style: {}, src: '', onerror: () => failed++ };
    loadMediaArtwork(empty, { id: 'frame-failure', recognized: false, localArtworkKey: 'version-b' });
    await new Promise(setImmediate);
    assert.equal(empty.src, '');
    assert.equal(failed, 1);
  } finally {
    globalThis.window = previous.window;
    globalThis.isTauri = previous.isTauri;
  }
});


test('封面与详情共用截图请求，缓存命中不入抽帧队列', async () => {
  const previous = { window: globalThis.window, isTauri: globalThis.isTauri };
  const requests = [];
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: {
    invoke: async (command, args) => { requests.push(args.input); return { path: 'C:/cache/shared.jpg' }; },
    convertFileSrc: path => 'asset:' + path,
  } };
  try {
    const item = { id: 'shared-frame', recognized: false, localArtworkKey: 'same' };
    const images = Array.from({ length: 2 }, () => ({ isConnected: true, src: '', style: {}, onerror() {} }));
    loadMediaArtwork(images[0], item, 'poster');
    loadMediaArtwork(images[1], item, 'backdrop');
    await new Promise(setImmediate);
    assert.equal(requests.length, 1);
    assert.equal(requests[0].cacheOnly, true);
    assert.equal(images[0].src, images[1].src);
  } finally { globalThis.window = previous.window; globalThis.isTauri = previous.isTauri; }
});

test('未生成截图和失效缓存可在原节点重试恢复', async () => {
  const previous = { window: globalThis.window, isTauri: globalThis.isTauri, now: Date.now };
  let available = false, failed = 0, now = Date.now();
  Date.now = () => now;
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: {
    invoke: async () => ({ path: available ? 'C:/cache/recovered.jpg' : null }),
    convertFileSrc: path => 'asset:' + path,
  } };
  try {
    const image = { isConnected: true, src: '', style: {}, onerror() { failed++; }, removeAttribute() { this.src = ''; } };
    loadMediaArtwork(image, { id: 'retry-frame', recognized: false, localArtworkKey: 'same' });
    await new Promise(setImmediate);
    assert.equal(failed, 1);
    available = true; now += 61000;
    retryMediaArtwork(image);
    await new Promise(setImmediate);
    assert.equal(image.src, 'asset:C:/cache/recovered.jpg');
    image.onerror();
    assert.equal(image.src, '');
    now += 61000; retryMediaArtwork(image);
    await new Promise(setImmediate);
    assert.equal(image.src, 'asset:C:/cache/recovered.jpg');
  } finally { globalThis.window = previous.window; globalThis.isTauri = previous.isTauri; Date.now = previous.now; }
});
