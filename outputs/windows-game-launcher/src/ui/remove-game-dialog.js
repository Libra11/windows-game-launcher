import { el, button } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import { submit } from './form-fields.js';
import './remove-game-dialog.css';

export function removeGameDialog(game, actions) {
  const form = el('form', 'form');
  const notice = el('div', 'remove-game-notice');
  notice.append(el('strong', '', game.title),
    el('p', '', '此游戏会从游戏库移除，并清除启动器保存的解锁、收藏和游玩记录。'),
    el('p', '', '游戏程序、存档和原始成就记录文件均保留，之后可以重新添加。'));
  form.append(notice);
  const dialog = modal('移除本地游戏？', '请确认要移除的游戏。', form);
  dialog.classList.add('remove-game-modal');
  submit(form, '确认移除', async () => {
    await actions.remove(game);
    await closeModal(dialog);
  });
  const confirm = form.querySelector('button[type="submit"]');
  confirm.classList.add('remove-game-confirm');
  const cancel = button('取消', 'secondary full', () => closeModal(dialog));
  form.insertBefore(cancel, confirm);
  cancel.focus();
}
