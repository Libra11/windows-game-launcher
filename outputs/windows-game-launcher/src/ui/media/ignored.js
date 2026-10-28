import { el, button, icon } from '../../lib/dom.js';
import { displayMediaPath, mediaPathMatches } from '../../lib/media-path.js';
import { modal, closeModal } from '../modal.js';
import './ignored.css';

export function ignorePathDialog(controller, target) {
  const content = el('div', 'form');
  const path = el('p', 'media-ignored-path', displayMediaPath(target.path));
  content.append(
    path,
    el(
      'p',
      'form-hint',
      '不再扫描或识别此路径，清理没有个人记录及其他资源的关联条目。磁盘文件和已有观影记录保留。'
    )
  );
  let dialog;
  const actions = el('div', 'media-form-actions');
  actions.append(
    button('取消', 'secondary', () => closeModal(dialog)),
    button('忽略', 'primary', async () => {
      await controller.run('media_ignore_path', target);
      await closeModal(dialog);
      const state = controller.state;
      if (state.sourceId === target.sourceId && target.directory) {
        if (mediaPathMatches(target.path, state.browsePath)) {
          state.browsePath =
            target.parentPath ||
            state.data.sources.find((v) => v.id === target.sourceId)?.path ||
            '';
        }
      }
      await controller.refresh();
      await controller.refreshLocalFiles(target.sourceId);
      controller.toast('已忽略，可在忽略列表中取消');
    })
  );
  content.append(actions);
  dialog = modal(target.directory ? '忽略此目录及其内容' : '忽略此文件', '', content);
  return dialog;
}

export async function ignoredPathsDialog(controller, sourceId = '') {
  const content = el('div', 'media-ignored-list');
  let dialog;
  async function update() {
    const entries = await controller.run('media_ignored_paths', sourceId ? { sourceId } : {});
    if (dialog && !dialog.open) return;
    content.replaceChildren();
    const groups = new Map();
    for (const entry of entries) {
      if (!groups.has(entry.sourceId)) {
        const group = el('section', 'media-ignored-group');
        const heading = el('div', 'media-ignored-heading');
        heading.append(
          el('strong', '', entry.sourceLabel),
          el('small', 'media-ignored-path', displayMediaPath(entry.sourcePath))
        );
        group.append(heading);
        groups.set(entry.sourceId, group);
        content.append(group);
      }
      const row = el('div', 'media-ignored-row');
      const copy = el('div', 'media-ignored-copy');
      const full = displayMediaPath(entry.path);
      const root = displayMediaPath(entry.sourcePath).replace(/[\\/]+$/, '');
      const relative = full.slice(root.length).replace(/^[\\/]+/, '');
      const path = el('span', 'media-ignored-path', relative || full);
      path.title = full;
      const description = el(
        'small',
        'form-hint',
        `${entry.directory ? '目录及其内容' : '文件'}${entry.covered ? ' · 上级目录仍被忽略' : ''}`
      );
      copy.append(path, description);
      const restore = button(entry.covered ? '取消此规则' : '取消并扫描', 'secondary', async () => {
        await controller.run('media_ignore_path', {
          sourceId: entry.sourceId,
          path: entry.path,
          remove: true,
        });
        await update();
        const source = controller.state.data.sources.find((v) => v.id === entry.sourceId);
        if (!entry.covered && source?.available) {
          try {
            const result = await controller.run(
              'media_scan',
              { sourceId: entry.sourceId, afterCurrent: true },
              false
            );
            controller.toast(
              result.queued ? '已取消忽略，将在当前扫描结束后重新扫描' : '已取消忽略，扫描已启动'
            );
          } catch (error) {
            controller.toast(
              String(error).includes('正在扫描')
                ? '已取消忽略，当前扫描完成后可重新扫描'
                : `已取消忽略，扫描未启动：${error}`,
              true
            );
          }
        } else
          controller.toast(
            entry.covered ? '已取消此规则，上级目录仍被忽略' : '已取消忽略，目录恢复连接后可扫描'
          );
        await controller.refresh();
        await controller.refreshLocalFiles(entry.sourceId);
      });
      row.append(icon(entry.directory ? 'folder' : 'play'), copy, restore);
      groups.get(entry.sourceId).append(row);
    }
    if (!entries.length)
      content.append(el('p', 'form-hint', '暂无忽略项。在本地影视的文件或目录上选择“忽略”。'));
    if (dialog?.open && !dialog.contains(document.activeElement))
      (content.querySelector('button') || dialog.querySelector('button'))?.focus();
  }
  await update();
  dialog = modal('忽略列表', '取消忽略后重新扫描，文件会重新收录。', content);
  return dialog;
}
