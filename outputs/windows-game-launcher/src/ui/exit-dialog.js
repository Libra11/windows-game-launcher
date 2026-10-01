import { el, button } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import { submit } from './form-fields.js';

export async function exitDialog(actions) {
  const games = await actions.run('list_games');
  const active = games.filter(game => ['starting','running'].includes(game.runtime?.state));
  const form = el('form', 'form');
  form.append(el('p', 'form-hint', active.length
    ? `当前有 ${active.length} 款游戏正在启动或运行。退出后游戏继续运行，但后台计时与成就检测会停止。再次打开后，核对进程并继续记录。`
    : '退出后会停止后台检测。当前记录会先保存，游戏文件不会受到影响。'));
  const dialog = modal('退出启动器？', '关闭到托盘可以继续后台计时与成就检测。', form);
  const cancel = button('继续使用', 'secondary full', () => closeModal(dialog)); form.append(cancel);
  submit(form, '保存并退出', () => actions.run('quit_launcher'));
  cancel.focus();
}
