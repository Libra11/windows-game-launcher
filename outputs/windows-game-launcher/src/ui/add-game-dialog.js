import { el } from '../lib/dom.js';
import { command } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { field, browse, submit } from './form-fields.js';
import { gamePlatformPicker } from './game-platform-picker.js';
import { steamGameSearch } from './steam-game-search.js';

export function addDialog(actions) {
  const form = el('form', 'form');
  const controls = el('fieldset', 'add-game-fields');
  const search = steamGameSearch();
  const exe = field('游戏启动文件', '', '选择游戏的 .exe 文件');
  exe.input.required = true; browse(exe, ['exe']);
  const platform = gamePlatformPicker(exe.input);
  const localLabel = el('label', 'setting-check steam-search-local');
  const localOnly = el('input'); localOnly.type = 'checkbox';
  localLabel.append(localOnly, el('span', '', '仅添加本地游戏，暂不关联 Steam'));
  localOnly.addEventListener('change', () => search.setLocalOnly(localOnly.checked));
  controls.append(exe.wrapper, platform.element, search.element, localLabel,
    el('p', 'form-hint', 'Steam 搜索用于补全封面和简介。成就按上方选择的平台获取，解锁记录独立保存在本地。'));
  form.append(controls);
  const dialog = modal('添加本地游戏', '先识别本地版本，再选择资料和成就来源。', form);
  dialog.classList.add('add-game-modal');
  dialog.addEventListener('close', () => { search.dispose(); platform.dispose(); }, { once: true });
  submit(form, '添加到游戏库', async () => {
    const appid = localOnly.checked ? '' : search.appid;
    if (!localOnly.checked && !appid) throw new Error('请先选择正确的 Steam 游戏，或勾选“仅添加本地游戏”。');
    if (!platform.value) throw new Error('请确认成就平台；无法识别时可暂不关联。');
    await platform.validate(appid);
    controls.disabled = true;
    try {
      const game = await actions.run('import_local', { exePath: exe.input.value.trim(), title: search.title, appid, achievementPlatform: platform.value, publicAchievementUrl: platform.publicSource, steamIdentityConfirmed: platform.identityConfirmed });
      let syncError = '';
      if (appid || platform.value === 'xbox') {
        try { await command('sync_game', { gameId: game.id }); }
        catch (error) { syncError = String(error); }
      }
      await closeModal(dialog);
      await actions.added(game);
      if (syncError) actions.toast(`游戏已添加，资料暂未同步：${syncError}；可稍后点击“更新资料”。`, true);
    } finally { controls.disabled = false; }
  });
}
