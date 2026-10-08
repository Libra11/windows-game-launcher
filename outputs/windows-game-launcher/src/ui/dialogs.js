import { el, sourceName } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import { field, browse, submit } from './form-fields.js';
import { editLocalDialog } from './edit-local-dialog.js';
export { addDialog } from './add-game-dialog.js';
export function editDialog(game, actions) {
  if (game.source === 'local') return editLocalDialog(game, actions);
  const form = el('form', 'form');
  const exe = field('实际游戏程序（仅用于检测运行状态）',game.exePath || '', '选择实际运行的 .exe'); browse(exe,['exe']);
  if (game.source === 'steam' || game.source === 'epic') {
    form.append(exe.wrapper,el('p','form-hint',`${sourceName(game)} 游戏仍通过 ${sourceName(game)} 客户端启动。若自动检测无法确认运行状态，可以指定实际游戏程序；填写启动器程序可能无法准确判断游戏是否结束。`));
    const dialog = modal('启动检测设置',game.title,form);
    submit(form,'保存设置',async()=> {await actions.run('set_launch_path',{gameId:game.id,exePath:exe.input.value.trim()}); await closeModal(dialog); await actions.refresh();});
    return;
  }
}
