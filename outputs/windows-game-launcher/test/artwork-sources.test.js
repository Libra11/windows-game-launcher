import test from 'node:test';
import assert from 'node:assert/strict';
import { artworkSources } from '../src/lib/artwork-sources.js';

test('卡片使用接口竖版资源，背景使用横幅，高分辨率失败后可回退', () => {
  const info = { libraryCovers:['portrait-2x.jpg', 'portrait.jpg'], libraryHeroes:['hero-2x.jpg', 'hero.jpg'], cover:'header.jpg', icon:'icon.jpg' };
  assert.deepEqual(artworkSources(info), ['portrait-2x.jpg', 'portrait.jpg', 'header.jpg', 'icon.jpg']);
  assert.deepEqual(artworkSources(info, true), ['hero-2x.jpg', 'hero.jpg', 'header.jpg', 'icon.jpg']);
});

test('已有商店横幅及 Epic 封面仍可显示，缺失资料不猜 AppID 路径', () => {
  assert.deepEqual(artworkSources({cover:'epic.jpg'}), ['epic.jpg']);
  assert.deepEqual(artworkSources({appid:'4162040'}), []);
  assert.deepEqual(artworkSources({libraryCovers:null, libraryHeroes:'invalid', cover:''}), []);
  assert.deepEqual(artworkSources({libraryCovers:['same.jpg', null, ''], cover:'same.jpg'}), ['same.jpg']);
});
