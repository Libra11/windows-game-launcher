import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

class Element extends EventTarget {
  constructor(text = '') { super(); this.textContent = text; this.children = []; this.value = ''; this.classList = { add() {} }; }
  append(...children) { this.children.push(...children); }
  setAttribute() {}
}

test('选择游戏后立即说明官方资料与共用编号，确认名称后才允许保存', async () => {
  const requests = [];
  const profile = { platform:'steam', reason:'本地配置', steamAppid:'2456740', recordConflicts:['GRIME II'] };
  globalThis.platformPickerTest = {
    el: (_tag, _class, text) => new Element(text),
    field: () => ({ wrapper:new Element(), input:new Element() }),
    customSelect: () => new Element(),
    command: async (_name, args) => { requests.push(args); return profile; },
  };
  try {
    const source = (await readFile(new URL('../src/ui/game-platform-picker.js', import.meta.url), 'utf8'))
      .replace(/^import .*;\r?\n/gm, '');
    const { gamePlatformPicker } = await import(`data:text/javascript;base64,${Buffer.from('const {el, field, customSelect, command} = globalThis.platformPickerTest;\n' + source).toString('base64')}`);
    const exe = new Element(); exe.value = 'D:/games/Mina/game.exe';
    const picker = gamePlatformPicker(exe, { platform:'steam' }, 'mina');
    picker.setGame('1875580', '挖掘者米娜');
    await new Promise(resolve => setImmediate(resolve));
    const hint = picker.element.children[2];
    const confirmation = picker.element.children.at(-1);
    assert.match(hint.textContent, /挖掘者米娜.*官方资料编号是 1875580/);
    assert.match(hint.textContent, /GRIME II.*不会自动读取/);
    assert.match(confirmation.children[1].textContent, /启动文件属于《挖掘者米娜》/);
    assert.equal(picker.identityConfirmed, false);
    await assert.rejects(picker.validate('1875580', '挖掘者米娜'), /不需要判断哪一个数字正确/);
    confirmation.children[0].checked = true;
    await picker.validate('1875580', '挖掘者米娜');
    assert.equal(picker.identityConfirmed, true);
    assert.deepEqual(requests.at(-1), { exePath:exe.value, appid:'1875580', gameId:'mina' });
    picker.setGame('2529790', 'GRIME II');
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(picker.identityConfirmed, false);
    assert.equal(confirmation.children[0].checked, false);
    picker.dispose();
  } finally { delete globalThis.platformPickerTest; }
});
