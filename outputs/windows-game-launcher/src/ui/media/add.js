import { el, button } from '../../lib/dom.js';
import { modal, closeModal } from '../modal.js';
import { customSelect } from '../custom-select.js';
import { input, mediaArtwork, emptyState } from './components.js';
import { mediaKindName } from '../../lib/media-query.js';

export function newItemDialog(controller) {
  const content = el('div', 'form');
  const row = el('form', 'input-row field');
  const query = input('', 'search', '输入影视名称、原名或别名');
  query.required = true;
  query.maxLength = 200;
  query.setAttribute('aria-label', '影视名称');
  const kind = customSelect(
    [
      ['all', '全部类型'],
      ['movie', '电影'],
      ['series', '电视剧'],
    ],
    'all',
    '搜索类型'
  );
  const results = el('div', 'media-identify-results');
  results.setAttribute('aria-live', 'polite');
  results.append(el('p', 'form-hint', '搜索后选择电影或具体季，添加到影视库。'));
  let dialog,
    generation = 0,
    adding = false;
  const invalidate = () => {
    generation++;
    results.replaceChildren(el('p', 'form-hint', '搜索后选择电影或具体季，添加到影视库。'));
  };
  query.oninput = invalidate;
  kind.addEventListener('change', invalidate);

  async function search() {
    if (adding || !query.value.trim()) {
      if (!query.value.trim()) query.reportValidity();
      return;
    }
    const version = ++generation;
    results.replaceChildren(el('p', 'form-hint', '正在搜索 TMDB…'));
    try {
      const response = await controller.run(
        'media_search_metadata',
        {
          query: query.value.trim(),
          kind: kind.value,
        },
        false
      );
      if (version !== generation || !dialog?.open) return;
      const candidates = (
        Array.isArray(response) ? response : response.results || response.items || []
      ).filter(
        (candidate) =>
          candidate.kind === 'movie' ||
          (candidate.kind === 'season' && Number.isInteger(candidate.seasonNumber))
      );
      results.replaceChildren();
      if (!candidates.length) {
        results.append(emptyState('没有找到匹配影视', '试试原名、别名或去掉年份后重新搜索。'));
        return;
      }
      for (const candidate of candidates) {
        const choice = button('', 'media-identify-result', async () => {
          if (adding) return;
          adding = true;
          query.disabled = true;
          for (const control of content.querySelectorAll('button')) control.disabled = true;
          try {
            const item = await controller.run('media_create_item', {
              candidate,
            });
            await closeModal(dialog);
            await controller.refresh();
            const id = item.itemId || item.id || item.item?.id;
            if (id) await controller.open({ id });
          } finally {
            adding = false;
            query.disabled = false;
            for (const control of content.querySelectorAll('button')) control.disabled = false;
          }
        });
        const copy = el('div');
        copy.append(
          el('strong', '', candidate.title || candidate.name),
          el('small', '', `${candidate.year || '年份未知'} · ${mediaKindName(candidate.kind)}`),
          el('p', '', candidate.overview || '暂无简介')
        );
        choice.append(mediaArtwork(candidate), copy);
        results.append(choice);
      }
    } catch {
      if (version === generation && dialog?.open)
        results.replaceChildren(el('p', 'media-error', '搜索失败，请检查 TMDB 凭据和网络后重试。'));
    }
  }
  row.append(query, button('搜索', 'primary', search, 'search'));
  row.onsubmit = (event) => {
    event.preventDefault();
    search();
  };
  content.append(row, kind, results);
  dialog = modal('添加影视剧', '从 TMDB 选择作品，无需本地文件；已收录作品会直接打开。', content);
  dialog.classList.add('media-identify-modal');
  dialog.addEventListener('close', () => generation++);
  return dialog;
}
