import test from 'node:test';
import assert from 'node:assert/strict';
import { launchState, installationHint } from '../src/lib/launch-state.js';

const steam = state => ({source:'steam',installation:{state,reason:'检测说明'},runtime:{state:'idle'}});
test('Steam 只有已安装才能启动，其余安装状态均禁用且文案准确',()=> {
  for (const [state,label] of [['checking','检查安装中'],['not_installed','未安装'],['client_missing','Steam 未安装'],['unknown','无法确认安装']]) {
    assert.deepEqual(launchState(steam(state)),{label,disabled:true,reason:'检测说明'});
  }
  assert.equal(launchState(steam('installed')).disabled,false);
  assert.equal(launchState(steam('installed')).label,'开始游戏');
  assert.equal(installationHint(steam('installed')),'');
  assert.equal(installationHint(steam('unknown')),'无法确认安装 · 检测说明');
});
test('启动中与运行中优先于安装状态，并继续禁止重复启动',()=> {
  for (const [state,label] of [['starting','正在启动…'],['running','游戏运行中']]) {
    const game = {...steam('not_installed'),runtime:{state,message:'进程已确认'}};
    assert.deepEqual(launchState(game),{label,disabled:true,reason:'进程已确认'});
    assert.equal(installationHint(game),'');
  }
});
test('安装和卸载后立即改变启动可用性，缺失状态不会默认可启动',()=> {
  const game = steam('not_installed');
  assert.equal(launchState(game).disabled,true);
  game.installation.state = 'installed';
  assert.equal(launchState(game).disabled,false);
  game.installation.state = 'not_installed';
  assert.equal(launchState(game).disabled,true);
  assert.equal(launchState({source:'steam'}).label,'检查安装中');
  assert.equal(launchState(steam('invalid')).disabled,true);
});
test('Steam 安装检测不阻止本地可执行文件启动',()=> {
  assert.equal(launchState({source:'local',installation:{state:'client_missing'}}).disabled,false);
  assert.equal(installationHint({source:'local'}),'');
});
