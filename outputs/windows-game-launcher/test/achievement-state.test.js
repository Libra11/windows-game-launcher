import test from 'node:test';
import assert from 'node:assert/strict';
import { hasNoSteamAchievements, achievementEmptyMessage } from '../src/lib/achievement-state.js';
import { epicUnlockError } from '../src/lib/achievement-state.js';
import { detectionState } from '../src/lib/detection-state.js';

test('Epic 未提供商店资料与身份不一致、零成就分别展示', () => {
  const game = {source:'epic',achievementPlatform:{definitionError:'Epic 当前未提供此游戏的商店成就资料，无法获取官方成就列表'}};
  assert.equal(detectionState(game).state, 'unsupported');
  assert.doesNotMatch(detectionState(game).label, /同步失败|暂无 Epic 成就/);
  assert.match(detectionState(game).detail, /无法确认成就总数/);
  assert.equal(detectionState({...game,achievementPlatform:{definitionError:'Epic 返回的成就游戏身份不一致'}}).state, 'error');
});

test('Epic 成就列表已获取但账号记录不可用时，不误报定义获取失败', () => {
  const game = { source:'epic', schemaSource:'Epic 官方成就', metadataJson:JSON.stringify({epicAchievementSyncError:'账号记录暂不可用'}) };
  assert.equal(epicUnlockError(game), '账号记录暂不可用');
  assert.match(detectionState(game).label, /列表已获取.*待确认/);
  assert.match(detectionState(game).detail, /已有解锁记录保留/);
  assert.equal(detectionState(game).state, 'pending');
  assert.equal(epicUnlockError({...game, source:'steam'}), '');
  assert.equal(epicUnlockError({...game, metadataJson:'{}'}), '');
});

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
