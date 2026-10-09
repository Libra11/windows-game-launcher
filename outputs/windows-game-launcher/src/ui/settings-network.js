import { el } from '../lib/dom.js';
import { customSelect } from './custom-select.js';
import { field } from './form-fields.js';

export function settingsNetwork(settings, actions, saved) {
  const element = el('section', 'settings-section');
  const header = el('header', 'settings-section-header');
  header.append(el('h2', '', '网络与代理'), el('p', 'settings-description', '设置资料与图片的连接方式。')); element.append(header);
  const form = el('form', 'settings-service form');
  const mode = customSelect([['system', '跟随系统'], ['direct', '不使用代理'], ['custom', '自定义代理']], settings.mode, '代理模式');
  const modeField = el('div', 'field'); modeField.append(el('span', '', '代理模式'), mode);
  const address = field('代理地址', settings.address, 'http://127.0.0.1:7890 或 socks5://127.0.0.1:1080');
  const tools = el('div', 'settings-tools settings-network-actions');
  const test = el('button', 'secondary', '测试连接'); test.type = 'button';
  const save = el('button', 'primary', '保存'); save.type = 'submit';
  const status = el('p', 'settings-description'); status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  tools.append(test, save); form.append(modeField, address.wrapper, tools, status); element.append(form);
  let disposed = false, busy = false;
  function update() {
    address.wrapper.hidden = mode.value !== 'custom';
    address.input.required = mode.value === 'custom';
    address.input.disabled = busy || mode.value !== 'custom';
    status.textContent = ''; status.classList.remove('form-error');
  }
  mode.addEventListener('change', update); address.input.addEventListener('input', () => { status.textContent = ''; });
  async function perform(testing) {
    if (busy || !form.reportValidity()) return;
    const settings = { mode: mode.value, address: address.input.value.trim() };
    busy = true; mode.disabled = true; test.disabled = true; save.disabled = true; address.input.disabled = true;
    status.classList.remove('form-error'); status.textContent = testing ? '正在测试连接…' : '正在保存…';
    try {
      if (testing) {
        const result = await actions.run('test_network_connection', {settings});
        if (!disposed) status.textContent = `资料接口：${result.api}；图片资源：${result.image}`;
      } else {
        await actions.run('save_network_settings', {settings});
        if (!disposed) { status.textContent = '已保存'; saved(); }
      }
    } catch (error) {
      if (!disposed) { status.textContent = String(error); status.classList.add('form-error'); }
    } finally {
      busy = false;
      if (!disposed) { mode.disabled = false; test.disabled = false; save.disabled = false; address.input.disabled = mode.value !== 'custom'; }
    }
  }
  test.onclick = () => perform(true);
  form.onsubmit = async event => { event.preventDefault(); await perform(false); };
  update();
  return {element, dispose() { disposed = true; }};
}
