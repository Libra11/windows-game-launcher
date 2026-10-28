import { el, icon } from '../../lib/dom.js';
import '../game-context-menu.css';

export function createMediaContextMenu(onError) {
  let closeCurrent = () => {};
  return {
    close() { closeCurrent(); },
    open(title, groups, event) {
      closeCurrent();
      const anchor = event?.target?.closest('button') || event?.currentTarget;
      const previousFocus = document.activeElement;
      const listeners = new AbortController();
      const menu = el('div', 'game-context-menu');
      menu.popover = 'auto';
      menu.setAttribute('role', 'menu');
      menu.setAttribute('aria-label', '影视操作');
      menu.append(el('div', 'game-menu-title', title));
      const items = el('div', 'game-menu-items');
      const close = (restore = false) => {
        listeners.abort();
        menu.remove();
        if (restore && previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
        if (closeCurrent === close) closeCurrent = () => {};
      };
      closeCurrent = close;
      for (const group of groups.filter(group => group.length)) {
        if (items.children.length) {
          const line = el('div', 'game-menu-separator');
          line.setAttribute('role', 'separator');
          items.append(line);
        }
        for (const [label, glyph, action] of group) {
          const row = el('button', 'game-menu-item');
          row.type = 'button';
          row.tabIndex = -1;
          row.setAttribute('role', 'menuitem');
          const copy = el('span', 'game-menu-copy');
          copy.append(el('span', 'game-menu-label', label));
          row.append(icon(glyph), copy);
          row.onclick = async () => {
            close(true);
            try { await action(); } catch (error) { onError?.(String(error)); }
          };
          items.append(row);
        }
      }
      menu.append(items);
      document.body.append(menu);
      menu.showPopover();
      const rect = anchor?.getBoundingClientRect();
      const keyboard = event?.type === 'keydown' || (!event?.clientX && !event?.clientY);
      const x = keyboard ? rect?.right ?? 8 : event.clientX;
      const y = keyboard ? rect?.top ?? 8 : event.clientY;
      const bounds = menu.getBoundingClientRect();
      menu.style.left = `${Math.max(8, Math.min(x, innerWidth - bounds.width - 8))}px`;
      menu.style.top = `${Math.max(8, Math.min(y, innerHeight - bounds.height - 8))}px`;
      const rows = [...items.querySelectorAll('[role="menuitem"]')];
      menu.addEventListener('keydown', event => {
        const index = rows.indexOf(document.activeElement);
        if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
          event.preventDefault();
          const next = event.key === 'Home' ? 0 : event.key === 'End' ? rows.length - 1
            : (index + (event.key === 'ArrowDown' ? 1 : -1) + rows.length) % rows.length;
          rows[next]?.focus({ preventScroll: true });
        } else if (event.key === 'Escape' || event.key === 'Tab') {
          if (event.key === 'Escape') event.preventDefault();
          close(true);
        }
      });
      const listen = (target, type, handler, options = {}) =>
        target.addEventListener(type, handler, { ...options, signal: listeners.signal });
      listen(menu, 'toggle', event => { if (event.newState === 'closed') close(true); });
      listen(document, 'pointerdown', event => { if (!menu.contains(event.target)) close(); }, { capture: true });
      listen(document, 'scroll', event => { if (!menu.contains(event.target)) close(); }, { capture: true });
      listen(window, 'resize', () => close());
      listen(window, 'blur', () => close());
      rows[0]?.focus({ preventScroll: true });
    },
  };
}
