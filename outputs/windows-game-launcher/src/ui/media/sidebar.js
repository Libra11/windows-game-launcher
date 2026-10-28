import { el, button } from '../../lib/dom.js';
import { mediaCategories, queryMedia } from '../../lib/media-query.js';
import { sidebarBrand } from '../sidebar-brand.js';
import { sidebarBottom } from '../sidebar-bottom.js';

export function mountMediaSidebar(
  host,
  controller,
  { onDomainChange, onSettings, preview = false } = {}
) {
  const brand = sidebarBrand('media', { back: () => controller.navigate('all'), onDomainChange });
  const nav = el('nav');
  nav.id = 'media-navigation';
  nav.setAttribute('aria-label', '影视主导航');
  for (const [label, categories] of [
    ['', ['all', 'recent', 'added', 'favorites']],
    ['分类', ['movies', 'series', 'watched', 'unwatched', 'other']],
  ]) {
    const group = el('div', 'nav-group');
    group.setAttribute('role', 'group');
    group.setAttribute('aria-label', label || '影视库');
    if (label) group.append(el('div', 'nav-group-label', label));
    for (const [value, title, glyph] of mediaCategories.filter(([value]) =>
      categories.includes(value)
    )) {
      const node = button(title, 'nav-item', () => controller.navigate(value), glyph);
      node.dataset.mediaCategory = value;
      node.title = title;
      node.append(el('span', 'nav-count', '0'));
      group.append(node);
    }
    nav.append(group);
  }
  const collections = el('div', 'nav-group organization-nav');
  const header = el('div', 'organization-nav-heading');
  const toggle = button(
    '收藏夹',
    'nav-group-label',
    () => {
      controller.state.collectionsCollapsed = !controller.state.collectionsCollapsed;
      update();
    },
    'chevron'
  );
  toggle.setAttribute('aria-expanded', 'true');
  header.append(
    toggle,
    button('', 'icon-button', () => controller.newCollection(), 'plus'),
    button('', 'icon-button', () => controller.manageCollections(), 'settings')
  );
  header.children[1].title = '新建收藏夹';
  header.children[1].setAttribute('aria-label', '新建收藏夹');
  header.children[2].title = '管理收藏夹';
  header.children[2].setAttribute('aria-label', '管理收藏夹');
  const list = el('div', 'organization-nav-list');
  collections.append(header, list);
  nav.append(collections);
  const features = el('div', 'nav-group');
  features.append(el('div', 'nav-group-label', '功能'));
  for (const [value, title, glyph] of mediaCategories.filter(([value]) =>
    ['files', 'history'].includes(value)
  )) {
    const node = button(title, 'nav-item', () => controller.navigate(value), glyph);
    node.dataset.mediaCategory = value;
    node.title = title;
    features.append(node);
  }
  nav.append(features);
  const bottom = sidebarBottom({ settings: onSettings, preview });
  host.replaceChildren(brand, nav, bottom);
  let previousView = '', previousCollectionsView = '', previousItems = null;
  function update() {
    const state = controller.state,
      data = state.data;
    const signature = JSON.stringify([state.category, state.collectionId, state.collectionsCollapsed,
      state.collectionsExpanded, data.collections]);
    const countsChanged = previousItems !== data.items;
    if (signature === previousView && !countsChanged) return;
    previousView = signature;
    previousItems = data.items;
    for (const node of nav.querySelectorAll('[data-media-category]')) {
      const value = node.dataset.mediaCategory;
      const active = !state.collectionId && state.category === value;
      node.classList.toggle('active', active);
      node.setAttribute('aria-current', active ? 'page' : 'false');
      const count = node.querySelector('.nav-count');
      if (count && countsChanged)
        count.textContent = String(
          queryMedia(data.items, { category: value, sort: 'none' }, data.collections).length
        );
    }
    toggle.setAttribute('aria-expanded', String(!state.collectionsCollapsed));
    const collectionsSignature = JSON.stringify([state.collectionId, state.collectionsCollapsed,
      state.collectionsExpanded, data.collections]);
    if (collectionsSignature === previousCollectionsView) return;
    previousCollectionsView = collectionsSignature;
    list.replaceChildren();
    if (state.collectionsCollapsed) return;
    const shown = state.collectionsExpanded ? data.collections : data.collections.slice(0, 4);
    const current = data.collections.find((item) => item.id === state.collectionId);
    if (current && !shown.some((item) => item.id === current.id)) shown.push(current);
    for (const collection of shown) {
      const node = button(
        collection.name,
        'nav-item',
        () => controller.navigate('all', collection.id),
        'collection'
      );
      node.dataset.mediaCollection = collection.id;
      node.classList.toggle('active', state.collectionId === collection.id);
      node.setAttribute('aria-current', state.collectionId === collection.id ? 'page' : 'false');
      node.append(el('span', 'nav-count', String(collection.itemIds?.length || 0)));
      node.ondragover = (event) => {
        if (event.dataTransfer.types.includes('application/x-youji-media-items')) {
          event.preventDefault();
          node.classList.add('drag-target');
        }
      };
      node.ondragleave = () => node.classList.remove('drag-target');
      node.ondrop = async (event) => {
        event.preventDefault();
        node.classList.remove('drag-target');
        try {
          const ids = JSON.parse(event.dataTransfer.getData('application/x-youji-media-items'));
          if (
            Array.isArray(ids) &&
            ids.every((id) => typeof id === 'string' && controller.state.data.items.some((item) => item.id === id))
          )
            await controller.addToCollection(collection.id, ids);
        } catch {}
      };
      list.append(node);
    }
    if (!data.collections.length)
      list.append(el('p', 'media-sidebar-empty', '创建清单，整理你的影视'));
    if (data.collections.length > 4)
      list.append(
        button(
          state.collectionsExpanded ? '收起更多' : `查看全部 · ${data.collections.length}`,
          'organization-nav-more',
          () => {
            state.collectionsExpanded = !state.collectionsExpanded;
            update();
          },
          'chevron'
        )
      );
  }
  update();
  return {
    update,
    dispose() {
      if (host.contains(nav)) host.replaceChildren();
    },
  };
}
