import test from 'node:test';
import assert from 'node:assert/strict';
import { isFamilyGame } from '../src/lib/family-library.js';
import { gameMetadata } from '../src/lib/game-metadata.js';
import { inCategory, queryGames } from '../src/lib/library-query.js';
import { launchState, installationHint } from '../src/lib/launch-state.js';

function game(id,shared,available=true) {
  return {id,title:id,source:'steam',installation:{state:'installed'},metadataJson:JSON.stringify({steamFamily:{shared,available}})};
}

test('家庭分类只显示共享条目，保留 Steam 总分类与 AppID 唯一条目',()=>{
  const games=[game('shared',true),game('owned',false),{...game('epic',true),source:'epic'}];
  assert.deepEqual(queryGames(games,{category:'steam-family'}).map(game=>game.id),['shared']);
  assert.equal(inCategory(games[0],'steam'),true);
  assert.equal(isFamilyGame(games[2]),false);
});

test('共享失效时即使已安装也禁用启动，运行中的游戏仍显示运行状态',()=>{
  const shared=game('shared',true,false);
  assert.equal(launchState(shared).disabled,true);
  assert.equal(launchState(shared).label,'共享已失效');
  assert.match(installationHint(shared),/家庭共享已不可用/);
  shared.runtime={state:'running'};
  assert.equal(launchState(shared).label,'游戏运行中');
  const owned=game('owned',false,false);
  assert.equal(launchState(owned).disabled,false);
});

test('复用解析后的资料，更新同一游戏资料后重新读取家庭资格',()=>{
  const shared=game('shared',true);
  assert.equal(gameMetadata(shared),gameMetadata(shared));
  assert.equal(isFamilyGame(shared),true);
  shared.metadataJson=JSON.stringify({steamFamily:{shared:false,available:true}});
  assert.equal(isFamilyGame(shared),false);
});
