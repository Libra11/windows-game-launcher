import { el, button, icon } from '../lib/dom.js';
import { setImageSource } from '../lib/network-images.js';
import { createOnlineSearchController, gamer520SearchUrl } from '../lib/online-search.js';
import './online-search.css';
import { createOnlineDetailPage } from './online-detail.js';
import { onlineGamePresentation, onlineGameType } from '../lib/online-game-presentation.js';

export function createOnlineSearchPage({ command, preview, toast }) {
  const controller = createOnlineSearchController(command);
  const { state } = controller;
  const root = el('div', 'online-search-page');
  const shell = el('div','online-search-shell'); shell.append(root);
  let detailPage, returnScroll = 0, returnFocus = '';
  function showDetail(item) {
    returnScroll = window.scrollY; returnFocus = `online-result-${item.id}`;
    detailPage ||= createOnlineDetailPage({command,preview,toast,back:()=>{
      root.hidden=false; detailPage.element.hidden=true; window.scrollTo(0,returnScroll);
      if(document.hasFocus())[...root.querySelectorAll('[data-focus-key]')].find(node=>node.dataset.focusKey===returnFocus)?.focus({preventScroll:true});
    }});
    if(!detailPage.element.parentElement)shell.append(detailPage.element);
    root.hidden=true; detailPage.element.hidden=false;window.scrollTo(0,0);
    if(document.hasFocus())detailPage.element.querySelector('[data-focus-key="online-detail-back"]').focus({preventScroll:true});
    detailPage.load(item);
  }
  const heading = el('div', 'online-search-heading');
  const copy = el('div');
  copy.append(el('h1', '', '在线搜索'), el('p', '', preview ? 'Gamer520 · 界面示例预览' : 'Gamer520 · 游戏资料与下载资源'));
  heading.append(copy); root.append(heading);
  const form = el('form', 'online-search-form');
  const field = el('label', 'search-box'); field.append(icon('search'));
  const input = el('input'); input.type = 'search'; input.maxLength = 200;
  input.placeholder = '输入游戏名称，例如空洞骑士'; input.setAttribute('aria-label', '搜索 Gamer520 游戏');
  input.dataset.focusKey = 'online-search-input';
  input.oninput = () => { state.draft = input.value; };
  field.append(input);
  const submit = el('button', 'primary'); submit.type = 'submit'; submit.append(icon('search'), el('span', '', '搜索'));
  submit.dataset.focusKey = 'online-search-submit'; form.append(field, submit);
  form.onsubmit = event => { event.preventDefault(); controller.search(); };
  heading.append(form);
  const toolbar = el('div', 'collection-toolbar online-search-toolbar');
  const title = el('div', 'collection-title'); const resultTitle = el('h2', '', '搜索结果');
  const count = el('span', 'count-pill', '0'); title.append(resultTitle, count);
  const resultQuery=el('span','online-search-query');title.append(resultQuery);
  const filters = el('div', 'filter-tabs');
  const filterButtons = [['pc','PC 游戏'],['all','全部类型']].map(([value,label]) => {
    const control = button(label, '', () => controller.scope(value));
    control.dataset.focusKey = `online-scope-${value}`; filters.append(control); return [value,control];
  });
  toolbar.append(title, filters); root.append(toolbar);
  const status = el('div', 'online-search-status'); status.setAttribute('role','status'); status.setAttribute('aria-live','polite');
  const results = el('div', 'online-search-results');
  const footer = el('div', 'online-search-footer');
  const pageLabel = el('span');
  const previous = button('上一页','secondary',()=>controller.page(state.data.page - 1),'back');
  const next = button('下一页','secondary',()=>controller.page(state.data.page + 1),'arrow');
  const original = button('打开原站搜索','secondary',()=>open(state.data.searchUrl),'network');
  previous.dataset.focusKey = 'online-page-prev'; next.dataset.focusKey = 'online-page-next';
  footer.append(pageLabel,previous,next,original);
  root.append(status, results, footer);
  let displayedData, displayedScope;
  async function open(url) {
    try {
      if (preview) window.open(url, '_blank', 'noopener,noreferrer');
      else await command('open_gamer520_page', { url });
    } catch (error) { toast(String(error), true); }
  }
  function empty(title, description) {
    const node = el('div', 'empty-state');
    node.append(icon('search'), el('h2','',title), el('p','',description)); return node;
  }
  function card(item) {
    const presentation=onlineGamePresentation(item.title);
    const tile = el('div', 'online-result-tile');
    const control = button('', 'online-result-card', () => showDetail(item));
    control.dataset.focusKey = `online-result-${item.id}`;
    control.setAttribute('aria-label', `${item.title}，查看详情与下载地址`);
    const art = el('div', 'online-result-art'); const placeholder = el('span', 'online-search-placeholder'); placeholder.append(icon('game')); art.append(placeholder);
    if (item.image) {
      const image = el('img'); image.alt = ''; image.loading = 'lazy'; image.decoding = 'async'; image.referrerPolicy = 'no-referrer';
      image.onload = () => art.classList.add('art-loaded');
      image.onerror = () => { image.hidden = true; art.classList.remove('art-loaded'); };
      setImageSource(image,item.image); art.append(image);
    }
    const copy = el('div', 'online-result-copy');
    const meta=el('div','online-result-meta');meta.append(el('span','online-result-type',onlineGameType(item)));
    const date = new Date(item.publishedAt);
    if (item.publishedAt && !Number.isNaN(date.getTime())) {
      const time = el('time','online-search-date',date.toLocaleDateString('zh-CN'));
      time.dateTime = item.publishedAt; meta.append(time);
    }
    copy.append(meta,el('h3','',presentation.name));
    if(presentation.edition){const edition=el('p','online-search-version',presentation.edition);edition.title=presentation.edition;copy.append(edition);}
    const bottom=el('div','online-result-bottom');const tags=el('div','online-result-tags');
    for(const label of presentation.badges.slice(0,3))tags.append(el('span','',label));
    const next=el('span','online-result-next');next.append(icon('arrow'));bottom.append(tags,next);copy.append(bottom);
    control.append(art,copy); tile.append(control); return tile;
  }
  function update() {
    submit.disabled = state.loading; submit.lastElementChild.textContent = state.loading ? '搜索中…' : '搜索';
    results.setAttribute('aria-busy',String(state.loading));
    for (const [value,control] of filterButtons) {
      control.classList.toggle('active',value === state.scope); control.setAttribute('aria-pressed',String(value === state.scope));
    }
    const shown = state.data?.items.filter(item=>state.scope === 'all' || item.isPc) || [];
    count.textContent = String(shown.length);
    resultTitle.textContent = '搜索结果';
    resultQuery.textContent=state.data?`“${state.data.query}”`:'';
    count.hidden=!state.data;
    status.classList.toggle('error',!!state.error); status.replaceChildren();
    if (state.error) {
      status.append(el('p','',state.error),button('重试','secondary',controller.retry,'refresh'),button('打开原站搜索','secondary',()=>open(gamer520SearchUrl(state.requested?.query || state.draft)),'network'));
      if (state.data) status.append(el('p','online-search-retained','下方保留上次成功搜索的结果。'));
    } else if (state.loading) status.append(el('p','',`正在搜索“${state.requested.query}” · 第 ${state.requested.page} 页…`));
    if (displayedData !== state.data || displayedScope !== state.scope || !results.firstChild) {
      displayedData = state.data; displayedScope = state.scope;
      if (!state.data) results.replaceChildren(empty('搜索你想找的游戏','输入名称并按 Enter，搜索结果将在这里展示。'));
      else if (!state.data.items.length) results.replaceChildren(empty('没有找到相关条目','试试中文名称、英文名称或更短的关键词。'));
      else if (!shown.length) results.replaceChildren(empty('当前页没有 PC 游戏','可切换全部类型，或翻页查看其他搜索结果。'));
      else {
        const grid = el('div','online-search-grid'); shown.forEach(item=>grid.append(card(item))); results.replaceChildren(grid);
      }
    }
    footer.hidden = !state.data;
    if (state.data) {
      pageLabel.textContent = `第 ${state.data.page} 页 · 当前页 ${shown.length} 项 / 原站 ${state.data.items.length} 项`;
      previous.disabled = state.loading || state.data.page <= 1; next.disabled = state.loading || !state.data.hasNext;
    }
  }
  const unsubscribe = controller.subscribe(update);
  return { element:shell, dispose:()=>{unsubscribe();detailPage?.cancel();} };
}
