import { el, icon } from '../lib/dom.js';
import { collectionDragPayload, collectionDrop, createDragClickGuard } from '../lib/collection-drag.js';
import './collection-drag.css';

const CARD = '[data-organization-drag-game]';
const TARGET = '[data-organization-collection]';
const DISTANCE = 8;
const LONG_PRESS = 450;
const EXPAND_DELAY = 550;

// 事件委托兼容完整列表缓存；触屏在长按前保留原生滚动，长按后接管 touchmove。
export function mountCollectionDrag(state, actions, sidebar) {
  const listeners = new AbortController();
  const clickGuard = createDragClickGuard();
  let gesture, ghost, hint, target, hovered, holdTimer, hoverTimer, frame;
  let busy = false, ghostWidth = 0, ghostHeight = 0;

  function available() {
    return !busy && !state.bigScreen && state.page === 'library' && !state.selectedId
      && !document.querySelector('dialog[open], :popover-open') && sidebar.isAvailable();
  }
  function context() {
    return JSON.stringify([state.filter, state.search, state.collectionId, state.view, state.organizationBatchMode]);
  }
  function clearHover() {
    clearTimeout(hoverTimer); hoverTimer = undefined; hovered = undefined;
  }
  function reset() {
    clearTimeout(holdTimer); clearHover(); cancelAnimationFrame(frame);
    const previous = gesture; gesture = undefined;
    target?.classList.remove('organization-drop-target'); target = undefined;
    ghost?.remove(); ghost = undefined;
    document.body.classList.remove('collection-drag-active');
    previous?.card.classList.remove('collection-drag-source');
    if (previous?.active) {
      clickGuard.suppress(previous.kind, previous.id);
      sidebar.endDrag();
      if (previous.kind === 'pointer' && document.documentElement.hasPointerCapture(previous.id)) {
        document.documentElement.releasePointerCapture(previous.id);
      }
    }
  }
  function reconcile() {
    if (!gesture) return;
    const known = new Set(state.games.map(game => game.id));
    if (!available() || gesture.context !== context()
      || gesture.payload.gameIds.some(id => !known.has(id))) reset();
  }
  function paint() {
    if (!gesture?.active) return;
    if (state.bigScreen || state.page !== 'library' || state.selectedId) { reset(); return; }
    const { x, y } = gesture;
    ghost.style.transform = `translate(${Math.max(8, Math.min(x + 16, innerWidth - ghostWidth - 8))}px,${Math.max(8, Math.min(y + 16, innerHeight - ghostHeight - 8))}px)`;
    const under = document.elementFromPoint(x, y);
    const next = under?.closest(TARGET);
    if (next !== target) {
      target?.classList.remove('organization-drop-target'); target = next;
      target?.classList.add('organization-drop-target');
      const collection = state.organization.collections.find(item => item.id === target?.dataset.organizationCollection);
      hint.textContent = collection ? `松开加入「${collection.name}」` : '拖到左侧收藏夹 · Esc 取消';
    }
    const reveal = under?.closest('.organization-nav-more[aria-expanded="false"], .organization-nav-heading .nav-group-label[aria-expanded="false"]');
    if (reveal !== hovered) {
      clearHover(); hovered = reveal;
      if (reveal) hoverTimer = setTimeout(() => { sidebar.reveal(reveal); clearHover(); }, EXPAND_DELAY);
    }
    // 复用侧栏整体滚动，使展开后屏幕外的收藏夹也可以接收拖放。
    const scroll = sidebar.scrollElement;
    const rect = scroll.getBoundingClientRect();
    const top = Math.max(0, rect.top), bottom = Math.min(innerHeight, rect.bottom);
    if (x >= rect.left && x <= rect.right && y >= top && y <= bottom) {
      const delta = y < top + 40 ? -8 : y > bottom - 40 ? 8 : 0;
      if (delta) scroll.scrollTop += delta;
    }
    frame = requestAnimationFrame(paint);
  }
  function begin() {
    if (!gesture || !available()) { reset(); return; }
    clearTimeout(holdTimer); gesture.active = true;
    sidebar.beginDrag();
    document.body.classList.add('collection-drag-active');
    gesture.card.classList.add('collection-drag-source');
    window.getSelection()?.removeAllRanges();
    ghost = el('div', 'collection-drag-ghost');
    ghost.setAttribute('role', 'status'); ghost.setAttribute('aria-live', 'polite');
    hint = el('span', 'collection-drag-hint', '拖到左侧收藏夹 · Esc 取消');
    const copy = el('div'); copy.append(el('strong', '', gesture.payload.title), hint);
    ghost.append(icon('folder'), copy); document.body.append(ghost);
    ghostWidth = ghost.offsetWidth; ghostHeight = ghost.offsetHeight;
    if (gesture.kind === 'pointer') document.documentElement.setPointerCapture(gesture.id);
    paint();
  }
  function prepare(event, kind, id, x, y) {
    if (gesture || !available()) return;
    const card = event.target.closest?.(CARD);
    if (!card || card.disabled || !card.closest('#content .library-view')) return;
    const payload = collectionDragPayload(state, card.dataset.organizationDragGame);
    if (!payload) return;
    gesture = { card, kind, id, x, y, startX: x, startY: y, payload, context: context(), active: false };
    if (kind === 'touch') holdTimer = setTimeout(begin, LONG_PRESS);
  }
  function move(x, y) {
    if (!gesture) return;
    gesture.x = x; gesture.y = y;
    if (gesture.active) return;
    if (Math.hypot(x - gesture.startX, y - gesture.startY) < DISTANCE) return;
    if (gesture.kind === 'touch') reset(); // 长按前移动交给浏览器滚动。
    else begin();
  }
  async function finish(x, y) {
    if (!gesture) return;
    if (!gesture.active) { reset(); return; }
    reconcile(); if (!gesture) return;
    const collectionId = document.elementFromPoint(x, y)?.closest(TARGET)?.dataset.organizationCollection;
    const gameIds = gesture.payload.gameIds;
    reset(); if (!collectionId) return;
    busy = true;
    try {
      const drop = collectionDrop(state, gameIds, collectionId);
      if (!drop.gameIds.length) {
        actions.toast(`已在「${drop.collection.name}」中`); return;
      }
      await actions.organizationBatch('collection', false, drop.gameIds, [collectionId]);
      actions.toast(`已将 ${drop.gameIds.length} 款游戏加入「${drop.collection.name}」`);
    } catch (error) {
      actions.toast(String(error), true);
    } finally { busy = false; }
  }
  function listen(host, type, handler, options = {}) {
    host.addEventListener(type, handler, { ...options, signal: listeners.signal });
  }
  listen(document, 'pointerdown', event => {
    if (event.pointerType !== 'touch' && event.isPrimary && event.button === 0) {
      clickGuard.begin();
      prepare(event, 'pointer', event.pointerId, event.clientX, event.clientY);
    }
  });
  listen(document, 'pointermove', event => {
    if (gesture?.kind !== 'pointer' || gesture.id !== event.pointerId) return;
    move(event.clientX, event.clientY);
    if (gesture?.active) event.preventDefault();
  }, { passive: false });
  listen(document, 'pointerup', event => {
    if (clickGuard.matches('pointer', event.pointerId)) { event.preventDefault(); return; }
    if (gesture?.kind !== 'pointer' || gesture.id !== event.pointerId) return;
    if (gesture.active) event.preventDefault();
    void finish(event.clientX, event.clientY);
  }, { capture: true });
  listen(document, 'pointercancel', event => { if (gesture?.kind === 'pointer' && gesture.id === event.pointerId) reset(); });
  listen(document, 'lostpointercapture', event => { if (gesture?.kind === 'pointer' && gesture.id === event.pointerId) reset(); });
  listen(document, 'touchstart', event => {
    if (event.touches.length !== 1) { reset(); return; }
    clickGuard.begin();
    const touch = event.changedTouches[0];
    prepare(event, 'touch', touch.identifier, touch.clientX, touch.clientY);
  }, { passive: true });
  listen(document, 'touchmove', event => {
    if (gesture?.kind !== 'touch') return;
    const touch = [...event.touches].find(item => item.identifier === gesture.id);
    if (!touch) { reset(); return; }
    move(touch.clientX, touch.clientY);
    if (gesture?.active) event.preventDefault();
  }, { passive: false });
  listen(document, 'touchend', event => {
    if ([...event.changedTouches].some(touch => clickGuard.matches('touch', touch.identifier))) {
      event.preventDefault(); return;
    }
    if (gesture?.kind !== 'touch') return;
    const touch = [...event.changedTouches].find(item => item.identifier === gesture.id);
    if (!touch) return;
    if (gesture.active) event.preventDefault();
    void finish(touch.clientX, touch.clientY);
  }, { passive: false });
  listen(document, 'touchcancel', reset);
  listen(document, 'dragstart', event => { if (event.target.closest?.(CARD)) event.preventDefault(); }, { capture: true });
  listen(document, 'contextmenu', event => { if (gesture) event.preventDefault(); }, { capture: true });
  listen(document, 'click', event => {
    const suppressed = clickGuard.consumeClick(event.detail);
    if (event.detail && (gesture?.active || suppressed)) {
      event.preventDefault(); event.stopImmediatePropagation();
    }
  }, { capture: true });
  listen(document, 'keydown', event => {
    if (gesture && event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); reset(); }
  }, { capture: true });
  listen(window, 'blur', reset);
  listen(window, 'resize', reset);
  listen(document, 'visibilitychange', () => { if (document.hidden) reset(); });
  listen(document, 'toggle', event => { if (event.newState === 'open') reset(); }, { capture: true });
  const dispose = () => { reset(); listeners.abort(); };
  if (import.meta.hot) import.meta.hot.dispose(dispose);
  return { reconcile, dispose };
}
