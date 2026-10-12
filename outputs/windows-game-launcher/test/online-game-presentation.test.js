import test from 'node:test';
import assert from 'node:assert/strict';
import { onlineGamePresentation, onlineGameType } from '../src/lib/online-game-presentation.js';

test('原站名称、版本和说明分开展示，保留完整说明，不推断缺失标签',()=>{
  const value=onlineGamePresentation('尘埃异变 GRIME|官方中文|V1.3.2-堕落之剑+全DLC|支持手柄|');
  assert.equal(value.name,'尘埃异变 GRIME');assert.match(value.edition,/V1\.3\.2-堕落之剑/);
  assert.deepEqual(value.badges,['中文','含 DLC','支持手柄']);assert.match(value.edition,/堕落之剑/);
  assert.deepEqual(onlineGamePresentation('纯名称').badges,[]);
  assert.equal(onlineGamePresentation('游戏|Build.23315732-天降神谕').edition,'Build.23315732-天降神谕');
  assert.deepEqual(onlineGamePresentation('游戏|无中文|无DLC|不支持手柄').badges,[]);
});

test('资源类型遵循原站分类，Switch 与未知资源不被标成 PC',()=>{
  assert.equal(onlineGameType({isPc:true}),'PC 游戏');
  assert.equal(onlineGameType({categories:['Switch游戏下载']}),'Switch');
  assert.equal(onlineGameType({categories:['修改器']}),'修改器');
  assert.equal(onlineGameType({}),'游戏资源');
});
