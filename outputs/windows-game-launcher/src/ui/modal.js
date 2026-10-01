import { el, button } from '../lib/dom.js';
import { dismiss } from './motion.js';

const closing = new WeakMap();
export function closeModal(dialog) {
  if (closing.has(dialog)) return closing.get(dialog);
  if (!dialog.open) return Promise.resolve();
  dialog.classList.add('closing');
  const done = dismiss(dialog).then(() => dialog.close());
  closing.set(dialog, done);
  return done;
}

export function modal(title, subtitle, content) {
  const previous = document.activeElement;
  const dialog = el('dialog', 'modal'); dialog.setAttribute('aria-label', title);
  const header = el('div', 'modal-header');
  const copy = el('div'); copy.append(el('h2', '', title), el('p', '', subtitle));
  const close = button('', 'icon-button', () => closeModal(dialog), 'close'); close.setAttribute('aria-label', '关闭');
  header.append(copy, close); dialog.append(header, content); document.body.append(dialog);
  dialog.addEventListener('click', event => {
    const rect = dialog.getBoundingClientRect();
    if (event.target === dialog && (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom)) closeModal(dialog);
  });
  dialog.addEventListener('cancel', event => { event.preventDefault(); closeModal(dialog); });
  dialog.addEventListener('close', () => { dialog.remove(); if (previous?.isConnected) previous.focus({ preventScroll: true }); }, { once: true });
  dialog.showModal(); return dialog;
}
