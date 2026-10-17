import { el, button } from '../lib/dom.js';
import { command, chooseFile } from '../lib/bridge.js';
import { createProgramPathResolver, programDropPoint } from '../lib/program-path.js';
import { field } from './form-fields.js';

const targets = new WeakMap();

export function programPathField(label, value = '') {
  const control = field(label, value, '选择或拖入 .exe 程序、.lnk 快捷方式');
  control.input.setAttribute('aria-label', label);
  control.wrapper.classList.add('program-path-field');
  control.wrapper.querySelector('span').append(el('small', 'program-path-hint', '可拖入 .exe / .lnk'));
  const row = el('div', 'input-row');
  control.wrapper.replaceChild(row, control.input);
  const error = el('p', 'program-path-error'); error.setAttribute('role', 'alert'); error.hidden = true;
  const resolver = createProgramPathResolver({
    resolve: path => command('resolve_program_path', { path }),
    apply: path => {
      if (!control.wrapper.isConnected) return;
      control.input.value = path;
      error.hidden = true;
      control.input.dispatchEvent(new Event('change', { bubbles:true }));
    },
    failed: reason => { error.textContent = String(reason); error.hidden = false; },
  });
  const select = path => { error.hidden = true; return resolver.select(path); };
  row.append(control.input, button('选择文件', 'secondary', async () => {
    try {
      const path = await chooseFile({ multiple:false, filters:[{ name:'程序或快捷方式', extensions:['exe','lnk'] }] });
      if (path && control.wrapper.isConnected && !control.input.matches(':disabled')) await select(path);
    } catch (reason) { error.textContent = String(reason); error.hidden = false; }
  }, 'folder'));
  control.wrapper.append(error);
  control.input.addEventListener('input', () => { resolver.invalidate(); error.hidden = true; });
  control.input.addEventListener('change', () => {
    if (/\.lnk$/i.test(control.input.value.trim())) void select(control.input.value.trim());
  });
  targets.set(control.wrapper, { input:control.input, select });
  return { ...control, ready:resolver.ready, dispose:resolver.invalidate };
}

export function programPathTarget(position) {
  const point = programDropPoint(position, window.devicePixelRatio);
  const dialog = [...document.querySelectorAll('dialog[open]')].at(-1);
  if (!point || !dialog) return null;
  const wrapper = document.elementsFromPoint(point.x, point.y)
    .map(node => node.closest('.program-path-field')).find(node => node && dialog.contains(node));
  const target = targets.get(wrapper);
  return target && !target.input.matches(':disabled') ? { wrapper, ...target } : null;
}
