import { el, button, icon, artwork, metadata, sourceName, sourceIcon } from '../lib/dom.js';
import { achievementList } from './detail-achievements.js';
import { launchButton, favoriteButton, activitySummary } from './game-controls.js';
import { collectionOverview } from './detail-overview.js';
import { removeGameDialog } from './remove-game-dialog.js';
import { epicUnlockError } from '../lib/achievement-state.js';

export function detailView(game, achievements, filter, actions) {
  const root = el('div', 'detail-view');
  root.append(button(actions.backLabel || '返回游戏库', 'detail-back', actions.back, 'back'));
  const hero = el('section', 'game-banner');
  const background = el('div', 'game-banner-art'); artwork(background, game, true);
  const copy = el('div', 'game-banner-copy');
  const source = el('span', 'game-platform'); source.append(icon(sourceIcon(game)), document.createTextNode(sourceName(game)));
  copy.append(source, el('h1', '', game.title)); hero.append(background, copy); root.append(hero);
  const intro = el('section', 'game-intro');
  const description = el('p', 'game-description', metadata(game).description || (game.source === 'epic' ? '这段冒险，等待你来开启。重新导入 Epic 游戏库可更新游戏资料。' : '这段冒险，等待你来开启。更新资料可获取游戏简介。'));
  const buttons = el('div', 'game-actions');
  buttons.append(launchButton(game,actions), favoriteButton(game,actions));
  buttons.append(button('更新资料', 'secondary', () => actions.sync(game), 'refresh'));
  buttons.append(button(game.source === 'local' ? '编辑游戏' : '启动检测设置', 'secondary', () => actions.edit(game), 'settings'));
  if (game.source === 'local') {
    const remove = button('移除游戏', 'secondary remove-game-action', () => removeGameDialog(game, actions), 'trash');
    remove.dataset.focusKey = 'remove-game';
    buttons.append(remove);
  }
  intro.append(description, buttons); root.append(intro);
  root.append(activitySummary(game));
  const detected = !epicUnlockError(game) && (achievements.some(item => item.unlockedAt) || /^(已读取|Steam 成就已同步|Epic 成就已同步)/.test(game.scanStatus || ''));
  const body = el('div', 'detail-body');
  body.append(achievementList(game, achievements, filter, detected, actions), collectionOverview(game, achievements, detected, actions));
  root.append(body); return root;
}
