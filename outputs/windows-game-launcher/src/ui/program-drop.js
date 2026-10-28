import { onFileDrop, command } from '../lib/bridge.js';
import { createProgramDropHandler } from '../lib/program-drop.js';
import { el } from '../lib/dom.js';
import { addDialog } from './add-game-dialog.js';
import './program-drop.css';
import { programPathTarget } from './program-path-field.js';

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
    let highlighted;
    return await onFileDrop(event => {
      const { type, paths, position } = event.payload;
      if(actions.enabled?.()===false){overlay.hidden=true;highlighted?.classList.remove('program-path-hover');highlighted=null;return;}
      const target = ['enter','over','drop'].includes(type) ? programPathTarget(position) : null;
      highlighted?.classList.remove('program-path-hover');
      highlighted = target?.wrapper;
      if (type !== 'drop') highlighted?.classList.add('program-path-hover');
      overlay.hidden = !['enter', 'over'].includes(type) || isBlocked();
      if (type === 'drop') {
        if (target) {
          if (paths.length !== 1) actions.toast('请一次拖入一个程序或快捷方式。', true);
          else void target.select(paths[0]);
        } else void handle(paths);
      }
    });
  } catch (error) { overlay.remove(); throw error; }
}
