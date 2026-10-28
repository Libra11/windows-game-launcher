import test from 'node:test';
import assert from 'node:assert/strict';
import {parseEpisodeNumbers,suggestedEpisodes} from '../src/lib/media-season-match.js';
const episodes=[1,2,3,5].map(episodeNumber=>({episodeNumber}));
test('单季映射支持空白跳过、单集、逗号和合并集范围，拒绝缺集及重复',()=>{
  assert.deepEqual(parseEpisodeNumbers('',episodes),[]);
  assert.deepEqual(parseEpisodeNumbers('1-3，5',episodes),[1,2,3,5]);
  for(const value of ['1-5','3-1','2,2','0','1.5','a','1-1000000000'])assert.throws(()=>parseEpisodeNumbers(value,episodes));
});
test('偏移可以修正越界集号；冲突季号、附加片与离线文件不自动建议',()=>{
  assert.deepEqual(suggestedEpisodes({parsedEpisodes:[13],canSuggest:true,warning:'所选季没有该集'},episodes,-12),[1]);
  assert.deepEqual(suggestedEpisodes({parsedEpisodes:[2],canSuggest:false,warning:'季号冲突'},episodes,0),[]);
  assert.deepEqual(suggestedEpisodes({parsedEpisodes:[2],canSuggest:false,canOverrideSeason:true,warning:'季号冲突'},episodes,0,true),[2]);
  assert.deepEqual(suggestedEpisodes({parsedEpisodes:[2],canSuggest:true,disabled:true},episodes,0),[]);
  assert.deepEqual(suggestedEpisodes({parsedEpisodes:[2],canSuggest:true},episodes,0.5),[]);
});
