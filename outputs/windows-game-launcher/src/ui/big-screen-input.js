import { nextFocus, gridNeighbor, gamepadActions, repeatedActions } from '../lib/big-screen-navigation.js';
import { closeModal } from './modal.js';
import { reducedMotion } from './motion.js';

export function mountBigScreenInput(root, actions) {
  const selector = 'button:not(:disabled), input, select, summary, a[href]';
  const scope = () => document.querySelector('dialog[open]') || root;
  const controls = () => [...scope().querySelectorAll(selector)].filter(node => node.getBoundingClientRect().width && !node.closest('[hidden]'));
  function focus(node) {
    node.focus({ preventScroll: true });
    node.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: reducedMotion() ? 'instant' : 'smooth' });
  }
  function move(direction) {
    const grid = document.activeElement.closest('.big-screen-grid');
    if (grid && scope().contains(grid)) {
      const cards = [...grid.querySelectorAll('.big-screen-card')];
      const columns = getComputedStyle(grid).gridTemplateColumns.split(' ').length;
      const index = gridNeighbor(cards.indexOf(document.activeElement), cards.length, columns, direction);
      if (index !== -1) return focus(cards[index]);
      // 第一排向上回到游戏操作，其他边界不跳行或跳到顶栏。
      if (direction !== 'up') return;
    }
    const nodes = controls();
    const rectangles = nodes.map(node => {
      const rect = node.getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
    });
    const index = nextFocus(rectangles, nodes.indexOf(document.activeElement), direction);
    if (index !== -1) {
      focus(nodes[index]);
    } else if (direction === 'up' || direction === 'down') {
      scope().querySelector('.big-screen-content')?.scrollBy({ top: direction === 'up' ? -260 : 260 });
    }
  }
  function perform(action) {
    if (['left', 'right', 'up', 'down'].includes(action)) return move(action);
    if (action === 'confirm') {
      if (!scope().contains(document.activeElement)) controls()[0]?.focus();
      const control = document.activeElement;
      if (!control.disabled) control.click();
    } else if (action === 'back') {
      const dialog = document.querySelector('dialog[open]');
      if (dialog) closeModal(dialog); else actions.back();
    } else if (!document.querySelector('dialog[open]')) actions.category(action === 'next' ? 1 : -1);
  }
  function keydown(event) {
    const typing = event.target.matches('input, textarea, select');
    if (typing && event.key !== 'Escape') return;
    const action = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down', Enter: 'confirm', ' ': 'confirm', Escape: 'back' }[event.key];
    if (!action) return;
    event.preventDefault();
    if (!event.repeat || ['left', 'right', 'up', 'down'].includes(action)) perform(action);
  }
  document.addEventListener('keydown', keydown);
  let frame, controllerId = null, armed = false;
  const held = new Map();
  function poll(now) {
    if (!document.hidden && document.hasFocus()) {
      let pad;
      try { pad = [...(navigator.getGamepads?.() || [])].find(item => item?.connected && item.mapping === 'standard'); } catch { /* 未提供手柄接口时继续使用键盘。 */ }
      const id = pad ? `${pad.index}:${pad.id}` : null;
      if (id !== controllerId) { held.clear(); controllerId = id; armed = false; }
      const current = gamepadActions(pad);
      // 连接手柄或从游戏返回后，先松开按键，避免带入游戏中的操作。
      if (!armed) { if (!current.length) armed = true; }
      else for (const action of repeatedActions(current, held, now)) perform(action);
    } else { held.clear(); armed = false; }
    frame = requestAnimationFrame(poll);
  }
  frame = requestAnimationFrame(poll);
  return () => { cancelAnimationFrame(frame); document.removeEventListener('keydown', keydown); };
}
