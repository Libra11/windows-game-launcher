import { gamePlatformPicker } from './game-platform-picker.js';
import { el } from '../lib/dom.js';
import { command } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { field, browse, submit } from './form-fields.js';
import { steamGameSearch } from './steam-game-search.js';

export function editLocalDialog(game, actions) {
  const form = el('form', 'form');
  const controls = el('fieldset', 'add-game-fields');
  const search = steamGameSearch(game);
  const exe = field('游戏启动文件', game.exePath, '选择实际游戏的 .exe'); exe.input.required = true; browse(exe, ['exe']);
  const platform = gamePlatformPicker(exe.input, game.achievementPlatform, game.id);
  search.element.addEventListener('change', () => platform.setGame(search.appid, search.title));
  platform.setGame(search.appid, search.title);
  const record = field('自定义解锁记录（可选）', game.customUnlockPath, 'achievements.json 或 achievements.ini 的完整路径'); browse(record, ['json', 'ini']);
  const local = el('label', 'setting-check'); const localOnly = el('input'); localOnly.type = 'checkbox'; localOnly.checked = !game.appid;
  local.append(localOnly, el('span', '', '跳过 Steam 资料关联（不获取 Steam 成就）'));
  if (localOnly.checked) search.setLocalOnly(true);
  localOnly.onchange = () => search.setLocalOnly(localOnly.checked);
  const confirm = el('label', 'setting-check'); const rebind = el('input'); rebind.type = 'checkbox';
  confirm.append(rebind, el('span', '', '若更换成就平台或对应游戏，同意清除本条目的旧解锁记录'));
  controls.append(exe.wrapper, platform.element, search.element, record.wrapper, local,
    el('p', 'form-hint', 'Steam 仅用于封面和简介时，更换 Steam 关联不会清除 Xbox 解锁；更换成就平台会重置解锁记录。'), confirm);
  form.append(controls);
  const dialog = modal('编辑本地游戏', '搜索并选择正确的 Steam 游戏，或调整启动与记录路径。', form);
  dialog.classList.add('add-game-modal'); dialog.addEventListener('close', () => { search.dispose(); platform.dispose(); }, { once:true });
  submit(form, '保存修改', async () => {
    const appid = localOnly.checked ? '' : search.appid;
    if (!localOnly.checked && !appid) throw new Error('请从搜索结果中选择正确的 Steam 游戏。');
    const changed = appid !== game.appid;
    if (!platform.value) throw new Error('请确认成就平台。');
    const platformChanged = platform.value !== game.achievementPlatform?.platform || (changed && platform.value === 'steam');
    if (platformChanged && !rebind.checked) throw new Error('关联已变更，请勾选确认清除旧资料和解锁记录。');
    await platform.validate(appid, search.title);
    controls.disabled = true;
    try {
      await actions.run('update_local', { gameId:game.id, title:search.title, exePath:exe.input.value.trim(), appid, customUnlockPath:record.input.value.trim(), achievementPlatform:platform.value, publicAchievementUrl:platform.publicSource, steamIdentityConfirmed:platform.identityConfirmed });
      let syncError = '';
      if (changed || platformChanged || platform.publicSource !== (game.achievementPlatform?.publicSource || '')) { try { await command('sync_game', {gameId:game.id}); } catch (error) { syncError = String(error); } }
      await actions.run('scan_now', {gameId:game.id});
      await closeModal(dialog); await actions.refresh();
      if (syncError) actions.toast(`设置已保存，资料暂未同步：${syncError}`, true);
      actions.afterSaved?.();
    } finally { controls.disabled = false; }
  });
}
