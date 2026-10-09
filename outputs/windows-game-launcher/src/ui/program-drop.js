import { onFileDrop, command } from '../lib/bridge.js';
import { createProgramDropHandler } from '../lib/program-drop.js';
import { el } from '../lib/dom.js';
import { addDialog } from './add-game-dialog.js';
import './program-drop.css';

export async function mountProgramDrop(actions) {
  const overlay = el('div', 'program-drop-overlay');
  overlay.hidden = true; overlay.setAttribute('role', 'status');
  overlay.append(el('strong', '', '松开以添加游戏'), el('span', '', '支持游戏程序 .exe 和快捷方式 .lnk'));
  document.body.append(overlay);
  const isBlocked = () => !!document.querySelector('dialog[open]');
  const handle = createProgramDropHandler({
    prepare: path => command('prepare_local_import', { path }),
    open: candidate => addDialog(actions, candidate),
    notify: actions.toast, isBlocked,
  });
  try {
    return await onFileDrop(event => {
      const { type, paths } = event.payload;
      overlay.hidden = !['enter', 'over'].includes(type) || isBlocked();
      if (type === 'drop') void handle(paths);
    });
  } catch (error) { overlay.remove(); throw error; }
}
