import { el, button } from '../lib/dom.js';
import { command, chooseFile } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { field, submit } from './form-fields.js';
import { customSelect } from './custom-select.js';
import { checkbox, checkSaveBeforeLaunch } from './save-automation.js';
import { directoryDiscovery } from './save-discovery.js';
import './save-forms.css';

export async function bindDirectory(game, refresh) {
  const form = el('form', 'form save-bind-form');
  const name = field('档案名称', '主存档');
  const root = field('已选目录', '', '从上方候选中选择，或手动指定目录');
  root.input.readOnly = true;
  const backup = checkbox('退出游戏后自动备份');
  const upload = checkbox('自动上传云端');
  const error = el('p', 'save-status'); error.setAttribute('role', 'status');
  const discovery = directoryDiscovery(game, path => { root.input.value = path; });
  const directory = el('section', 'save-form-section');
  const directoryActions = el('div', 'save-inline-actions');
  directoryActions.append(button('手动选择目录', 'secondary', async () => {
    try { const value = await chooseFile({ directory: true, multiple: false }); if (value) root.input.value = value; }
    catch (e) { error.textContent = String(e); }
  }, 'folder'));
  directory.append(discovery.element, root.wrapper, directoryActions,
    el('p', 'form-hint', '确认目录只属于此游戏。恢复时会替换整个目录，并保留原进度。'));
  const local = el('section', 'save-form-section');
  local.append(el('h3', '', '备份设置'), name.wrapper, backup.label,
    el('p', 'form-hint', '退出游戏后等待文件稳定，有变化才备份。'));
  const cloud = el('section', 'save-form-section');
  const cloudDetails = el('div', 'save-cloud-details'); cloudDetails.hidden = true;
  const cloudLabel = el('div', 'field'); cloudLabel.append(el('span', '', '云端档案'));
  const options = [['', '创建新的云端档案']];
  let cloudSelect = customSelect(options, '', '云端档案'); cloudLabel.append(cloudSelect);
  const located = el('div', 'save-cloud-candidates');
  const locate = button('查找此云档案的本机目录', 'secondary', async () => {
    if (!cloudSelect.value) return;
    locate.disabled = true; error.textContent = '正在定位目录…';
    try {
      const result = await command('saves_locate', { gameId: game.id, cloudId: cloudSelect.value });
      located.replaceChildren();
      if (result.candidates.length) {
        const select = customSelect([['', '请选择并核对目录'], ...result.candidates.map(p => [p, p])], '', '云档案候选目录');
        select.addEventListener('change', () => { if (select.value) root.input.value = select.value; });
        located.append(select); error.textContent = '找到候选，请核对后选择。';
      } else error.textContent = '未找到目录，可手动选择或先运行游戏生成存档。';
    } catch (e) { error.textContent = String(e); }
    finally { locate.disabled = false; }
  });
  const syncCloud = () => { locate.hidden = !cloudSelect.value; located.replaceChildren(); };
  cloudSelect.addEventListener('change', syncCloud); syncCloud();
  const cloudActions = el('div', 'save-inline-actions'); cloudActions.append(locate);
  cloudDetails.append(cloudLabel, cloudActions, located,
    el('p', 'form-hint', '上传后自动清理过期版本：保留最近 10 份及近 7 天每天最后一份。当前版本、分叉和手动保留版本不会清理。'));
  // Existing cloud archives remain accessible for directory relocation without enabling upload.
  const advanced = el('details', 'save-cloud-advanced');
  advanced.append(el('summary', '', '关联已有云档案 / 从其他电脑定位'));
  const updateVisibility = () => { cloudDetails.hidden = !(upload.input.checked || advanced.open); };
  upload.input.addEventListener('change', updateVisibility);
  advanced.addEventListener('toggle', updateVisibility);
  cloud.append(el('h3', '', '云端设置'), upload.label, advanced, cloudDetails);
  form.append(directory, local, cloud, error);
  const dialog = modal('绑定存档目录', '先选择目录，再确认备份方式。识别不会修改或上传存档。', form);
  dialog.classList.add('save-bind-modal');
  dialog.addEventListener('close', discovery.dispose, { once: true });
  let bound;
  submit(form, '确认绑定', async () => {
    if (!root.input.value) throw new Error('请先选择存档目录');
    if (!bound) bound = await command('saves_bind', { gameId: game.id, name: name.input.value, root: root.input.value });
    if ((cloudSelect.value || upload.input.checked) && !bound.cloudId)
      bound = await command('saves_connect', { profileId: bound.id, cloudId: cloudSelect.value || null });
    await command('saves_automation_settings', { profileId: bound.id, autoBackup: backup.input.checked, autoUpload: upload.input.checked });
    await refresh(); await closeModal(dialog);
    if (bound.cloudId) await checkSaveBeforeLaunch(game);
  });
  try {
    const response = await command('saves_cloud_profiles', { gameId: game.id });
    if (!dialog.open) return;
    const replacement = customSelect([...options, ...(response.profiles || []).map(p => [p.id, p.name])], cloudSelect.value, '云端档案');
    cloudSelect.replaceWith(replacement); cloudSelect = replacement;
    cloudSelect.addEventListener('change', syncCloud); syncCloud();
  } catch { error.textContent = '暂未连接云存档，可先仅备份到本机。'; }
}
