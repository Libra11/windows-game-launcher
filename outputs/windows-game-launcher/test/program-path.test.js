import test from 'node:test';
import assert from 'node:assert/strict';
import { createProgramPathResolver, programDropPoint } from '../src/lib/program-path.js';

test('按 Windows 缩放比例将文件落点转换为页面坐标', () => {
  assert.deepEqual(programDropPoint({x:600,y:300},1.5), {x:400,y:200});
  assert.deepEqual(programDropPoint({x:400,y:200},2), {x:200,y:100});
  assert.equal(programDropPoint(null,1),null);
  assert.equal(programDropPoint({x:1,y:2},0),null);
});

test('较早的快捷方式解析不会覆盖后拖入的程序', async () => {
  const tasks = new Map(), applied = [];
  const resolver = createProgramPathResolver({resolve:path=>new Promise(done=>tasks.set(path,done)),apply:path=>applied.push(path),failed:()=>assert.fail()});
  const first=resolver.select('first.lnk'); await Promise.resolve();
  const second=resolver.select('second.exe'); await Promise.resolve();
  tasks.get('second.exe')('D:/second.exe'); await second;
  tasks.get('first.lnk')('D:/first.exe'); await first; await resolver.ready();
  assert.deepEqual(applied,['D:/second.exe']);
});

test('解析期间手动输入或关闭弹窗会放弃过时结果', async () => {
  let complete;
  const resolver=createProgramPathResolver({resolve:()=>new Promise(done=>{complete=done;}),apply:()=>assert.fail(),failed:()=>assert.fail()});
  const task=resolver.select('a.lnk'); await Promise.resolve(); resolver.invalidate();
  await resolver.ready(); complete('a.exe'); await task;
});

test('保存等待路径解析完成，解析失败后禁止保存，重新选择可以恢复', async () => {
  let complete, failures=0; const applied=[];
  const resolver=createProgramPathResolver({resolve:()=>new Promise((done,fail)=>{complete={done,fail};}),apply:path=>applied.push(path),failed:()=>failures++});
  resolver.select('missing.lnk'); await Promise.resolve();
  const ready=resolver.ready(); complete.fail(new Error('目标不存在'));
  await assert.rejects(ready,/目标不存在/); assert.equal(failures,1);
  resolver.select('valid.lnk'); await Promise.resolve(); complete.done('D:/game.exe');
  await resolver.ready(); assert.deepEqual(applied,['D:/game.exe']);
});
