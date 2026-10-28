import { el, button } from '../../lib/dom.js';
import { modal, closeModal } from '../modal.js';
import { customSelect } from '../custom-select.js';
import { field, input, check } from './components.js';

export function addSourceDialog(controller) {
  const content = el('form', 'form');
  const path = input('', 'text', '选择电影或电视剧所在目录');
  path.required = true;
  const label = input('', 'text', '例如：移动硬盘、家庭电影');
  const kind = customSelect(
    [
      ['auto', '自动判断'],
      ['movies', '电影'],
      ['tv', '电视剧'],
      ['other', '其他'],
    ],
    'auto',
    '媒体目录类型'
  );
  const smart = check('智能文件夹：将明确对应单部作品的目录折叠展示', true);
  const row = el('div', 'input-row');
  row.append(
    path,
    button(
      '选择目录',
      'secondary',
      async () => {
        const chosen = await controller.chooseFile({
          directory: true,
          multiple: false,
          title: '选择影视媒体目录',
        });
        if (typeof chosen === 'string') path.value = chosen;
      },
      'folder'
    )
  );
  content.append(
    field('媒体目录', row),
    field('显示名称', label),
    field('目录内容', kind),
    smart,
    el('p', 'form-hint', '添加后后台扫描并尝试识别。扫描不会移动、改名或删除视频。')
  );
  let dialog;
  const save = button('添加并扫描', 'primary', async () => {
    if (!path.value.trim()) {
      path.reportValidity();
      return;
    }
    await controller.run('media_add_source', {
      path: path.value.trim(),
      label: label.value.trim(),
      kind: kind.value,
      smartFolders: smart.checkbox.checked,
    });
    await closeModal(dialog);
    await controller.refresh();
  });
  const actions = el('div', 'media-form-actions');
  actions.append(
    button('取消', 'secondary', () => closeModal(dialog)),
    save
  );
  content.append(actions);
  content.onsubmit = (event) => {
    event.preventDefault();
    save.click();
  };
  dialog = modal('添加媒体目录', '媒体源独立于影视条目，可随时重新扫描', content);
  return dialog;
}
