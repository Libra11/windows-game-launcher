import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

class Element extends EventTarget {
  constructor(tag, text = '') { super(); this.tagName = tag.toUpperCase(); this.textContent = text; this.children = []; this.value = ''; this.classList = {add() {}, remove() {}}; }
  append(...children) { this.children.push(...children); }
  setAttribute() {}
  reportValidity() { return true; }
}

test('代理草稿只在保存时生效，连接测试和保存失败不影响待重启状态', async () => {
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
    const calls = []; let saved = 0, rejectSave = false, restartRequired = true;
    const actions = {run:async (name, args) => {
      calls.push({name,args});
      if (name === 'test_network_connection') return {api:'连接成功',image:'连接失败'};
      if (rejectSave) throw new Error('保存失败');
      return {...args.settings,webviewRestartRequired:restartRequired};
    }};
    const part = settingsNetwork({mode:'system', address:'',webviewRestartRequired:false}, actions, () => {saved++;});
    const form = part.element.children[1];
    const mode = form.children[0].children[1], address = form.children[1];
    const [connectionTest, save] = form.children[2].children, status = form.children[3], restartNotice = form.children[4];
    assert.equal(address.hidden, true);
    assert.equal(restartNotice.hidden, true);
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
    assert.match(status.textContent, /资料接口：连接成功；图片域名（所选代理测试）：连接失败/);
    assert.equal(restartNotice.hidden, true);
    rejectSave = true;
    await form.onsubmit({preventDefault() {}});
    assert.equal(saved, 0);
    assert.equal(save.disabled, false);
    assert.match(status.textContent, /保存失败/);
    rejectSave = false;
    await form.onsubmit({preventDefault() {}});
    assert.equal(saved, 1);
    assert.equal(calls.at(-1).name, 'save_network_settings');
    assert.match(status.textContent, /资料接口立即生效；图片代理需重启应用后生效/);
    assert.equal(restartNotice.hidden, false);
    await form.onsubmit({preventDefault() {}});
    assert.equal(restartNotice.hidden, false, '重复保存不能当作 WebView 已应用配置');
    address.children[0].value = 'http://127.0.0.1:17891';
    address.children[0].dispatchEvent(new Event('input'));
    await connectionTest.onclick();
    assert.equal(restartNotice.hidden, false, '编辑草稿或测试连接不能清除重启提示');
    rejectSave = true;
    await form.onsubmit({preventDefault() {}});
    assert.equal(restartNotice.hidden, false, '保存失败保留此前待生效状态');
    rejectSave = false; restartRequired = false;
    mode.value = 'system'; mode.dispatchEvent(new Event('change'));
    assert.equal(restartNotice.hidden, false, '恢复草稿尚未保存时仍需重启');
    await form.onsubmit({preventDefault() {}});
    assert.equal(restartNotice.hidden, true, '保存回启动配置后无需重启');
    assert.equal(status.textContent, '已保存，代理设置已生效。');
    part.dispose();
    const reopened = settingsNetwork({mode:'custom',address:'http://127.0.0.1:17890',webviewRestartRequired:true}, actions, () => {});
    assert.equal(reopened.element.children[1].children[4].hidden, false, '再次进入设置仍显示待重启提示');
    assert.match(reopened.element.children[1].children[4].textContent, /托盘菜单退出/);
    reopened.dispose();
    const restarted = settingsNetwork({mode:'custom',address:'http://127.0.0.1:17890',webviewRestartRequired:false}, actions, () => {});
    assert.equal(restarted.element.children[1].children[4].hidden, true, '重启后的已应用配置不再提示');
    restarted.dispose();
  } finally { delete globalThis.networkSettingsTest; }
});
