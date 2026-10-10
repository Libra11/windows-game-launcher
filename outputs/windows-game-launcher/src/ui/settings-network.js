import { el } from '../lib/dom.js';
import { customSelect } from './custom-select.js';
import { field } from './form-fields.js';

export function settingsNetwork(settings, actions, saved) {
  const element = el('section', 'settings-section');
  const header = el('header', 'settings-section-header');
  header.append(el('h2', '', '网络与代理'), el('p', 'settings-description', '此设置用于资料接口和图片。资料接口保存后立即生效；图片代理变更需重启应用。')); element.append(header);
  const form = el('form', 'settings-service form');
  const mode = customSelect([['system', '跟随系统'], ['direct', '不使用代理'], ['custom', '自定义代理']], settings.mode, '代理模式');
  const modeField = el('div', 'field'); modeField.append(el('span', '', '代理模式'), mode);
  const address = field('代理地址', settings.address, 'http://127.0.0.1:7890 或 socks5://127.0.0.1:1080');
  const tools = el('div', 'settings-tools settings-network-actions');
  const test = el('button', 'secondary', '测试连接'); test.type = 'button';
  const save = el('button', 'primary', '保存'); save.type = 'submit';
  const status = el('p', 'settings-description'); status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  const restartNotice = el('p', 'settings-description', '图片代理设置待生效。请从托盘菜单退出游迹，再重新打开；关闭窗口可能只会隐藏到托盘。');
  restartNotice.setAttribute('role', 'status'); restartNotice.setAttribute('aria-live', 'polite');
  restartNotice.hidden = !settings.webviewRestartRequired;
  tools.append(test, save); form.append(modeField, address.wrapper, tools, status, restartNotice); element.append(form);
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
        if (!disposed) status.textContent = `资料接口：${result.api}；图片域名（所选代理测试）：${result.image}`;
      } else {
        const result = await actions.run('save_network_settings', {settings});
        if (!disposed) {
          restartNotice.hidden = !result.webviewRestartRequired;
          status.textContent = result.webviewRestartRequired ? '已保存，资料接口立即生效；图片代理需重启应用后生效。' : '已保存，代理设置已生效。';
          saved(status.textContent);
        }
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
