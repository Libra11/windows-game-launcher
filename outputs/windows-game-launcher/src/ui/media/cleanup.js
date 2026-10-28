import { el, button } from '../../lib/dom.js';
import { mediaItemTitle, mediaKindName } from '../../lib/media-query.js';
import { modal, closeModal } from '../modal.js';
import './cleanup.css';

export function cleanupDialog(controller, item) {
  const content = el('div', 'form media-cleanup');
  const previewHost = el('div', 'media-cleanup-preview');
  const error = el('p', 'media-error');
  error.setAttribute('role', 'alert');
  const actions = el('div', 'media-form-actions');
  let dialog, preview = null, generation = 0;
  const confirm = button('确认清理', 'secondary media-cleanup-confirm', save);
  confirm.disabled = true;
  const reload = button('重新预览', 'secondary', load);
  actions.append(button('取消', 'secondary', () => closeModal(dialog)), reload, confirm);
  content.append(previewHost, error, actions);
  dialog = modal('清理条目', mediaItemTitle(item, controller.state.data.items), content);
  dialog.addEventListener('close', () => generation++, { once: true });
  async function load() {
    const version = ++generation;
    preview = null; confirm.hidden = false; confirm.disabled = true; error.textContent = '';
    previewHost.replaceChildren(el('p', 'form-hint', '正在核对条目、子项和相关记录…'));
    try {
      const value = await controller.run('media_cleanup', { action: 'preview', itemId: item.id }, false);
      if (!dialog.open || version !== generation) return;
      preview = value;
      previewHost.replaceChildren(el('p', 'form-hint', `将从影视库移除 ${value.summary.items} 项，包含所选条目、子项及 ${value.summary.parents} 个空父级。不会删除磁盘文件。`));
      const list = el('ul', 'media-cleanup-list');
      for (const entry of value.entries) {
        const row = el('li');
        const kind = { series: '整剧', season: '季', episode: '单集' }[entry.kind] || mediaKindName(entry.kind);
        row.append(el('strong', '', entry.title), el('small', 'form-hint', `${kind}${entry.scope === 'emptyParent' ? ' · 空父级' : entry.scope === 'child' ? ' · 子项' : ''}`));
        list.append(row);
      }
      previewHost.append(list);
      const summary = value.summary;
      const records = el('div', 'media-cleanup-summary');
      records.append(
        el('p', 'form-hint', `观影记录：${summary.personal} 项（已看 ${summary.watched}、评分 ${summary.ratings}、短评 ${summary.comments}、收藏 ${summary.favorites}）`),
        el('p', 'form-hint', `观看历史 ${summary.sessions} 次 · 豆瓣观看记录 ${summary.doubanRecords} 条`),
        el('p', 'form-hint', `收藏夹归属 ${summary.collectionLinks} 项 · 标签归属 ${summary.tagLinks} 项`)
      );
      previewHost.append(records);
      if (!value.allowed) {
        error.textContent = value.blockedReason;
        return;
      }
      if (value.hasDoubanSource) previewHost.append(el('p', 'form-hint', '清理库内条目及其来源记录，不修改豆瓣账号内容；之后重新导入时可能再次收录。'));
      if (value.hasRecords) {
        const check = el('input'); check.type = 'checkbox';
        const label = el('label', 'media-cleanup-check');
        label.append(check, document.createTextNode('同时删除相关记录（观影历史、评分、短评、收藏与标签归属）'));
        check.onchange = () => { confirm.disabled = !check.checked; };
        previewHost.append(label);
      } else confirm.disabled = false;
      previewHost.append(el('p', 'form-hint', '确认后无法直接撤销。'));
    } catch (e) {
      if (dialog.open && version === generation) { previewHost.replaceChildren(); error.textContent = String(e); }
    }
  }
  async function save() {
    if (!preview?.allowed) return;
    error.textContent = '';
    const checked = !!previewHost.querySelector('input[type=checkbox]')?.checked;
    if (preview.hasRecords && !checked) return;
    content.inert = true;
    let result;
    try {
      result = await controller.run('media_cleanup', { action: 'apply', itemId: item.id, fingerprint: preview.fingerprint, deleteRecords: checked }, false);
    } catch (e) {
      preview = null; confirm.hidden = true; confirm.disabled = true; error.textContent = String(e);
      return;
    } finally { content.inert = false; }
    const removedIds = preview.entries.map(entry => entry.id);
    await closeModal(dialog);
    controller.toast(`已清理 ${result.removed} 项条目`);
    await controller.afterCleanup(removedIds);
  }
  load();
  return dialog;
}
