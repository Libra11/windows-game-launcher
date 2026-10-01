import test from 'node:test';
import assert from 'node:assert/strict';
import { queryGames, inCategory, playTime } from '../src/lib/library-query.js';

const games = [
  {id:'a',title:'A',source:'steam',favorite:false,lastPlayed:'2026-09-28T00:00:00Z'},
  {id:'b',title:'B',source:'local',favorite:true,lastPlayed:''},
  {id:'c',title:'C',source:'local',favorite:false,lastPlayed:'2026-09-30T00:00:00Z'},
];
const ids = options => queryGames(games,options).map(game=>game.id);
test('收藏置顶，最近游玩按真实记录排序，并排除未游玩游戏',()=> {
  assert.deepEqual(ids({}),['b','a','c']);
  assert.deepEqual(ids({category:'recent'}),['c','a']);
  assert.deepEqual(ids({category:'favorites'}),['b']);
  assert.deepEqual(ids({sort:'recent'}),['c','a','b']);
});
test('大屏平台分类可以与收藏、最近游玩组合，搜索不改变原始数据',()=> {
  assert.deepEqual(ids({category:'local',collection:'recent'}),['c']);
  assert.deepEqual(ids({category:'steam',collection:'favorites'}),[]);
  assert.deepEqual(ids({search:'  c  '}),['c']);
  assert.deepEqual(games.map(game=>game.id),['a','b','c']);
  assert.equal(inCategory(games[1],'recent'),false);
});
test('游玩时长不把未知或不足一分钟显示为零小时',()=> {
  assert.equal(playTime(0),'暂无游玩记录');
  assert.equal(playTime(30),'不足 1 分钟');
  assert.equal(playTime(125),'2 分钟');
  assert.equal(playTime(3660),'1 小时 1 分钟');
});
