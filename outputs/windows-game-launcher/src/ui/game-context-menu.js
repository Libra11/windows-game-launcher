import { el, icon } from '../lib/dom.js';
import { gameMenuScope, gameMenuModel, gameManagementItems, gameCollectionItems } from '../lib/game-context-menu.js';
import { bulkOrganizationDialog } from './organization-dialogs.js';
import { removeGameDialog } from './remove-game-dialog.js';
import './game-context-menu.css';

const TILE = '[data-game-menu-id]';
const EDITABLE = 'input,textarea,[contenteditable="true"]';

export function mountGameContextMenu(state, actions) {
  const listeners = new AbortController();
  let session, root, submenu, trigger, mainItems;
  const context = () => JSON.stringify([state.page, state.selectedId, state.bigScreen, state.filter,
    state.search, state.collectionId, state.view, state.organizationBatchMode, [...state.organizationSelection]]);
  const game = () => actions.game(session?.scope.gameId);
  const entries = panel => [...panel.querySelectorAll('[role="menuitem"],[role="menuitemcheckbox"]')]
    .filter(node => node.closest('[role="menu"]') === panel);

  function restoreFocus(previous) {
    if (!document.hasFocus()) return;
    const node = previous.anchor?.isConnected ? previous.anchor
      : [...document.querySelectorAll('#content [data-focus-key]')].find(value => value.dataset.focusKey === previous.focusKey);
    node?.focus({ preventScroll: true });
  }
  function closeSubmenu(focus = false) {
    submenu?.remove(); submenu = undefined;
    trigger?.setAttribute('aria-expanded', 'false');
    if (focus) trigger?.focus({ preventScroll: true });
    trigger = undefined;
  }
  function close(focus = false) {
    const previous = session;
    if (previous?.anchor.dataset.gameMenuButton) previous.anchor.setAttribute('aria-expanded', 'false');
    session = undefined; closeSubmenu(); root?.remove(); root = undefined;
    if (focus && previous) restoreFocus(previous);
  }
  function place(panel, x, y) {
    const rect = panel.getBoundingClientRect();
    panel.style.left = `${Math.max(8, Math.min(x, innerWidth - rect.width - 8))}px`;
    panel.style.top = `${Math.max(8, Math.min(y, innerHeight - rect.height - 8))}px`;
  }
  function row(descriptor) {
    const node = el('button', 'game-menu-item'); node.type = 'button'; node.tabIndex = -1;
    node.dataset.menuAction = descriptor.id;
    node.setAttribute('role', descriptor.collectionId && descriptor.checked !== undefined ? 'menuitemcheckbox' : 'menuitem');
    if (descriptor.checked !== undefined) node.setAttribute('aria-checked', String(descriptor.checked));
    node.setAttribute('aria-disabled', String(!!descriptor.disabled));
    node.classList.toggle('danger', !!descriptor.danger);
    node.title = descriptor.reason || descriptor.label;
    const copy = el('span', 'game-menu-copy'); copy.append(el('span', 'game-menu-label', descriptor.label));
    if (descriptor.disabled && descriptor.reason) {
      copy.append(el('span', 'game-menu-reason', descriptor.reason));
      node.setAttribute('aria-description', descriptor.reason);
    }
    node.append(icon(descriptor.glyph), copy);
    if (descriptor.submenu) {
      node.setAttribute('aria-haspopup', 'menu'); node.setAttribute('aria-expanded', 'false');
      node.append(icon('chevron', 'game-menu-trailing'));
      node.onpointerenter = event => { if (event.pointerType === 'mouse') openSubmenu(node, descriptor.id); };
    } else {
      if (descriptor.checked !== undefined) node.append(icon('check', 'game-menu-trailing game-menu-check'));
      node.onpointerenter = event => { if (event.pointerType === 'mouse' && node.closest('[role="menu"]') === root) closeSubmenu(); };
    }
    node.onclick = event => {
      event.preventDefault(); event.stopPropagation();
      if (!session || session.pending || descriptor.disabled) return;
      if (descriptor.submenu) openSubmenu(node, descriptor.id, true);
      else void execute(descriptor);
    };
    return node;
  }
  function paintMain() {
    const model = gameMenuModel(state, session.scope);
    if (!model) { close(); return; }
    session.modelKey = JSON.stringify(model);
    root.querySelector('.game-menu-title').textContent = model.title;
    mainItems.replaceChildren();
    for (const group of model.groups.filter(value => value.length)) {
      if (mainItems.children.length) {
        const line = el('div', 'game-menu-separator'); line.setAttribute('role', 'separator'); mainItems.append(line);
      }
      mainItems.append(...group.map(row));
    }
    place(root, session.x, session.y);
  }
  function paintCollections() {
    if (!submenu || !session) return;
    const list = submenu.querySelector('.game-menu-items');
    const scrollTop = submenu.scrollTop;
    const search = submenu.querySelector('input').value;
    const focused = document.activeElement?.dataset.menuAction;
    const values = gameCollectionItems(state, session.scope, search);
    const key = JSON.stringify(values);
    if (submenu.dataset.itemsKey === key) return;
    submenu.dataset.itemsKey = key;
    list.replaceChildren();
    list.append(...values.map(row));
    if (!values.length) list.append(el('p', 'game-menu-empty', search ? '没有匹配的收藏夹' : '还没有收藏夹'));
    list.append(row({ id: 'new-collection', label: '新建收藏夹…', glyph: 'plus' }));
    entries(submenu).find(node => node.dataset.menuAction === focused)?.focus({ preventScroll: true });
    submenu.scrollTop = scrollTop;
  }
  function openSubmenu(node, kind, focus = false) {
    if (!session || session.pending) return;
    if (trigger === node && submenu) {
      if (focus) entries(submenu)[0]?.focus({ preventScroll: true });
      return;
    }
    closeSubmenu(); trigger = node;
    node.setAttribute('aria-expanded', 'true');
    submenu = el('div', 'game-context-menu game-menu-submenu'); submenu.popover = 'manual';
    submenu.setAttribute('role', 'menu'); submenu.setAttribute('aria-label', kind === 'collections' ? '收藏夹' : '管理游戏');
    submenu.dataset.kind = kind;
    if (kind === 'collections') {
      const search = el('input', 'game-menu-search'); search.type = 'search'; search.placeholder = '搜索收藏夹';
      search.setAttribute('aria-label', '搜索收藏夹'); search.oninput = paintCollections;
      submenu.append(search, el('div', 'game-menu-items')); paintCollections();
    } else {
      const list = el('div', 'game-menu-items'); list.append(...gameManagementItems(game()).map(row)); submenu.append(list);
    }
    root.append(submenu); submenu.showPopover();
    const anchor = node.getBoundingClientRect(), rect = submenu.getBoundingClientRect();
    const x = anchor.right + 5 + rect.width <= innerWidth - 8 ? anchor.right + 5 : anchor.left - rect.width - 5;
    place(submenu, x, anchor.top);
    if (focus) entries(submenu)[0]?.focus({ preventScroll: true });
  }
  async function execute(descriptor) {
    const previous = session;
    // 菜单开启后仍按当前状态校验；不会使用旧运行状态重复启动。
    const latest = gameMenuModel(state, previous.scope);
    if (!latest || previous.context !== context()) { close(); return; }
    const current = game();
    if (descriptor.checked !== undefined) {
      previous.pending = true; root.setAttribute('aria-busy', 'true');
      const remove = !!state.organization.byCollection.get(descriptor.collectionId)?.has(current.id);
      try {
        await actions.organizationBatch('collection', remove, previous.scope.gameIds, [descriptor.collectionId]);
      } catch (error) { actions.toast(String(error), true); }
      finally {
        previous.pending = false;
        if (session === previous) { root.setAttribute('aria-busy', 'false'); reconcile(); }
      }
      return;
    }
    const launch = latest.groups.flat().find(value => value.id === descriptor.id);
    if (launch?.disabled) { reconcile(); return; }
    close(true);
    const bulk = { 'add-collection': ['collection', false], 'remove-collection': ['collection', true],
      'add-tag': ['tag', false], 'remove-tag': ['tag', true] }[descriptor.id];
    try {
      if (bulk) bulkOrganizationDialog(...bulk, previous.scope.gameIds, actions);
      else if (descriptor.id === 'remove-current') await actions.organizationBatch('collection', true, previous.scope.gameIds, [descriptor.collectionId]);
      else if (descriptor.id === 'remove') removeGameDialog(current, actions);
      else if (descriptor.id === 'new-collection') actions.organizationNewCollection();
      else if (descriptor.id === 'tags') actions.organize(current);
      else await ({ launch: actions.launch, install: actions.install, details: actions.select, favorite: actions.favorite,
        sync: actions.sync, edit: actions.edit, scan: actions.scan, guide: actions.guide })[descriptor.id]?.(current);
    } catch (error) { actions.toast(String(error), true); }
  }
  function open(gameId, anchor, x, y) {
    if (state.bigScreen || state.page !== 'library' || state.selectedId
      || document.body.classList.contains('collection-drag-active') || document.querySelector('dialog[open]')) return;
    const scope = gameMenuScope(state, gameId); if (!scope) return;
    close(); anchor.focus({ preventScroll: true });
    if (anchor.dataset.gameMenuButton) anchor.setAttribute('aria-expanded', 'true');
    const rect = anchor.getBoundingClientRect();
    session = { scope, anchor, focusKey: anchor.dataset.focusKey, context: context(),
      x: x ?? rect.right, y: y ?? rect.top, pending: false };
    root = el('div', 'game-context-menu'); root.popover = 'auto';
    root.id = 'game-context-menu';
    root.setAttribute('role', 'menu'); root.setAttribute('aria-label', '游戏操作');
    root.append(el('div', 'game-menu-title')); mainItems = el('div', 'game-menu-items'); root.append(mainItems);
    const currentRoot = root;
    root.addEventListener('toggle', event => { if (event.newState === 'closed' && root === currentRoot) close(); });
    document.body.append(root); root.showPopover({ source: anchor }); paintMain();
    entries(root)[0]?.focus({ preventScroll: true });
  }
  function reconcile() {
    if (!session) return;
    const model = gameMenuModel(state, session.scope);
    if (!model || session.context !== context() || document.querySelector('dialog[open]')) { close(); return; }
    if (session.pending) return;
    if (session.modelKey !== JSON.stringify(model)) {
      const focused = document.activeElement?.dataset.menuAction;
      const kind = submenu?.dataset.kind;
      const input = submenu?.querySelector('input');
      const search = input?.value || '';
      const searchFocused = !!input && document.activeElement === input;
      const selection = searchFocused ? [input.selectionStart, input.selectionEnd] : null;
      closeSubmenu(); paintMain();
      const nextTrigger = entries(root).find(node => node.dataset.menuAction === kind);
      if (nextTrigger) {
        openSubmenu(nextTrigger, kind);
        if (kind === 'collections') {
          const nextInput = submenu.querySelector('input'); nextInput.value = search; paintCollections();
          if (searchFocused) { nextInput.focus({ preventScroll: true }); nextInput.setSelectionRange(...selection); }
        }
      }
      [...entries(root), ...(submenu ? entries(submenu) : [])].find(node => node.dataset.menuAction === focused)?.focus({ preventScroll: true });
    }
    if (submenu?.dataset.kind === 'collections') paintCollections();
  }
  function listen(host, type, handler, options = {}) {
    host.addEventListener(type, handler, { ...options, signal: listeners.signal });
  }
  listen(document, 'contextmenu', event => {
    if (event.target.closest?.(EDITABLE) || window.getSelection()?.toString()) return;
    event.preventDefault();
    const tile = event.target.closest?.(TILE);
    if (!tile || !tile.closest('#content .library-view')) { close(); return; }
    open(tile.dataset.gameMenuId, tile.querySelector('.game-card'), event.clientX, event.clientY);
  });
  listen(document, 'click', event => {
    const node = event.target.closest?.('[data-game-menu-button]'); if (!node) return;
    event.preventDefault(); event.stopPropagation();
    if (session?.anchor === node) { close(true); return; }
    open(node.dataset.gameMenuButton, node);
  });
  listen(document, 'keydown', event => {
    if (!root) {
      if (event.key !== 'ContextMenu' && !(event.key === 'F10' && event.shiftKey)) return;
      const tile = event.target.closest?.(TILE); if (!tile || event.target.closest(EDITABLE)) return;
      event.preventDefault(); open(tile.dataset.gameMenuId, event.target); return;
    }
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(true); return; }
    if (event.key === 'Tab') { close(true); return; }
    const panel = event.target.closest?.('[role="menu"]'); if (!panel) return;
    const values = entries(panel), index = values.indexOf(document.activeElement);
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      if (event.target.matches(EDITABLE) && ['Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? values.length - 1
        : index < 0 ? (event.key === 'ArrowDown' ? 0 : values.length - 1)
        : (index + (event.key === 'ArrowDown' ? 1 : -1) + values.length) % values.length;
      values[next]?.focus({ preventScroll: false });
    } else if (event.key === 'ArrowRight' && document.activeElement?.getAttribute('aria-haspopup') === 'menu') {
      event.preventDefault(); openSubmenu(document.activeElement, document.activeElement.dataset.menuAction, true);
    } else if (event.key === 'ArrowLeft' && panel === submenu && !event.target.matches(EDITABLE)) {
      event.preventDefault(); closeSubmenu(true);
    }
  }, { capture: true });
  listen(document, 'scroll', event => { if (root && !root.contains(event.target)) close(); }, { capture: true });
  listen(window, 'resize', () => close());
  listen(window, 'blur', () => close());
  listen(document, 'visibilitychange', () => { if (document.hidden) close(); });
  const dispose = () => { close(); listeners.abort(); };
  if (import.meta.hot) import.meta.hot.dispose(dispose);
  return { reconcile, dispose };
}
