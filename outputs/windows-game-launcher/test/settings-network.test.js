import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

class Element extends EventTarget {
  constructor(tag, text = '') { super(); this.tagName = tag.toUpperCase(); this.textContent = text; this.children = []; this.value = ''; this.classList = {add() {}, remove() {}}; }
  append(...children) { this.children.push(...children); }
  setAttribute() {}
  reportValidity() { return true; }
}

test('代理草稿只在保存时生效，连接测试使用当前输入，保存失败可重试', async () => {
  globalThis.networkSettingsTest = {
    el: (tag, _class, text) => new Element(tag, text),
    customSelect: (_options, value) => Object.assign(new Element('div'), {value}),
    field: (_label, value) => {
      const wrapper = new Element('label'), input = Object.assign(new Element('input'), {value});
      wrapper.append(input); return {wrapper,input};
    },
  };
  try {
    const source = (await readFile(new URL('../src/ui/settings-network.js', import.meta.url), 'utf8')).replace(/^import .*;\r?\n/gm, '');
    const {settingsNetwork} = await import(`data:text/javascript;base64,${Buffer.from('const {el, customSelect, field} = globalThis.networkSettingsTest;\n' + source).toString('base64')}`);
    const calls = []; let saved = 0, rejectSave = false;
    const part = settingsNetwork({mode:'system', address:''}, {run:async (name, args) => {
      calls.push({name,args});
      if (name === 'test_network_connection') return {api:'连接成功',image:'连接失败'};
      if (rejectSave) throw new Error('保存失败');
    }}, () => {saved++;});
    const form = part.element.children[1];
    const mode = form.children[0].children[1], address = form.children[1];
    const [connectionTest, save] = form.children[2].children, status = form.children[3];
    assert.equal(address.hidden, true);
    mode.value = 'custom'; mode.dispatchEvent(new Event('change'));
    assert.equal(address.hidden, false);
    address.children[0].value = ' http://127.0.0.1:17890 ';
    assert.equal(calls.length, 0);
    await connectionTest.onclick();
    assert.equal(calls.length, 1);
    assert.equal(calls[0].name, 'test_network_connection');
    assert.equal(calls[0].args.settings.mode, 'custom');
    assert.equal(calls[0].args.settings.address, 'http://127.0.0.1:17890');
    assert.equal(saved, 0);
    assert.match(status.textContent, /资料接口：连接成功；图片资源：连接失败/);
    rejectSave = true;
    await form.onsubmit({preventDefault() {}});
    assert.equal(saved, 0);
    assert.equal(save.disabled, false);
    assert.match(status.textContent, /保存失败/);
    rejectSave = false;
    await form.onsubmit({preventDefault() {}});
    assert.equal(saved, 1);
    assert.equal(calls.at(-1).name, 'save_network_settings');
    assert.equal(status.textContent, '已保存');
    part.dispose();
  } finally { delete globalThis.networkSettingsTest; }
});
