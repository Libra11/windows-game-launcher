import test from 'node:test';
import assert from 'node:assert/strict';
import { displayMediaPath } from '../src/lib/media-path.js';

test('仅隐藏 Windows 扩展路径前缀，保留盘符、网络共享与中文目录', () => {
  assert.equal(displayMediaPath(String.raw`\\?\L:\youji`), String.raw`L:\youji`);
  assert.equal(
    displayMediaPath(String.raw`\\?\UNC\NAS\影视\电视剧`),
    String.raw`\\NAS\影视\电视剧`
  );
  assert.equal(displayMediaPath(String.raw`\\?\unc\NAS\影视`), String.raw`\\NAS\影视`);
  for (const path of [
    String.raw`L:\youji`,
    String.raw`\\NAS\影视`,
    'L:/影视/电影',
    String.raw`\\?\Volume{123}\影视`,
  ]) {
    assert.equal(displayMediaPath(path), path);
  }
  assert.equal(displayMediaPath(null), '');
});
