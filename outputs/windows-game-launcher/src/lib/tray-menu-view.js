import { appIcon, el, icon } from './dom.js';

export function menuFocusIndex(count, current, key) {
  if (!count) return -1;
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  if (key === 'ArrowDown' || key === 'Tab') return (current + 1 + count) % count;
  if (key === 'ArrowUp') return current < 0 ? count - 1 : (current - 1 + count) % count;
  return current;
}

function itemButton(entry, glyph, activate) {
  const button = el('button', 'tray-item');
  button.type = 'button';
  button.dataset.key = entry.key;
  button.setAttribute('role', 'menuitem');
  button.tabIndex = -1;
  const mark = el('span', 'tray-item-mark');
  mark.append(icon(glyph));
  const title = el('span', 'tray-item-title', entry.title);
  title.title = entry.title;
  const trailing = el('span', 'tray-item-trailing');
  trailing.append(icon('chevron'));
  button.append(mark, title, trailing);
  button.addEventListener('click', () => activate(entry.key));
  return button;
}

function section(title, glyph, rows, activate) {
  const group = el('section', 'tray-section');
  const heading = el('div', 'tray-section-heading');
  const label = el('span', 'tray-section-label');
  label.append(icon(glyph), el('span', '', title));
  heading.append(label, el('span', 'tray-section-count', rows.length ? `${rows.length} 项` : '空'));
  group.append(heading);
  if (!rows.length) {
    const empty = el('div', 'tray-empty');
    empty.append(icon('history'), el('span', '', '暂无记录'));
    group.append(empty);
    return group;
  }
  const list = el('div', 'tray-section-items');
  for (const entry of rows) list.append(itemButton(entry, glyph, activate));
  group.append(list);
  return group;
}

export function createTrayMenu(root, { activate, dismiss }) {
  const focusButton = button => {
    if (!button) return;
    root.querySelectorAll('.tray-item').forEach(item => { item.tabIndex = item === button ? 0 : -1; });
    button.focus({ preventScroll: true });
  };
  const focusFirst = () => focusButton(root.querySelector('.tray-item'));

  root.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      event.preventDefault();
      dismiss();
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End', 'Tab'].includes(event.key)) return;
    const buttons = [...root.querySelectorAll('.tray-item')];
    const current = buttons.indexOf(document.activeElement);
    const key = event.key === 'Tab' && event.shiftKey ? 'ArrowUp' : event.key;
    const next = menuFocusIndex(buttons.length, current, key);
    if (next < 0) return;
    event.preventDefault();
    focusButton(buttons[next]);
  });

  return {
    render(snapshot, { focus = false } = {}) {
      const card = el('div', 'tray-card');
      card.setAttribute('role', 'menu');
      card.setAttribute('aria-label', '游迹快速启动');
      const brand = el('header', 'tray-brand');
      brand.append(appIcon('tray-brand-icon'));
      const copy = el('div', 'tray-brand-copy');
      copy.append(el('strong', '', '游迹'), el('span', '', '从上次离开的地方继续'));
      brand.append(copy, el('span', 'tray-brand-caption', '快速启动'));
      const quit = itemButton({ key: 'quit', title: '退出' }, 'close', activate);
      quit.classList.add('tray-quit');
      card.append(
        brand,
        section('最近游戏', 'game', snapshot.games || [], activate),
        el('div', 'tray-separator'),
        section('最近影视', 'film', snapshot.media || [], activate),
        el('div', 'tray-separator'),
        quit,
      );
      root.replaceChildren(card);
      const buttons = [...root.querySelectorAll('.tray-item')];
      buttons.forEach((button, index) => { button.tabIndex = index === 0 ? 0 : -1; });
      if (focus) focusFirst();
    },
    focusFirst,
  };
}
