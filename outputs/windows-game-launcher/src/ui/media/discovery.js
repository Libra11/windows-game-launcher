import { el, button } from '../../lib/dom.js';
import { customSelect } from '../custom-select.js';
import { emptyState } from './components.js';
import { mediaItemTitle, safeImageUrl } from '../../lib/media-query.js';
import { cachedMediaArtwork, loadMediaArtwork } from '../../lib/media-artwork.js';
import {
  personWorks,
  personDepartment,
  peopleIndexMessage,
} from '../../lib/media-discovery.js';
import { statusFilters } from './filters.js';
import './discovery.css';

export function personPortrait(person, itemId, className = '') {
  const portrait = el('div', `media-credit-portrait ${className}`);
  portrait.setAttribute('aria-hidden', 'true');
  portrait.append(el('span', '', (person.name || '人').slice(0, 1)));
  const url = safeImageUrl(person.profileUrl);
  if (url) {
    const image = el('img');
    image.alt = '';
    image.loading = 'lazy';
    image.decoding = 'async';
    image.referrerPolicy = 'no-referrer';
    image.onerror = () => image.remove();
    portrait.append(image);
    const artwork = { id: itemId, personId: person.id, profileUrl: url };
    loadMediaArtwork(image, artwork, 'profile');
    image.src = cachedMediaArtwork(artwork, 'profile') || url;
  }
  return portrait;
}

function indexStatus(root, state) {
  const text = peopleIndexMessage(state.data.peopleIndex);
  if (!text) return;
  const message = el('p', 'media-discovery-status', text);
  message.setAttribute('role', 'status');
  root.append(message);
}

function appendWorks(root, items, controller, { list = false, credits = false } = {}) {
  const makeCard = (item) => {
    const card = controller.card(item, {
      list,
      displayTitle: mediaItemTitle(item, controller.state.data.items, controller.itemContext().byId),
    }, credits ? 'person' : 'search');
    if (credits && !card.querySelector('.media-person-credit')) {
      const jobs = {
        Director: '导演',
        Writer: '编剧',
        Screenplay: '编剧',
        'Executive Producer': '执行制片',
        Producer: '制片',
      };
      const roles = (item.personCredit?.roles || []).map((role) => jobs[role] || role);
      const label = roles.length
        ? roles.join(' / ')
        : item.personCredit?.departments?.includes('cast')
          ? '出演'
          : '参与制作';
      const detail = el('small', 'media-card-meta media-person-credit', label);
      detail.title = label;
      card.querySelector('.media-card-copy').append(detail);
    }
    return card;
  };
  let grid;
  if (credits && items.length > 120) grid = controller.personGrid.show(items, makeCard, list ? 'list' : 'grid');
  else {
    if (credits) controller.personGrid.hide();
    grid = el('div', list ? 'media-list' : 'media-grid');
    grid.append(...items.map(makeCard));
  }
  root.append(grid);
}

const SEARCH_PAGE_SIZE = 30;
function searchPagination(controller, kind, page, total) {
  const pages = Math.ceil(total / SEARCH_PAGE_SIZE);
  if (pages <= 1) return null;
  const nav = el('nav', 'media-search-pagination');
  nav.setAttribute('aria-label', kind === 'people' ? '演职员搜索结果分页' : '影视搜索结果分页');
  const previous = button('上一页', 'secondary', () => controller.setSearchPage(kind, page - 1));
  const next = button('下一页', 'secondary', () => controller.setSearchPage(kind, page + 1));
  previous.dataset.focusKey = `search-${kind}-previous`;
  next.dataset.focusKey = `search-${kind}-next`;
  const input = el('input', 'media-search-page-input');
  input.type = 'number';
  input.required = true;
  input.min = '1';
  input.max = String(pages);
  input.value = String(page + 1);
  input.dataset.focusKey = `search-${kind}-page`;
  input.setAttribute('aria-label', kind === 'people' ? '演职员结果页码' : '影视结果页码');
  const jump = button('跳转', 'secondary', () => {
    if (!input.reportValidity()) return;
    controller.setSearchPage(kind, Number(input.value) - 1);
  });
  jump.dataset.focusKey = `search-${kind}-jump`;
  input.onkeydown = (event) => {
    if (event.key === 'Enter' && !event.isComposing) { event.preventDefault(); jump.click(); }
  };
  previous.disabled = page === 0;
  next.disabled = page >= pages - 1;
  nav.append(previous, input, el('span', '', `/ ${pages} 页`), jump, next);
  return nav;
}

export function searchPage(controller) {
  const { state } = controller;
  const results = state.searchResults;
  const peoplePage = Math.min(state.searchPeoplePage || 0, Math.max(0, Math.ceil((results?.people.length || 0) / SEARCH_PAGE_SIZE) - 1));
  const itemsPage = Math.min(state.searchItemsPage || 0, Math.max(0, Math.ceil((results?.items.length || 0) / SEARCH_PAGE_SIZE) - 1));
  const people = results?.people.slice(peoplePage * SEARCH_PAGE_SIZE, (peoplePage + 1) * SEARCH_PAGE_SIZE) || [];
  const items = results?.items.slice(itemsPage * SEARCH_PAGE_SIZE, (itemsPage + 1) * SEARCH_PAGE_SIZE) || [];
  return controller.views.use('search-page', [state.searchQuery, state.view, state.searchPending, state.searchError, peoplePage, itemsPage,
    results?.people.length, results?.items.length, people, items, state.data.peopleIndex,
    items.map(item => [mediaItemTitle(item, state.data.items, controller.itemContext().byId), controller.posterKey(item)])], () => {
    const root = el('div', 'media-discovery');
    root.dataset.mediaSurface = 'search';
    const heading = el('div', 'media-search-heading');
    heading.append(el('p', 'media-discovery-subtitle', `全影视库搜索 · “${state.searchQuery}”`),
      button('刷新结果', 'secondary', () => controller.refreshSearch(), 'refresh'));
    root.append(heading);
    indexStatus(root, state);
    if (state.searchPending) {
      const loading = el('p', 'media-discovery-status', '正在搜索影视与演职员…');
      loading.setAttribute('role', 'status');
      root.append(loading);
      if (!results) return root;
    }
    if (state.searchError) {
      root.append(el('p', 'media-error', '搜索暂时无法完成，请重新搜索。'));
      if (!results) return root;
    }
    if (!results) return root;
    // 人员与作品分组展示，人员姓名命中不会被伪装成片名命中。
    if (results.people.length) {
      const section = el('section', 'media-search-group');
      const heading = el('div', 'media-section-heading');
      heading.append(el('h2', '', '演职员'), el('span', 'count-pill', String(results.people.length)));
      section.append(heading);
      const grid = el('div', 'media-people-grid');
      for (const person of people) {
        const works = personWorks(person, state.data.items, {}, controller.itemContext());
        const card = controller.views.use('search-person:' + person.id, [person, works.length, works.slice(0, 2).map(item => item.title)], () => {
          const card = button('', 'media-person-card', () => controller.openPerson(person));
          card.dataset.focusKey = `media-person-${person.id}`;
          card.setAttribute(
            'aria-label',
            `${person.name}，${personDepartment(person)}，库内 ${works.length} 部作品`
          );
          card.append(
            personPortrait(person, works[0]?.id),
            el('strong', '', person.name),
            el('small', '', `${personDepartment(person)} · 库内 ${works.length} 部`),
            el(
              'small',
              'media-person-known-for',
              works
                .slice(0, 2)
                .map((item) => item.title)
                .join('、')
            )
          );
          return card;
        });
        grid.append(card);
      }
      section.append(grid);
      const pagination = searchPagination(controller, 'people', peoplePage, results.people.length);
      if (pagination) section.append(pagination);
      root.append(section);
    }
    if (results.items.length) {
      const section = el('section', 'media-search-group');
      const heading = el('div', 'media-section-heading');
      heading.append(el('h2', '', '影视'), el('span', 'count-pill', String(results.items.length)));
      section.append(heading);
      appendWorks(section, items, controller, { list: state.view === 'list' });
      const pagination = searchPagination(controller, 'items', itemsPage, results.items.length);
      if (pagination) section.append(pagination);
      root.append(section);
    }
    if (!results.items.length && !results.people.length) {
      root.append(
        emptyState('没有找到相关内容', '试试其他片名、人员姓名或已收录的别名。搜索覆盖整个影视库。')
      );
    }
    return root;
  });
}

export function personPage(controller) {
  const { state } = controller;
  const person = state.data.people.find((value) => value.id === state.person?.id) || state.person;
  const all = personWorks(person, state.data.items, {}, controller.itemContext());
  const query = state.personQuery;
  const shown = personWorks(person, state.data.items, query, controller.itemContext());
  return controller.views.use('person-page', [person, all, query, state.data.peopleIndex, shown.map(item => [mediaItemTitle(item, state.data.items, controller.itemContext().byId), controller.posterKey(item)])], () => {
    const root = el('div', 'media-discovery');
    root.dataset.mediaSurface = 'person';
    const profile = el('div', 'media-person-summary');
    const copy = el('div');
    copy.append(
      el('p', 'media-discovery-subtitle', `${personDepartment(person)} · 库内 ${all.length} 部作品`),
      el('p', 'media-discovery-status', '仅展示资料库中的作品，包含没有本地资源的观影记录。')
    );
    profile.append(controller.views.use('person-avatar', [person.id, person.name, person.profileUrl, all[0]?.id], () => personPortrait(person, all[0]?.id, 'media-person-avatar')), copy);
    root.append(profile);
    indexStatus(root, state);
    const toolbar = el('div', 'collection-toolbar');
    const kinds = el('div', 'media-kind-tabs');
    kinds.setAttribute('role', 'group');
    kinds.setAttribute('aria-label', '作品类型');
    for (const [value, label] of [
      ['all', '全部'],
      ['movie', '电影'],
      ['series', '电视剧'],
    ]) {
      const tab = button(label, `secondary ${query.kind === value ? 'active' : ''}`, () => {
        query.kind = value;
        controller.render();
      });
      tab.setAttribute('aria-pressed', String(query.kind === value));
      tab.dataset.focusKey = `media-person-kind-${value}`;
      kinds.append(tab);
    }
    const controls = el('div', 'collection-controls media-library-controls');
    statusFilters(controls, query, controller.render, { recognized: false });
    const sort = customSelect(
      [
        ['az', '名称 A → Z'],
        ['za', '名称 Z → A'],
        ['rating', '个人评分优先'],
      ],
      query.sort,
      '作品排序',
      'sort-select'
    );
    sort.onchange = () => {
      query.sort = sort.value;
      controller.render();
    };
    controls.append(sort);
    const views = el('div', 'view-switch');
    for (const [value, glyph, label] of [
      ['grid', 'library', '封面视图'],
      ['list', 'list', '列表视图'],
    ]) {
      const toggle = button(
        '',
        `icon-button ${query.view === value ? 'active' : ''}`,
        () => {
          query.view = value;
          controller.render();
        },
        glyph
      );
      toggle.setAttribute('aria-label', label);
      toggle.setAttribute('aria-pressed', String(query.view === value));
      views.append(toggle);
    }
    controls.append(views);
    toolbar.append(kinds, el('span', 'count-pill', String(shown.length)), controls);
    root.append(toolbar);
    if (shown.length)
      appendWorks(root, shown, controller, { list: query.view === 'list', credits: true });
    else {
      controller.personGrid.hide();
      root.append(
        emptyState(
          '没有符合条件的作品',
          '可取消已观看或可观看筛选，查看此人的其他库内作品。',
          button('清除筛选', 'secondary', () => {
            Object.assign(query, { kind: 'all', watchedFilter: 'all', onlyPlayable: false });
            controller.render();
          })
        )
      );
    }
    return root;
  });
}
