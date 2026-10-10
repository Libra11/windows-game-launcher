import test from 'node:test';
import assert from 'node:assert/strict';
import { gameMenuScope, gameMenuModel, gameManagementItems, gameCollectionItems } from '../src/lib/game-context-menu.js';
import { indexOrganization } from '../src/lib/library-organization.js';

function state() {
  return {
    games: [
      { id:'a', title:'游戏 A', source:'steam', installation:{state:'not_installed'} },
      { id:'b', title:'游戏 B', source:'local', favorite:true },
      { id:'c', title:'游戏 C', source:'epic' },
    ],
    collectionId:'one', organizationBatchMode:false, organizationSelection:new Set(),
    organization:indexOrganization({ tags:[], collections:[{id:'one',name:'周末'},{id:'two',name:'常玩'}],
      gameTags:[], gameCollections:[['a','one'],['b','two']] }),
  };
}
const menu = (value, id) => gameMenuModel(value, gameMenuScope(value, id));
const items = model => model.groups.flat();

test('已选卡片使用有效的多选范围，未选卡片只作用于自身，不改变选择', () => {
  const value=state(); value.organizationBatchMode=true; value.organizationSelection=new Set(['a','b','deleted']);
  const scope=gameMenuScope(value,'a');
  assert.deepEqual(scope.gameIds,['a','b']);
  assert.equal(scope.batch,true);
  assert.deepEqual(gameMenuScope(value,'c').gameIds,['c']);
  assert.deepEqual([...value.organizationSelection],['a','b','deleted']);
  const ids=items(menu(value,'a')).map(item=>item.id);
  assert.ok(ids.includes('add-collection')&&ids.includes('remove-tag'));
  assert.ok(!ids.includes('launch')&&!ids.includes('remove')&&!ids.includes('favorite'));
});

test('Steam 入口跟随安装及运行状态，菜单开启后重新建模不会沿用旧状态', () => {
  const value=state(); const scope=gameMenuScope(value,'a');
  assert.equal(items(gameMenuModel(value,scope))[0].label,'安装游戏');
  value.games[0].installation.state='installed';
  assert.equal(items(gameMenuModel(value,scope))[0].label,'开始游戏');
  value.games[0].runtime={state:'running'};
  assert.equal(items(gameMenuModel(value,scope))[0].label,'游戏运行中');
  assert.equal(items(gameMenuModel(value,scope))[0].disabled,true);
});

test('菜单遵守共享失效和安装请求等待限制', () => {
  const value=state(); value.games[0].installRequestPending=true;
  assert.equal(items(menu(value,'a'))[0].disabled,true);
  assert.equal(items(menu(value,'a'))[0].label,'正在打开安装…');
  value.games[0].installRequestPending=false;
  value.games[0].metadataJson=JSON.stringify({steamFamily:{shared:true,available:false}});
  const first=items(menu(value,'a'))[0];
  assert.equal(first.disabled,true);
  assert.equal(first.label,'共享已失效');
});

test('Epic 不推断安装状态，本地独有移除及记录维护入口', () => {
  const value=state();
  const epic=items(menu(value,'c'));
  assert.equal(epic[0].label,'在 Epic 中打开');
  assert.ok(epic.some(item=>item.id==='install'));
  assert.ok(!epic.some(item=>item.id==='remove'));
  const local=items(menu(value,'b'));
  assert.equal(local.find(item=>item.id==='favorite').label,'取消收藏');
  assert.equal(local.at(-1).id,'remove');
  assert.ok(gameManagementItems(value.games[1]).some(item=>item.id==='scan'));
  assert.ok(!gameManagementItems(value.games[2]).some(item=>item.id==='scan'));
});

test('收藏夹搜索与勾选来自当前归属，仅成员展示移出当前收藏夹入口', () => {
  const value=state(), scope=gameMenuScope(value,'a');
  const before=structuredClone(value.organization.gameCollections);
  assert.deepEqual(gameCollectionItems(value,scope,' 周末 ').map(item=>[item.collectionId,item.checked]),[['one',true]]);
  assert.equal(gameCollectionItems(value,scope,'不存在').length,0);
  assert.ok(items(menu(value,'a')).some(item=>item.id==='remove-current'));
  assert.ok(!items(menu(value,'b')).some(item=>item.id==='remove-current'));
  assert.deepEqual(value.organization.gameCollections,before);
});

test('固定操作范围中游戏已移除时拒绝继续使用菜单', () => {
  const value=state(); value.organizationBatchMode=true; value.organizationSelection=new Set(['a','b']);
  const scope=gameMenuScope(value,'a'); value.games=value.games.filter(game=>game.id!=='b');
  assert.equal(gameMenuModel(value,scope),null);
  assert.equal(gameMenuScope(value,'deleted'),null);
});
