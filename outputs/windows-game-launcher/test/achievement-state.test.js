import test from 'node:test';
import assert from 'node:assert/strict';
import { hasNoSteamAchievements, achievementEmptyMessage } from '../src/lib/achievement-state.js';

test('Steam 已确认暂无成就与获取失败、尚未同步分开提示', () => {
  const game = {source:'steam', schemaSource:'Steam Web API（暂无成就）'};
  assert.equal(hasNoSteamAchievements(game, []), true);
  assert.match(achievementEmptyMessage(game, []), /此游戏暂无 Steam 成就/);
  assert.doesNotMatch(achievementEmptyMessage(game, []), /API Key|更新资料/);
  assert.match(achievementEmptyMessage({source:'steam', scanStatus:'同步失败：网络请求失败'}, []), /记录状态/);
  assert.match(achievementEmptyMessage({source:'steam'}, []), /尚未获取/);
});

test('已有缓存和本地成就不会被暂无 Steam 成就状态隐藏', () => {
  const game = {source:'steam', schemaSource:'Steam Web API（暂无成就）'};
  assert.equal(hasNoSteamAchievements(game, [{apiName:'FIRST'}]), false);
  assert.match(achievementEmptyMessage(game, [{apiName:'FIRST'}]), /这个分类/);
  assert.equal(hasNoSteamAchievements({...game, source:'local'}, []), false);
});
