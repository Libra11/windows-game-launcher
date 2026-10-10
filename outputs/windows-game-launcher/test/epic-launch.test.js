import test from 'node:test';
import assert from 'node:assert/strict';
import { launchState, installState, installationHint } from '../src/lib/launch-state.js';
import { achievementEmptyMessage } from '../src/lib/achievement-state.js';
import { detectionState } from '../src/lib/detection-state.js';

test('账号导入的 Epic 游戏无需本机安装状态即可交给客户端打开', () => {
  const game = { source:'epic', exePath:'', installation:{ state:'unknown' } };
  assert.equal(launchState(game).disabled, false);
  assert.equal(launchState(game).label, '在 Epic 中打开');
  assert.equal(installationHint(game), '安装状态由 Epic 客户端管理');
  assert.equal(installState(game).label,'安装游戏');
  assert.equal(installState(game).disabled,false);
  assert.match(achievementEmptyMessage(game, []), /更新资料/);
  game.runtime = { state:'starting' };
  assert.equal(launchState(game).disabled, true);
  assert.equal(installState(game).disabled,true);
  game.runtime = { state:'running' };
  assert.equal(launchState(game).disabled, true);
  assert.equal(installState(game).disabled,true);
});

test('Epic 暂无成就与同步失败分开展示，缓存筛选保留原有提示', () => {
  const game = { source:'epic', schemaSource:'Epic 官方成就（暂无成就）', achievementPlatform:{} };
  assert.match(achievementEmptyMessage(game, []), /暂无 Epic 成就/);
  assert.equal(detectionState(game).label, '暂无 Epic 成就');
  game.achievementPlatform.definitionError = 'Epic 授权已失效，请重新登录';
  assert.match(achievementEmptyMessage(game, []), /授权已失效/);
  assert.equal(detectionState(game).state, 'error');
  assert.equal(achievementEmptyMessage(game, [{}]), '这个分类下暂时没有成就。');
  game.achievementPlatform = {}; game.scanStatus = 'Epic 成就已同步'; game.schemaSource = 'Epic 官方成就';
  assert.equal(detectionState(game).state, 'ready');
});
