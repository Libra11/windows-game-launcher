import { el } from '../lib/dom.js';
import { modal } from './modal.js';

export async function recentUnlocksDialog(actions) {
  const items = await actions.run('list_recent_unlocks');
  const content = el('div', 'recent-unlocks');
  if (!items.length) content.append(el('p', 'form-hint', '暂时没有新的解锁记录。首次导入的历史成就不会补发通知。'));
  for (const item of items) {
    const row = el('section', 'recent-unlock');
    row.append(el('strong', '', item.achievementName), el('span', '', item.gameTitle), el('small', '', new Date(item.unlockedAt).toLocaleString('zh-CN')));
    content.append(row);
  }
  modal('最近解锁', '保存最近 50 条新解锁，即使错过通知也可以在这里查看。', content);
}
