import test from 'node:test';
import assert from 'node:assert/strict';
import { importSummary, importGamePlan } from '../src/ui/cloud-import-summary.js';

test('按业务类别汇总，而非把同步记录数当作游戏数',()=>{
  const rows=importSummary({resourceCount:2623,resourceCounts:{game:261,automatic_unlock:20,manual_unlock:3,collection:15,collection_member:40,tag_member:9,definition_cache:100,play_session:44}});
  assert.deepEqual(rows.find(r=>r[0]==='游戏'),['游戏',261,'款']);
  assert.deepEqual(rows.find(r=>r[0]==='成就记录'),['成就记录',23,'条']);
  assert.deepEqual(rows.find(r=>r[0]==='分类归属'),['分类归属',49,'条']);
  assert.deepEqual(rows.find(r=>r[0]==='成就资料'),['成就资料',100,'款游戏']);
});
test('只提示未关联且有兼容候选的本地游戏，不把新分配的云端编号当作已有关联',()=>{
  const game={id:'local',source:'local',namespace:'steam:42'};
  const value={mappings:[{localId:'local',cloudId:'new-id'}],cloudGames:[{id:'existing',source:'local',namespace:'steam:42'},{id:'wrong',source:'local',namespace:'steam:99'}]};
  assert.equal(importGamePlan(value,game).needsChoice,true);
  assert.equal(importGamePlan(value,game).candidates.length,1);
  assert.equal(importGamePlan(value,{...game,source:'steam'}).needsChoice,false);
  assert.equal(importGamePlan({...value,cloudGames:[]},game).needsChoice,false);
  assert.equal(importGamePlan({...value,mappings:[{localId:'local',cloudId:'existing'}]},game).needsChoice,false);
});
