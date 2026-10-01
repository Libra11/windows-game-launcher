import { el, button } from '../lib/dom.js';
import { chooseFile } from '../lib/bridge.js';

export function field(label, value = '', placeholder = '', type = 'text') {
  const wrapper = el('label', 'field'); const input = el('input');
  input.type = type; input.value = value; input.placeholder = placeholder;
  input.autocomplete = 'off'; wrapper.append(el('span', '', label), input);
  return { wrapper, input };
}

export function browse(field, extensions) {
  const row = el('div', 'input-row'); field.wrapper.replaceChild(row, field.input);
  row.append(field.input, button('选择文件', 'secondary', async () => {
    const path = await chooseFile({ multiple: false, filters: [{ name: extensions.join(' / '), extensions }] });
    if (path) { field.input.value = path; field.input.dispatchEvent(new Event('change', { bubbles:true })); }
  }, 'folder'));
}

export function submit(form, label, handler) {
  const action = el('button', 'primary full', label); action.type = 'submit'; form.append(action);
  form.addEventListener('submit', async event => {
    event.preventDefault(); if (action.disabled) return;
    action.disabled = true; action.textContent = '正在处理…';
    form.querySelector('.form-error')?.remove();
    try { await handler(); } catch (error) {
      const message = el('p', 'form-error', String(error)); message.setAttribute('role', 'alert'); form.insertBefore(message, action);
    }
    finally { action.disabled = false; action.textContent = label; }
  });
}
