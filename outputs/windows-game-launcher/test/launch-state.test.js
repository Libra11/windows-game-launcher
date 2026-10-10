import test from 'node:test';
import assert from 'node:assert/strict';
import { launchState, installState, installationHint } from '../src/lib/launch-state.js';

const steam = state => ({source:'steam',installation:{state,reason:'检测说明'},runtime:{state:'idle'}});
test('Steam 已安装可启动、未安装可安装，其余状态禁用',()=> {
  for (const [state,label] of [['checking','检查安装中'],['client_missing','Steam 未安装'],['unknown','无法确认安装']]) {
    assert.deepEqual(launchState(steam(state)),{label,disabled:true,reason:'检测说明'});
  }
  assert.equal(launchState(steam('not_installed')).label,'安装游戏');
  assert.equal(launchState(steam('not_installed')).disabled,false);
  assert.equal(launchState(steam('not_installed')).action,'install');
  assert.equal(installationHint(steam('not_installed')),'未安装 · 检测说明');
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
test('安装和卸载后立即切换操作，缺失状态不会默认可启动',()=> {
  const game = steam('not_installed');
  assert.equal(launchState(game).action,'install');
  game.installation.state = 'installed';
  assert.equal(launchState(game).disabled,false);
  assert.equal(launchState(game).action,undefined);
  game.installation.state = 'not_installed';
  assert.equal(launchState(game).action,'install');
  assert.equal(launchState({source:'steam'}).label,'检查安装中');
  assert.equal(launchState(steam('invalid')).disabled,true);
});
test('本地游戏缺失启动文件时不提供平台安装入口',()=> {
  const game={source:'local',installation:{state:'not_installed'}};
  assert.equal(launchState(game).disabled,true);
  assert.equal(launchState(game).label,'启动文件缺失');
  assert.equal(installState(game).disabled,true);
  assert.equal(installationHint({source:'local'}),'');
});

test('Steam 失效家庭共享不能安装，恢复资格后允许安装',()=>{
  const game={...steam('not_installed'),metadataJson:JSON.stringify({steamFamily:{shared:true,available:false}})};
  assert.equal(launchState(game).label,'共享已失效');
  assert.equal(installState(game).disabled,true);
  game.metadataJson=JSON.stringify({steamFamily:{shared:true,available:true}});
  assert.equal(launchState(game).action,'install');
});
