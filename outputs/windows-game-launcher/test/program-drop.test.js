import test from 'node:test';
import assert from 'node:assert/strict';
import { createProgramDropHandler } from '../src/lib/program-drop.js';

function setup(overrides = {}) {
  const opened = [], messages = [], prepared = [];
  const handler = createProgramDropHandler({
    prepare: async path => { prepared.push(path); return { exePath:path, title:'游戏' }; },
    open: candidate => opened.push(candidate),
    notify: message => messages.push(message),
    isBlocked: () => false,
    ...overrides,
  });
  return { handler, opened, messages, prepared };
}

test('接受 EXE 和快捷方式，仅打开导入窗口，不直接保存或启动', async () => {
  const state = setup();
  await state.handler(['C:\\Games\\游戏.EXE']);
  await state.handler(['C:\\Desktop\\游戏.LNK']);
  assert.equal(state.opened.length, 2);
  assert.deepEqual(state.prepared, ['C:\\Games\\游戏.EXE', 'C:\\Desktop\\游戏.LNK']);
});

test('多文件、其他格式及已有窗口不会触发解析', async () => {
  const state = setup();
  for (const paths of [[], ['a.exe', 'b.exe'], ['a.url'], ['games']]) await state.handler(paths);
  assert.equal(state.prepared.length, 0);
  const blocked = setup({ isBlocked: () => true });
  await blocked.handler(['a.exe']);
  assert.equal(blocked.prepared.length, 0);
});

test('重复条目和损坏文件不打开导入窗口，失败后仍可重试', async () => {
  let attempts = 0;
  const state = setup({ prepare: async () => {
    attempts++;
    if (attempts === 1) throw new Error('快捷方式目标不存在');
    if (attempts === 2) return { existingGameId:'local-1' };
    return { exePath:'a.exe' };
  }});
  for (let i = 0; i < 3; i++) await state.handler(['a.lnk']);
  assert.equal(state.opened.length, 1);
  assert.match(state.messages[0], /目标不存在/);
  assert.match(state.messages[1], /无需重复/);
});

test('解析期间重复拖入及新打开的弹窗不会产生多个导入窗口', async () => {
  let resolve, blocked = false;
  const state = setup({
    prepare: () => new Promise(done => { resolve = done; }),
    isBlocked: () => blocked,
  });
  const pending = state.handler(['a.exe']);
  await state.handler(['b.exe']);
  blocked = true;
  resolve({ exePath:'a.exe' });
  await pending;
  assert.equal(state.opened.length, 0);
  assert.equal(state.messages.length, 2);
});
