import { el, button, icon } from '../lib/dom.js';
import { command, preview } from '../lib/bridge.js';
import { setImageSource } from '../lib/network-images.js';
import { field } from './form-fields.js';
import './steam-game-search.css';

export function steamGameSearch(initial = null) {
  const element = el('section', 'steam-game-search');
  const name = field('游戏名称', '', '输入中文或英文名称，搜索 Steam 游戏');
  name.input.required = true; name.input.maxLength = 100;
  const row = el('div', 'input-row'); name.wrapper.replaceChild(row, name.input);
  const search = el('button', 'secondary steam-search-button'); search.type = 'button';
  search.append(icon('search'), el('span', '', '搜索'));
  row.append(name.input, search);
  const status = el('p', 'steam-search-status', '输入名称后，从 Steam 候选中选择正确的游戏。');
  status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  const results = el('div', 'steam-search-results');
  results.setAttribute('aria-label', 'Steam 游戏候选');
  element.append(name.wrapper, status, results);
  let selected = null, timer, version = 0, disposed = false, localOnly = false;
  let composing = false;

  function invalidate() {
    clearTimeout(timer); version++; selected = null;
    results.replaceChildren(); results.removeAttribute('aria-busy');
    status.classList.remove('error');
    element.dispatchEvent(new Event('change'));
  }

  function select(game) {
    invalidate(); selected = game; name.input.value = game.name;
    status.textContent = '已匹配 Steam，保存后将获取封面、简介和成就定义。';
    const chosen = candidate(game);
    chosen.classList.add('selected');
    const change = button('重新选择', 'steam-search-change', () => {
      invalidate(); name.input.focus(); return find();
    });
    const summary = el('div', 'steam-search-selection');
    summary.append(chosen, change); results.append(summary);
    element.dispatchEvent(new Event('change'));
  }

  function candidate(game, selectable = false) {
    const item = selectable ? button('', 'steam-search-result', () => select(game)) : el('div', 'steam-search-result');
    const art = el('div', 'steam-search-art'); art.append(icon('game'));
    if (game.image) {
      const image = el('img'); image.alt = ''; image.loading = 'lazy';
      image.onerror = () => { image.hidden = true; }; setImageSource(image, game.image); art.append(image);
    }
    const copy = el('div', 'steam-search-copy');
    copy.append(el('strong', '', game.name), el('span', '', `Steam 官方资料 · AppID ${game.appid}`));
    item.append(art, copy, icon(selectable ? 'plus' : 'check'));
    if (selectable) item.setAttribute('aria-label', `选择 ${game.name}，AppID ${game.appid}`);
    return item;
  }

  async function find() {
    clearTimeout(timer);
    if (disposed || localOnly || composing) return;
    const query = name.input.value.trim();
    invalidate();
    if (!query) { status.textContent = '输入名称后，从 Steam 候选中选择正确的游戏。'; return; }
    const request = version;
    status.textContent = '正在搜索 Steam…'; results.setAttribute('aria-busy', 'true');
    try {
      const games = await command('search_steam_games', { query });
      if (disposed || request !== version) return;
      status.textContent = games.length
        ? `${preview ? '预览示例 · ' : ''}找到 ${games.length} 个候选，请确认名称与版本；结果可能包含 DLC 或原声带。`
        : '没有找到匹配项，试试英文名称或更短的关键词，也可以勾选“跳过 Steam 资料关联”。';
      results.replaceChildren(...games.map(game => candidate(game, true)));
    } catch (error) {
      if (disposed || request !== version) return;
      status.textContent = `搜索失败：${String(error)}。点击搜索重试。`;
      status.classList.add('error');
    } finally {
      if (!disposed && request === version) results.removeAttribute('aria-busy');
    }
  }

  function changed() {
    invalidate();
    if (localOnly) { status.textContent = '请输入游戏名称；不获取 Steam 封面、简介和成就资料，Xbox 成就仍可单独关联。'; return; }
    status.textContent = name.input.value.trim() ? '等待搜索…' : '输入名称后，从 Steam 候选中选择正确的游戏。';
    if (!composing && name.input.value.trim()) timer = setTimeout(find, 400);
  }
  name.input.addEventListener('input', changed);
  name.input.addEventListener('compositionstart', () => { composing = true; invalidate(); });
  name.input.addEventListener('compositionend', () => { composing = false; changed(); });
  name.input.addEventListener('keydown', event => {
    if (event.isComposing || composing) return;
    if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); find(); }
    if (event.key === 'ArrowDown' && results.querySelector('button.steam-search-result')) {
      event.preventDefault(); results.querySelector('button.steam-search-result').focus();
    }
  });
  results.addEventListener('keydown', event => {
    if (!['ArrowDown', 'ArrowUp'].includes(event.key)) return;
    const items = [...results.querySelectorAll('button.steam-search-result')];
    const index = items.indexOf(document.activeElement); if (index < 0) return;
    event.preventDefault();
    const next = index + (event.key === 'ArrowDown' ? 1 : -1);
    if (next < 0) name.input.focus(); else items[Math.min(next, items.length - 1)].focus();
  });
  search.addEventListener('click', find);

  if (initial) {
    if (initial.appid) select({ appid:initial.appid, name:initial.title, image:initial.image || '' });
    else {
      name.input.value = initial.title;
      name.input.dispatchEvent(new Event('input', { bubbles: true }));
    }
  }

  return {
    element,
    get title() { return name.input.value.trim(); },
    get appid() { return selected?.appid || ''; },
    setLocalOnly(value) {
      localOnly = value; search.disabled = value; changed();
    },
    dispose() { disposed = true; invalidate(); },
  };
}
