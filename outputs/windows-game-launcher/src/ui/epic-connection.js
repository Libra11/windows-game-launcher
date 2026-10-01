import { el, button, icon } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { field, submit } from './form-fields.js';

export function epicLoginDialog(actions, afterLogin) {
  const form = el('form', 'form');
  const code = field('一次性授权码', '', '粘贴 authorizationCode 或完整授权结果', 'password');
  code.input.required = true;
  const status = el('p', 'form-hint', '登录后，Epic 页面会显示授权结果。复制 authorizationCode 的值，回到这里完成连接。');
  status.setAttribute('aria-live', 'polite');
  const login = button('在浏览器登录 Epic', 'secondary', async () => {
    await actions.run('epic_begin_login');
    status.textContent = '浏览器已打开。完成 Epic 登录后，复制页面上的 authorizationCode 并粘贴到下方。';
    code.input.focus();
  }, 'epic');
  login.disabled = preview;
  const agreements = button('打开 Epic 商店登录', 'secondary', async () => {
    await actions.run('epic_open_account_login');
    status.textContent = '请在 Epic 商店登录同一账号，处理实际出现的账号提示。如果商店没有提示，请在官方 Epic 客户端登录。完成后回到这里重新获取授权码。';
  });
  agreements.disabled = preview;
  form.append(login, status, agreements, el('p', 'form-hint', '若登录页面返回 PRIVACY_POLICY_ACCEPTANCE，请先在 Epic 商店或官方客户端登录并处理账号提示，再重新获取授权码。'), code.wrapper, el('p', 'form-hint', '授权保存在本机，后续导入会自动刷新凭证。无需填写 Epic 密码或读取本机游戏清单。'));
  const dialog = modal('连接 Epic 账号', '授权读取账号游戏库，包括尚未安装的游戏。', form);
  dialog.addEventListener('close', () => { code.input.value = ''; }, { once:true });
  submit(form, '连接账号', async () => {
    await actions.run('epic_complete_login', { code:code.input.value.trim() });
    code.input.value = '';
    await closeModal(dialog);
    actions.toast('Epic 账号已连接');
    await afterLogin?.();
  });
  if (preview) form.querySelector('[type="submit"]').disabled = true;
  return dialog;
}

export function epicConnection(actions) {
  const root = el('section', 'form');
  const heading = el('div', 'connection-card'); heading.append(icon('epic'), el('div', '', '连接 Epic 游戏库'));
  const status = el('p', 'form-hint', '正在读取连接状态…'); status.setAttribute('aria-live', 'polite');
  let disposed = false;
  const refresh = async () => {
    try {
      const connection = await actions.run('epic_connection_status');
      if (disposed) return;
      status.textContent = connection.connected ? `已连接 ${connection.displayName || 'Epic 账号'}，导入时自动刷新授权。` : '尚未连接。登录 Epic 后可导入账号游戏库，无需安装游戏。';
      login.querySelector('span').textContent = connection.connected ? '重新登录 Epic' : '登录 Epic';
      disconnect.hidden = !connection.connected;
      importGames.disabled = !connection.connected || preview;
    } catch (error) { if (!disposed) status.textContent = String(error); }
  };
  const login = button('登录 Epic', 'secondary', () => epicLoginDialog(actions, refresh), 'epic');
  const importGames = button('导入 Epic 游戏', 'secondary', async () => {
    status.textContent = '正在读取 Epic 账号游戏库和游戏资料…';
    try {
      const count = await actions.run('import_epic');
      await actions.refresh();
      if (!disposed) status.textContent = `Epic 游戏库已同步，新增 ${count} 款游戏。`;
      actions.toast(`新增 ${count} 款 Epic 游戏`);
    } catch (error) { if (!disposed) status.textContent = String(error); }
  }, 'refresh');
  const disconnect = button('断开连接', 'secondary', async () => {
    await actions.run('epic_disconnect'); await refresh(); actions.toast('Epic 已断开连接，已导入的游戏保留');
  });
  const tools = el('div', 'settings-tools'); tools.append(login, importGames, disconnect);
  login.disabled = preview; importGames.disabled = true; disconnect.hidden = true;
  root.append(heading, status, tools, el('p', 'form-hint', '通过 Epic 账号授权读取游戏名称、封面和简介。安装及启动由 Epic 客户端处理。'));
  refresh();
  return { element:root, dispose() { disposed = true; } };
}
