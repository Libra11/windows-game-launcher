import test from 'node:test';
import assert from 'node:assert/strict';
import { collectionDragPayload, collectionDrop, createDragClickGuard } from '../src/lib/collection-drag.js';
import { indexOrganization } from '../src/lib/library-organization.js';

function state() {
  return {
    games: [{ id:'a', title:'游戏 A' }, { id:'b', title:'游戏 B' }],
    organizationBatchMode:true,
    organizationSelection:new Set(['a','b','deleted']),
    organization:indexOrganization({
      tags:[], collections:[{ id:'one', name:'周末' }, { id:'two', name:'常玩' }],
      gameTags:[], gameCollections:[['a','one'],['a','two']],
    }),
  };
}

test('Esc 取消后继续按住超过 750ms，松开后的点击仍只拦截一次', t => {
  let now=0;
  t.mock.method(performance,'now',()=>now);
  const guard=createDragClickGuard();
  guard.begin();guard.suppress('pointer',7);
  now=5000;
  assert.equal(guard.matches('pointer',7),true);
  assert.equal(guard.consumeClick(1),true);
  assert.equal(guard.consumeClick(1),false);
});

test('取消标记只匹配原手势，键盘点击不消费标记，新按下正常点击', () => {
  const guard=createDragClickGuard();
  guard.suppress('pointer',7);
  assert.equal(guard.matches('pointer',8),false);
  assert.equal(guard.matches('touch',7),false);
  assert.equal(guard.consumeClick(0),false);
  assert.equal(guard.matches('pointer',7),true);
  guard.begin();
  assert.equal(guard.consumeClick(1),false);
});

test('触屏取消后保留原触点拦截，新触摸清除无合成点击的旧标记', () => {
  const guard=createDragClickGuard();
  guard.suppress('touch',23);
  assert.equal(guard.matches('touch',23),true);
  assert.equal(guard.matches('touch',24),false);
  guard.begin();
  assert.equal(guard.matches('touch',23),false);
  assert.equal(guard.consumeClick(1),false);
});

test('拖动已勾选卡片携带多选游戏，排除已移除游戏；非批量模式只携带单款', () => {
  const value=state();
  assert.deepEqual(collectionDragPayload(value,'a'), { gameIds:['a','b'], title:'2 款游戏' });
  value.organizationBatchMode=false;
  assert.deepEqual(collectionDragPayload(value,'a'), { gameIds:['a'], title:'游戏 A' });
  assert.equal(collectionDragPayload(value,'deleted'),null);
});

test('拖动未勾选卡片只加入该款，不修改既有勾选集合', () => {
  const value=state();value.organizationSelection=new Set(['a']);
  assert.deepEqual(collectionDragPayload(value,'b'), { gameIds:['b'], title:'游戏 B' });
  assert.deepEqual([...value.organizationSelection],['a']);
});

test('加入目标收藏夹只提交缺失归属，重复拖入为空操作，其他归属保持', () => {
  const value=state();
  const before=structuredClone(value.organization.gameCollections);
  assert.deepEqual(collectionDrop(value,['a','b','b'],'one').gameIds,['b']);
  assert.deepEqual(collectionDrop(value,['a'],'one').gameIds,[]);
  assert.deepEqual(value.organization.gameCollections,before);
  assert.deepEqual([...value.organization.byGameCollections.get('a')],['one','two']);
});

test('拖放结束时重新校验已删除的目标或游戏，拒绝空载荷', () => {
  const value=state();
  assert.throws(()=>collectionDrop(value,['a'],'deleted'),/收藏夹已不存在/);
  value.games=value.games.filter(game=>game.id!=='b');
  assert.throws(()=>collectionDrop(value,['a','b'],'one'),/游戏已变化/);
  assert.throws(()=>collectionDrop(value,[],'one'),/游戏已变化/);
});
