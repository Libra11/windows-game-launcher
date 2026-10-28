import { el, button } from '../../lib/dom.js';
import { modal, closeModal } from '../modal.js';
import { field, input, mediaArtwork } from './components.js';

export function doubanMetadataDialog(controller, item) {
  const content = el('div', 'form');
  const source = item.externalIds?.find(value => value.provider === 'douban');
  const subject = input(source?.externalId || '', 'text', 'https://movie.douban.com/subject/…/ 或作品编号');
  subject.setAttribute('aria-label', '豆瓣作品链接或编号');
  const form = el('form', 'input-row');
  const preview = el('div', 'media-douban-preview');
  preview.setAttribute('aria-live', 'polite');
  const error = el('p', 'media-error');
  error.setAttribute('role', 'status');
  let token = '', generation = 0, busy = false, dialog;
  const confirm = button('确认使用豆瓣资料', 'primary', async () => {
    if (!token || busy) return;
    busy = true;
    confirm.disabled = fetch.disabled = subject.disabled = true;
    error.textContent = '';
    try {
      await controller.run('media_douban_metadata', { action: 'apply', itemId: item.id, token }, false);
      await closeModal(dialog);
      await controller.refreshDetail();
    } catch (failure) { error.textContent = String(failure); }
    finally { busy = false; confirm.disabled = !token; fetch.disabled = subject.disabled = false; }
  });
  confirm.disabled = true;
  const fetch = button('获取预览', 'secondary', async () => {
    if (busy || !subject.value.trim()) return;
    const version = ++generation;
    busy = true;
    token = '';
    confirm.disabled = fetch.disabled = subject.disabled = true;
    error.textContent = '';
    preview.replaceChildren(el('p', 'form-hint', '正在读取豆瓣作品资料…'));
    try {
      const result = await controller.run('media_douban_metadata', {
        action: 'preview', itemId: item.id, subject: subject.value.trim(),
      }, false);
      if (!dialog.open || version !== generation) return;
      const detail = result.detail;
      token = result.token;
      const copy = el('div');
      copy.append(el('h3', '', detail.title), el('p', 'form-hint', [
        detail.year, detail.kind === 'season' ? '电视剧 · 季级资料' : '电影',
        detail.seasonNumber != null ? `第 ${detail.seasonNumber} 季` : '',
        ...(detail.genres || []),
      ].filter(Boolean).join(' · ')), el('p', '', detail.overview || '暂无简介'));
      preview.replaceChildren(mediaArtwork({ ...detail, id: item.id, artworkPreviewToken: token }), copy);
    } catch (failure) {
      if (dialog.open && version === generation) { preview.replaceChildren(); error.textContent = String(failure); }
    } finally {
      busy = false;
      fetch.disabled = subject.disabled = false;
      confirm.disabled = !token;
    }
  }, 'search');
  subject.oninput = () => { token = ''; confirm.disabled = true; preview.replaceChildren(); error.textContent = ''; };
  form.append(subject, fetch);
  form.onsubmit = event => { event.preventDefault(); fetch.click(); };
  const actions = el('div', 'media-form-actions');
  actions.append(button('取消', 'secondary', () => closeModal(dialog)), confirm);
  content.append(field('豆瓣作品', form), el('p', 'form-hint',
    '用于 TMDB 缺少的作品。确认后标为已识别；保留文件、播放进度和个人记录。电视剧只保存季级资料，不创建分集。'), preview, error, actions);
  dialog = modal('从豆瓣获取作品资料', item.title, content);
  dialog.classList.add('media-identify-modal');
  dialog.addEventListener('close', () => generation++);
  return dialog;
}
