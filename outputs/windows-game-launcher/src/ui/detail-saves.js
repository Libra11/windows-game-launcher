import { automationSettings, taskSummary, restoreWithReview, checkSaveBeforeLaunch } from './save-automation.js';
import { bindDirectory } from './save-bind.js';
import { customSelect } from './custom-select.js';
import { listen } from '@tauri-apps/api/event';
import { el, button } from '../lib/dom.js';
import { command, preview } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { submit } from './form-fields.js';

function confirmAction(title, text, label, action) {
  const form = el('form', 'form');
  form.append(el('p', '', text));
  const dialog = modal(title, '操作前请确认游戏和其他存档工具均已退出。', form);
  form.append(button('取消', 'secondary', () => closeModal(dialog)));
  submit(form, label, async () => { await action(); await closeModal(dialog); });
}
function size(bytes) { return `${(Number(bytes || 0) / 1024 / 1024).toFixed(2)} MiB`; }
function timestamp(value) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '时间未知' : date.toLocaleString();
}
export function savesButton(game) {
  return button('游戏存档', 'secondary', () => openSaves(game), 'folder');
}
export function openSaves(game) {
  const content = el('div', 'form');
  const status = el('p', '');
  status.setAttribute('role', 'status');
  const list = el('div', 'form');
  content.append(el('p', '', '可启用退出后自动备份和上传。云端版本不会自动替换本机文件；恢复会完整替换绑定目录，并保留原目录及安全快照。'), status);
  const actions = el('div', 'game-actions');
  let busy = false;
  const run = async action => {
    if (busy) return;
    busy = true; status.className = ''; status.textContent = '正在处理…';
    content.querySelectorAll('button').forEach(b => { b.disabled = true; });
    try { const message = await action(); status.textContent = message || '操作完成'; }
    catch (error) { status.className = 'form-error'; status.textContent = String(error); }
    finally { busy = false; content.querySelectorAll('button').forEach(b => { b.disabled = false; }); }
  };
  const refresh = async () => {
    const entries = await command('saves_list', { gameId: game.id });
    list.replaceChildren();
    if (!entries.length) list.append(el('p', '', '尚未绑定存档目录。'));
    entries.forEach(({ profile, snapshots, settings={}, tasks=[], flags={} }) => {
      const section = el('section', 'form');
      section.append(el('h3', '', profile.name), el('p', '', profile.root),
        el('p', '', profile.cloudId ? '已关联云端档案' : '仅保存在本机'));
      section.append(el('p','',taskSummary(settings,tasks)));
      if(settings.lastSuccess)section.append(el('p','',`最近备份成功：${timestamp(settings.lastSuccess)}`));
      const controls = el('div', 'game-actions');
      controls.append(button('自动备份设置','secondary',()=>automationSettings(profile,settings,refresh)));
      if(tasks.some(t=>t.error))controls.append(button('重新尝试','secondary',()=>run(async()=>{await command('saves_retry',{profileId:profile.id});await refresh();return '已重新排队';})));
      controls.append(button('立即备份', 'secondary', () => confirmAction(
        '创建本地存档快照', '请确认专用目录中只包含此游戏的存档。备份期间游迹将阻止启动游戏。', '确认备份',
        async () => {
          await command('saves_backup', { profileId: profile.id });
          await refresh(); status.textContent = '本地快照已保存；开启自动上传后会继续上传。';
        })));
      if (profile.cloudId) {
        controls.append(button('上传 / 查询状态', 'secondary', () => run(async () => {
          const result = await command('saves_upload', { profileId: profile.id }); await refresh();
          return result.pending ? '文件已上传，云端正在排队校验，游迹会自动查询并继续上传后续快照。' : '待传快照已通过云端校验';
        })));
        controls.append(button('云端历史与分叉', 'secondary', () => run(async () => {
          await cloudHistory(profile, refresh); return '已读取云端历史';
        })));
      } else {
        controls.append(button('关联云存档', 'secondary', () => run(async () => {
          await connectProfile(game, profile, refresh); return '请选择云端档案';
        })));
      }
      section.append(controls);
      snapshots.forEach(snapshot => {
        if(flags[snapshot.id]?.available===false)return;
        const row = el('div', 'form');
        row.append(el('strong', '', timestamp(snapshot.createdAt)),
          el('p', '', `${size(snapshot.bytes)} · ${snapshot.uploaded ? '已上传 / 已下载' : snapshot.uploadStarted ? '待上传或云端校验中' : '本地快照，待上传'}`));
        if (snapshot.error) row.append(el('p', 'form-error', snapshot.error));
        row.append(button('恢复此本地快照','secondary',()=>run(async()=>{
          const restored=await restoreWithReview(profile,snapshot.id,refresh);
          return restored?'存档恢复完成，原目录和安全快照已保留。':'已取消恢复';
        })));
        row.append(button(flags[snapshot.id]?.pinned?'取消保留':'手动保留','secondary',()=>run(async()=>{await command('saves_pin',{profileId:profile.id,snapshotId:snapshot.id,pinned:!flags[snapshot.id]?.pinned});await refresh();})));
        section.append(row);
      });
      list.append(section);
    });
  };
  actions.append(button('识别 / 绑定存档目录', 'secondary', () => bindDirectory(game, refresh)),
    button('刷新', 'secondary', () => run(refresh)));
  content.append(actions, list);
  const dialog=modal(`${game.title} · 游戏存档`, '本地快照与云端历史', content);
  let disposed=false,unlisten;
  dialog.addEventListener('close',()=>{disposed=true;unlisten?.();},{once:true});
  if(!preview)listen('save-automation-changed',()=>{if(!disposed&&!busy)refresh().catch(e=>{status.textContent=String(e);});}).then(stop=>{if(disposed)stop();else unlisten=stop;});
  if (preview) { status.textContent = '浏览器预览不能读取或修改本机存档，请在桌面应用中使用。'; actions.querySelectorAll('button').forEach(b => { b.disabled = true; }); }
  else run(refresh);
}
async function connectProfile(game, profile, refresh) {
  const response = await command('saves_cloud_profiles', { gameId: game.id });
  const profiles = Array.isArray(response) ? response : response.profiles;
  if (!Array.isArray(profiles)) throw new Error('云档案列表格式错误');
  const form = el('form', 'form');
  const select = customSelect([['', '创建新的云端档案'], ...profiles.map(p => [p.id, p.name])], '', '云端档案');
  const label = el('label', 'field'); label.append(el('span', '', '云端档案'), select);
  form.append(label, el('p', '', '关联后不能换绑；本机内容不会自动成为云端当前版本。其他设备首次上传可能形成分叉。'));
  const dialog = modal('关联云存档', profile.name, form);
  submit(form, '确认关联', async () => {
    await command('saves_connect', { profileId: profile.id, cloudId: select.value || null });
    await refresh(); await closeModal(dialog);
    await checkSaveBeforeLaunch(game);
  });
}
async function cloudHistory(profile, refresh) {
  const content = el('div', 'form');
  const status = el('p', ''); status.setAttribute('role', 'status'); content.append(status);
  const rows = el('div', 'form'); content.append(rows);
  let running = false;
  const perform = async action => {
    if (running) throw new Error('请等待当前操作完成');
    running = true; status.textContent = '正在处理…'; status.className = '';
    try { await action(); await load(); await refresh(); status.textContent = '操作完成'; }
    catch (e) { status.textContent = String(e); status.className = 'form-error'; throw e; }
    finally { running = false; }
  };
  async function load() {
    const result = await command('saves_history', { profileId: profile.id });
    rows.replaceChildren();
    if (!result.snapshots.length) rows.append(el('p', '', '云端尚无快照'));
    result.snapshots.forEach(s => {
      const row = el('div', 'form');
      row.append(el('strong', '', `${timestamp(s.createdAt || s.serverCreatedAt)}${result.profile.selectedSnapshotId === s.id ? ' · 当前选定版本' : ''}`),
        el('p', '', `设备：${s.deviceName || s.deviceId || '未知'} · ${size(s.bytes)}`),
        el('p', '', `父版本：${(s.parentIds || []).join(', ') || '无（独立来源）'}`));
      if(s.available===false)row.append(el('p','','归档已清理，保留版本关系，不可恢复'));
      if(s.available!==false)row.append(button('下载到本机', 'secondary', () => {
        perform(() => command('saves_download', { profileId: profile.id, snapshotId: s.id })).catch(() => {});
      }));
      if(s.available!==false)row.append(button('选择此云端版本', 'secondary', () => confirmAction(
        '选择云端版本', '此操作解决当前显示的云端分叉并保留历史版本，不修改本机目录。时间较晚不代表游戏进度较新。', '确认选择',
        () => perform(() => command('saves_resolve', {
          profileId: profile.id, snapshotId: s.id, baseRevision: result.profile.revision,
          parentIds: [...new Set([s.id, result.profile.selectedSnapshotId, ...result.snapshots.filter(v => !result.snapshots.some(child => (child.parentIds || []).includes(v.id))).map(v => v.id)].filter(Boolean))],
        })))));
      if(s.available!==false)row.append(button(s.pinned?'取消手动保留':'手动保留','secondary',()=>{perform(()=>command('saves_pin',{profileId:profile.id,snapshotId:s.id,pinned:!s.pinned})).catch(()=>{});}));
      row.append(button('删除云端快照', 'secondary', () => confirmAction(
        '删除云端快照？', '此快照将不再从云端提供恢复。本机已有副本保留；服务端可能拒绝删除被引用的版本。', '确认删除',
        () => perform(() => command('saves_delete_cloud', { snapshotId: s.id, confirmed: true })))));
      rows.append(row);
    });
  }
  await load();
  modal('云端历史与分叉', '下载后返回本地列表，另行确认恢复。', content);
}
