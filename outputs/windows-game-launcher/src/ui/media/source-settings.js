import { el, button } from '../../lib/dom.js';
import { settingsToggle } from '../settings-components.js';
import { confirmAction } from './forms.js';
import { displayMediaPath } from '../../lib/media-path.js';

export function sourceSettings(controller) {
  const element = el('section', 'settings-service');
  const heading = el('div', 'settings-media-heading');
  heading.append(
    el('h3', '', '媒体目录'),
    button('添加目录', 'secondary', () => controller.addSource(), 'plus')
  );
  const list = el('div', 'settings-media-sources');
  element.append(
    heading,
    el('p', 'settings-description', '为电影和电视剧添加目录，离线时保留已有资料。'),
    el(
      'p',
      'form-hint',
      '在不需要扫描的目录中创建空文件 .youji-ignore（不要加 .txt 后缀），即可跳过该目录及全部子目录。删除标记后重新扫描可恢复；磁盘标记不列入应用内忽略列表。'
    ),
    list
  );
  let sourceKey;

  function update(sources) {
    const nextKey = JSON.stringify(sources);
    if (sourceKey === nextKey) return;
    sourceKey = nextKey;
    list.replaceChildren();
    for (const source of sources) {
      const row = el('section', 'settings-media-source');
      const copy = el('div', 'settings-media-source-copy');
      const title = el('div', 'settings-media-source-title');
      title.append(
        el('strong', '', source.label || displayMediaPath(source.path)),
        el('span', 'settings-description', source.available ? '可访问' : '离线')
      );
      copy.append(
        title,
        el('p', 'settings-description settings-media-path', displayMediaPath(source.path))
      );
      if (source.lastScan)
        copy.append(
          el('p', 'form-hint', `上次扫描：${new Date(source.lastScan).toLocaleString('zh-CN')}`)
        );
      if (source.error) copy.append(el('p', 'form-error', source.error));
      const tools = el('div', 'settings-tools');
      tools.append(
        button('浏览', 'secondary', () => controller.browse(source.id, ''), 'folder'),
        button('扫描', 'secondary', () => controller.scan(source.id), 'refresh'),
        button(
          `忽略项${source.ignoredCount ? ' · ' + source.ignoredCount : ''}`,
          'settings-link',
          () => controller.ignoredPaths(source.id)
        ),
        button(
          '移除媒体源',
          'settings-link',
          () =>
            confirmAction(
              '移除媒体目录',
              '移除该媒体源，并清理没有其他资源、个人记录或独立来源支撑的空条目；保留已有记录与独立收录，不删除磁盘文件。',
              async () => {
                await controller.run('media_remove_source', { sourceId: source.id });
                await controller.refresh();
              },
              '移除媒体源'
            ),
          'close'
        )
      );
      const smartFolders = settingsToggle(
        '智能文件夹',
        '将明确对应单部作品的目录折叠展示，更改立即保存。',
        source.smartFolders,
        async (value) => {
          await controller.run('media_add_source', { ...source, smartFolders: value });
          await controller.refresh();
        }
      );
      smartFolders
        .querySelector('input')
        .setAttribute('aria-label', `${source.label || displayMediaPath(source.path)}：智能文件夹`);
      row.append(copy, tools, smartFolders);
      list.append(row);
    }
    if (!sources.length) list.append(el('p', 'form-hint', '尚未添加媒体目录。'));
  }
  return { element, update };
}
