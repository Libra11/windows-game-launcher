import test from 'node:test';
import assert from 'node:assert/strict';
import { createLibraryViewCache } from '../src/lib/library-view-cache.js';

function state() {
  return {games:[
    {id:'a',title:'A',source:'local',playedSeconds:100,playtime:{seconds:100,state:'ready'}},
    {id:'b',title:'B',source:'local',playedSeconds:200,playtime:{seconds:200,state:'ready'}},
  ],filter:'all',search:'',sort:'az',installedOnly:false,view:'grid',bigCategory:'all',bigCollection:'all'};
}

test('返回未变化列表和仅计时更新时保留整个页面，不重新构建卡片',()=>{
  const cache=createLibraryViewCache(),current=state();let builds=0;
  const build=()=>({version:++builds});
  const first=cache('desktop',current,build);
  current.games=structuredClone(current.games);
  current.games[0].runtime={state:'running',elapsedSeconds:20};
  current.games[0].playedSeconds=120;
  current.games[0].playtime.seconds=120;
  current.games[0].lastScan='2026-10-10T12:00:00Z';
  assert.equal(cache('desktop',current,build),first);
  const big=cache('big-screen',current,build);
  assert.notEqual(big,first);
  assert.equal(cache('desktop',current,build),first);
  assert.equal(builds,2);
});

test('筛选与封面资料变化时重建列表，避免保留旧数据',()=>{
  const cache=createLibraryViewCache(),current=state();const build=()=>({});
  const first=cache('desktop',current,build);
  current.filter='local';
  const filtered=cache('desktop',current,build);assert.notEqual(filtered,first);
  current.games[0].metadataJson='{"localArtwork":{"portrait":"new.jpg"}}';
  assert.notEqual(cache('desktop',current,build),filtered);
});

test('时长变化未改变顺序时复用列表，跨过另一款游戏后重新排序',()=>{
  const cache=createLibraryViewCache(),current=state();current.sort='time';const build=()=>({});
  const first=cache('desktop',current,build);
  current.games[0].playtime.seconds=150;
  assert.equal(cache('desktop',current,build),first);
  current.games[0].playtime.seconds=300;
  assert.notEqual(cache('desktop',current,build),first);
});
