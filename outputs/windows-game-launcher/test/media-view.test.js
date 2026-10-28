import test from 'node:test';
import assert from 'node:assert/strict';
import { createMediaViews, mediaViewSignature, mediaDetailScope, reconcileMediaChildren } from '../src/lib/media-view.js';
import { queryMedia } from '../src/lib/media-query.js';

test('不变区域不再次构建，播放推进不替换图片，身份和封面变化会更新', () => {
  const views = createMediaViews();
  let builds = 0;
  const build = () => ({ isConnected: true, number: ++builds });
  const item = { id: 'a', posterUrl: 'old', positionSeconds: 10, updatedAt: '1' };
  const first = views.use('card:a', item, build);
  assert.equal(views.use('card:a', { ...item, positionSeconds: 20, updatedAt: '2' }, build), first);
  views.prune();
  assert.equal(views.use('card:a', item, build), first);
  assert.notEqual(views.use('card:a', { ...item, posterUrl: 'new' }, build), first);
  assert.equal(builds, 2);
  assert.notEqual(mediaViewSignature(item), mediaViewSignature({ ...item, positionSeconds: 0 }));
  assert.notEqual(mediaViewSignature(item), mediaViewSignature({ ...item, favorite: true }));
});

test('离开页面后释放未挂载区域，允许空的附加资料与显式销毁', () => {
  const views = createMediaViews();
  const old = views.use('card', {}, () => ({ isConnected: false }));
  assert.equal(views.use('credits', {}, () => null), null);
  views.prune();
  const next = views.use('card', {}, () => ({ isConnected: true }));
  assert.notEqual(next, old);
  views.clear();
  assert.notEqual(views.use('card', {}, () => ({})), next);
});

test('详情范围只含当前作品与完整季集，乱序和异常环不会波及其他作品', () => {
  const items = [{ id: 'episode', parentId: 'season' }, { id: 'other' },
    { id: 'season', parentId: 'show' }, { id: 'show', parentId: 'episode' }];
  assert.deepEqual(mediaDetailScope(items, 'show').map(item => item.id), ['episode', 'season', 'show']);
  assert.deepEqual(mediaDetailScope(items, 'other'), [items[1]]);
});

test('侧栏计数省略排序后仍与所有分类的正式查询一致', () => {
  const items = [{ id: 'show', kind: 'series', recognized: true },
    { id: 's2', parentId: 'show', kind: 'season', title: '第二季', seasonNumber: 2, watched: true },
    { id: 's1', parentId: 'show', kind: 'season', title: '第一季', seasonNumber: 1, favorite: true },
    { id: 'movie', kind: 'movie', title: '电影', lastPlayed: 'today' }];
  for (const category of ['all', 'movies', 'series', 'other', 'watched', 'unwatched', 'favorites', 'recent', 'added'])
    assert.deepEqual(new Set(queryMedia(items, { category, sort: 'none' }).map(item => item.id)),
      new Set(queryMedia(items, { category }).map(item => item.id)));
});

test('局部协调不触碰未变化节点，重排只移动必需节点并清理删除项', () => {
  const parent = { children: [], inserts: 0, get firstChild() { return this.children[0] || null; },
    insertBefore(node, before) {
      this.inserts++;
      node.remove();
      const index = before ? this.children.indexOf(before) : this.children.length;
      this.children.splice(index, 0, node);
    } };
  const node = () => ({
    get nextSibling() { return parent.children[parent.children.indexOf(this) + 1] || null; },
    remove() { const index = parent.children.indexOf(this); if (index >= 0) parent.children.splice(index, 1); },
  });
  const a = node(), b = node(), c = node();
  reconcileMediaChildren(parent, [a, b, c]);
  parent.inserts = 0;
  reconcileMediaChildren(parent, [a, b, c]);
  assert.equal(parent.inserts, 0);
  reconcileMediaChildren(parent, [c, a]);
  assert.deepEqual(parent.children, [c, a]);
  assert.equal(parent.inserts, 1);
});
