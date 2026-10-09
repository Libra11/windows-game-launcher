import test from 'node:test';
import assert from 'node:assert/strict';
import { achievementImageSources } from '../src/lib/achievement-images.js';

test('Steam 旧成就地址改用社区页面的新 CDN 路径，保留同一游戏与图片标识', () => {
  const path = 'images/apps/499170/2de4fd2f5a0fa2ae280613043cda19884df2cd72.jpg';
  const old = `https://steamcdn-a.akamaihd.net/steamcommunity/public/${path}`;
  assert.deepEqual(achievementImageSources(old), [
    `https://shared.akamai.steamstatic.com/community_assets/${path}`, old,
  ]);
  assert.deepEqual(achievementImageSources(old.replace('https:', 'http:')), achievementImageSources(old));
});

test('当前 Steam、Epic 和其他 HTTPS 图标地址原样保留', () => {
  for (const url of [
    'https://shared.akamai.steamstatic.com/community_assets/images/apps/499170/abc.jpg',
    'https://cdn.example.com/epic/icon.png',
    'https://steamcdn-a.akamaihd.net/other/image.jpg',
    'https://steamcdn-a.akamaihd.net.example.com/steamcommunity/public/images/apps/1/abc.jpg',
  ]) assert.deepEqual(achievementImageSources(url), [url]);
});

test('空地址、路径和非 HTTPS 的其他来源不会作为网络图片加载', () => {
  for (const url of ['', null, 'D:/games/icon.png', 'javascript:alert(1)', 'http://example.com/icon.png', 'https://user:secret@example.com/icon.png']) {
    assert.deepEqual(achievementImageSources(url), []);
  }
});
