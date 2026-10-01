import { el } from '../lib/dom.js';
import { field } from './form-fields.js';
import { command } from '../lib/bridge.js';

export function gamePlatformPicker(exeInput, initial) {
  const root = el('div', 'field');
  const select = el('select');
  for (const [value, text] of [['', '请选择成就平台'], ['steam', 'Steam 成就'], ['xbox', 'Xbox 成就'], ['none', '暂不关联成就']]) {
    const option = el('option', '', text); option.value = value; select.append(option);
  }
  const hint = el('p', 'form-hint', '选择游戏启动文件后自动识别；封面和简介可以独立关联 Steam。'); hint.setAttribute('aria-live', 'polite');
  root.append(el('span', '', '成就平台'), select, hint);
  const source = field('Xbox 公开成就页面', initial?.publicSource || '', 'https://www.exophase.com/game/游戏名-xbox/achievements/');
  source.wrapper.classList.add('xbox-public-source');
  root.append(source.wrapper, el('p', 'form-hint', 'Xbox 资料无需登录；页面必须对应本地游戏，解锁来自游戏真实事件。'));
  const updateSource = () => { source.wrapper.hidden = select.value !== 'xbox'; };
  updateSource();
  const initialPath = exeInput.value.trim();
  const confirm = el('label', 'setting-check steam-identity-confirm'); confirm.hidden = true;
  const confirmed = el('input'); confirmed.type = 'checkbox';
  const confirmText = el('span'); confirm.append(confirmed, confirmText); root.append(confirm);
  let confirmationKey = '', identityConfirmed = false;
  function resetConfirmation() { confirmationKey = ''; identityConfirmed = false; confirmed.checked = false; confirm.hidden = true; }
  let generation = 0, disposed = false, profile;
  async function detect() {
    const current = ++generation; select.value = ''; profile = undefined; resetConfirmation(); updateSource();
    if (!exeInput.value.trim()) { hint.textContent = '请先选择启动文件。'; return; }
    hint.textContent = '正在识别成就平台…';
    try {
      const result = await command('detect_game_platform', { exePath: exeInput.value.trim() });
      if (disposed || current !== generation) return;
      profile = result; source.input.value = result.publicSource || "";
      if (['steam', 'xbox'].includes(result.platform)) select.value = result.platform;
      updateSource();
      hint.textContent = `${result.reason}${result.titleId ? ` · Xbox Title ID ${result.titleId}` : ''}${result.steamAppid ? ` · Steam AppID ${result.steamAppid}` : ''}${result.titleId ? result.automatic ? '。当前游戏有本地接口捕获支持，具体版本将在运行时验证。' : '。可获取 Xbox 定义，当前版本尚无自动解锁支持。' : ''}`;
    } catch (error) { if (current === generation && !disposed) hint.textContent = String(error); }
  }
  if (initial) { profile = initial; select.value = ['steam','xbox','none'].includes(initial.platform) ? initial.platform : ''; hint.textContent = initial.reason || '请确认成就平台。'; }
  updateSource();
  exeInput.addEventListener('change', detect);
  select.addEventListener('change', () => {
    resetConfirmation(); updateSource();
    if (select.value === 'xbox' && !profile?.titleId) hint.textContent = '未检测到 Xbox Title ID，请选择对应版本的启动文件。';
  });
  async function validate(appid) {
    identityConfirmed = false;
    if (select.value !== 'steam') { resetConfirmation(); return; }
    if (!appid) throw new Error('Steam 成就需要先选择对应游戏。');
    const path = exeInput.value.trim();
    const result = await command('detect_game_platform', { exePath:path });
    if (disposed || path !== exeInput.value.trim()) throw new Error('启动文件已变更，请重新确认。');
    profile = result;
    if (!result.steamAppid || result.steamAppid === appid) { resetConfirmation(); return; }
    const key = `${path}|${result.steamAppid}|${appid}`;
    if (key !== confirmationKey) {
      confirmed.checked = path === initialPath && initial?.confirmedSteamAppid === appid && initial?.steamAppid === result.steamAppid;
      confirmationKey = key;
    }
    confirm.hidden = false;
    confirmText.textContent = `本地配置 AppID ${result.steamAppid} 与所选游戏 ${appid} 不一致；我确认以所选游戏获取成就资料`;
    hint.textContent = '发现 Steam 编号冲突。请确认所选游戏，启动器不会修改游戏配置。自动解锁仍需匹配的真实记录。';
    if (!confirmed.checked) throw new Error('Steam 编号不一致，请核对并勾选确认，或重新选择游戏。');
    identityConfirmed = true;
  }
  return { element:root, validate, get identityConfirmed() { return identityConfirmed; }, get value() { return select.value; }, get publicSource() { return source.input.value.trim(); }, dispose() { disposed = true; generation++; exeInput.removeEventListener('change', detect); } };
}
