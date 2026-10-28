import test from 'node:test';
import assert from 'node:assert/strict';
import { createPresentationLoader, presentationKey } from '../src/lib/media-presentation.js';
import { createMediaPreview } from '../src/lib/media-preview.js';

const movie = {
  id: 'local',
  externalIds: [
    { provider: 'douban', providerType: 'subject', externalId: '123' },
    { provider: 'tmdb', providerType: 'movie', externalId: '157336' },
  ],
};
test('相同TMDB作品的并发详情只请求一次，已加载资料跨本地条目复用', async () => {
  let calls = 0,
    resolve;
  const loading = new Promise((done) => (resolve = done));
  const loader = createPresentationLoader({
    run: async () => {
      calls++;
      await loading;
      return { logoUrl: 'logo', people: [{ id: 1, name: '演员' }] };
    },
  });
  const first = loader.load(movie);
  await loader.load({ ...movie, id: 'alias' });
  assert.equal(calls, 1);
  resolve();
  await first;
  await loader.load(movie);
  assert.equal(calls, 1);
  assert.equal(loader.get({ ...movie, id: 'alias' }).people.length, 1);
});
test('刷新失败保留已加载艺术字和演职员，允许显式重试', async () => {
  let fail = false;
  const loader = createPresentationLoader({
    run: async () => {
      if (fail) throw Error('离线');
      return { logoUrl: 'cached', people: [{ id: 1 }] };
    },
  });
  await loader.load(movie);
  fail = true;
  await loader.load(movie, true);
  assert.equal(loader.get(movie).status, 'error');
  assert.equal(loader.get(movie).logoUrl, 'cached');
  assert.equal(loader.get(movie).people.length, 1);
  fail = false;
  await loader.load(movie, true);
  assert.equal(loader.get(movie).status, 'ready');
});
test('没有TMDB身份的条目不请求，电影和电视剧相同编号不会串用缓存', async () => {
  let calls = 0;
  const loader = createPresentationLoader({
    run: async () => {
      calls++;
      return {};
    },
  });
  await loader.load({ id: 'raw' });
  assert.equal(calls, 0);
  assert.equal(presentationKey({ id: 'raw' }), '');
  assert.notEqual(
    presentationKey(movie),
    presentationKey({
      ...movie,
      externalIds: [{ provider: 'tmdb', providerType: 'tv', externalId: '157336' }],
    })
  );
});
test('公开示例提供真实艺术字与演员、主创，未知作品安全返回空资料', async () => {
  const command = createMediaPreview();
  const data = await command('media_presentation', { itemId: 'preview-slow-horses' });
  assert.match(data.logoUrl, /^https:\/\/image\.tmdb\.org\/t\/p\/w500\/.*\.png$/);
  assert.ok(data.people.some((person) => person.department === 'cast' && person.role));
  assert.ok(data.people.some((person) => person.department === 'crew'));
  assert.ok(data.people.length <= 16);
  assert.deepEqual(await command('media_presentation', { itemId: 'preview-unidentified' }), {
    logoUrl: '',
    people: [],
  });
});
