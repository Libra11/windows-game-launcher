import { el } from '../lib/dom.js';
import { programPathField } from './program-path-field.js';
import './local-launch-fields.css';

export function localLaunchFields(game = {}) {
  const element = el('div', 'local-launch-fields');
  const exe = programPathField('游戏程序', game.exePath || ''); exe.input.required = true;
  const section = el('div', 'local-launcher-section');
  const label = el('label', 'setting-check local-launcher-toggle');
  const enabled = el('input'); enabled.type = 'checkbox'; enabled.checked = !!game.launchUri;
  label.append(enabled, el('span', '', '通过启动器启动'));
  const launcher = programPathField('启动器程序', game.launchUri || '');
  const update = () => { launcher.wrapper.hidden = !enabled.checked; launcher.input.required = enabled.checked; launcher.input.disabled = !enabled.checked; };
  enabled.addEventListener('change', update); update();
  section.append(label, launcher.wrapper); element.append(exe.wrapper, section);
  return { element, exe:exe.input, get launcherPath() { return enabled.checked ? launcher.input.value.trim() : ''; },
    async ready() { await exe.ready(); if (enabled.checked) await launcher.ready(); },
    dispose() { exe.dispose(); launcher.dispose(); },
  };
}
