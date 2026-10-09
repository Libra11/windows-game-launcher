import { el } from '../lib/dom.js';
import { field } from './form-fields.js';
import { command } from '../lib/bridge.js';
import { customSelect } from './custom-select.js';

export function gamePlatformPicker(exeInput, initial, gameId) {
  const root = el('div', 'field');
  const select = customSelect([['','请选择成就平台'],['steam','Steam 成就'],['xbox','Xbox 成就'],['none','暂不关联成就']],'','成就平台');
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
  let generation = 0, reviewGeneration = 0, disposed = false, profile, selection;
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
      hint.textContent = `${result.reason}${result.titleId ? ` · Xbox Title ID ${result.titleId}` : ''}${result.steamAppid ? ` · 本地配置编号 ${result.steamAppid}（不一定是游戏的官方编号）` : ''}${result.titleId ? result.automatic ? '。当前游戏有本地接口捕获支持，具体版本将在运行时验证。' : '。可获取 Xbox 定义，当前版本尚无自动解锁支持。' : ''}`;
      if (selection?.appid) review();
    } catch (error) { if (current === generation && !disposed) hint.textContent = String(error); }
  }
  if (initial) { profile = initial; select.value = ['steam','xbox','none'].includes(initial.platform) ? initial.platform : ''; hint.textContent = initial.reason || '请确认成就平台。'; }
  updateSource();
  exeInput.addEventListener('change', detect);
  select.addEventListener('change', () => {
    resetConfirmation(); updateSource();
    if (select.value === 'xbox' && !profile?.titleId) hint.textContent = '未检测到 Xbox Title ID，请选择对应版本的启动文件。';
    if (selection?.appid) review();
  });
  async function validate(appid, title = '', requireConfirmation = true) {
    const request = ++reviewGeneration;
    identityConfirmed = false;
    if (select.value !== 'steam') { resetConfirmation(); return; }
    if (!appid) throw new Error('Steam 成就需要先选择对应游戏。');
    const path = exeInput.value.trim();
    const result = await command('detect_game_platform', { exePath:path, appid, gameId });
    if (disposed || request !== reviewGeneration || path !== exeInput.value.trim() || select.value !== 'steam') {
      if (requireConfirmation) throw new Error('游戏选择已变更，请重新确认。');
      return;
    }
    profile = result;
    if (!result.steamAppid || result.steamAppid === appid) { resetConfirmation(); hint.textContent = result.steamAppid ? `已匹配《${title}》的 Steam 官方资料（${appid}），本地配置编号一致。` : `将使用《${title}》的 Steam 官方资料（${appid}）；本地未提供编号，解锁仍需真实记录。`; return; }
    const conflicts = result.recordConflicts || [];
    const key = `${path}|${result.steamAppid}|${appid}|${conflicts.join('|')}`;
    if (key !== confirmationKey) {
      confirmed.checked = path === initialPath && initial?.confirmedSteamAppid === appid && initial?.steamAppid === result.steamAppid;
      confirmationKey = key;
    }
    confirm.hidden = false;
    confirmText.textContent = `启动文件属于《${title}》，使用该游戏的 Steam 官方资料`;
    hint.textContent = `《${title}》的 Steam 官方资料编号是 ${appid}；本地配置写的是 ${result.steamAppid}，可能是沿用的其他编号。请按游戏名称和版本确认，不要按本地编号更换资料。${conflicts.length ? `该配置编号还被《${conflicts.join('》《')}》使用。添加后不会自动读取这个共用编号下的成就；需要修正游戏配置或在编辑中指定本游戏的独立记录文件。` : '添加后仍会核验真实记录是否属于该游戏，不匹配的记录不会导入。'}启动器不会自动改写配置或存档。`;
    if (!confirmed.checked && requireConfirmation) throw new Error('请确认启动文件属于上方选中的游戏；不需要判断哪一个数字正确。');
    identityConfirmed = confirmed.checked;
  }
  function review() { validate(selection?.appid || '', selection?.title || '', false).catch(error => { if (!disposed) hint.textContent = String(error); }); }
  return { element:root, validate, setGame(appid, title) { selection = { appid, title }; resetConfirmation(); reviewGeneration++; if (appid && exeInput.value.trim()) review(); }, get identityConfirmed() { return identityConfirmed; }, get value() { return select.value; }, get publicSource() { return source.input.value.trim(); }, dispose() { disposed = true; generation++; reviewGeneration++; exeInput.removeEventListener('change', detect); } };
}
