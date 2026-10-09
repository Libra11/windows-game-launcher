import { el, button, icon, metadata } from '../lib/dom.js';
import { setImageSource } from '../lib/network-images.js';
import { achievementEmptyMessage } from '../lib/achievement-state.js';
import { achievementImageSources } from '../lib/achievement-images.js';

export function achievementList(game, achievements, filter, detected, actions) {
  const root = el('section', 'detail-achievements');
  const total = achievements.length;
  const toolbar = el('div', 'achievement-toolbar'); toolbar.append(el('h2', '', `${game.schemaSource === 'Xbox 本地事件（仅已确认）' ? '已确认的 Xbox 成就' : '游戏成就'} · ${total}`));
  const tabs = el('div', 'filter-tabs');
  for (const [value, label] of [['all', '全部'], ['unlocked', '已解锁'], ['locked', '未解锁']]) {
    const tab = button(label, filter === value ? 'active' : '', () => actions.filter(value)); tab.setAttribute('aria-pressed', String(filter === value)); tabs.append(tab);
    tab.dataset.focusKey = `achievement-${value}`;
  }
  toolbar.append(tabs); root.append(toolbar);
  const shown = achievements.filter(item => filter === 'all' || (filter === 'unlocked' ? !!item.unlockedAt : !item.unlockedAt));
  const list = el('div', 'achievement-list');
  const epicDetails = game.source === 'epic' ? metadata(game).epicAchievementDetails || {} : {};
  shown.forEach(item => {
    const details = epicDetails[item.apiName] || {};
    const name = item.unlockedAt && details.unlockedName || item.name;
    const description = item.unlockedAt && details.unlockedDescription || item.description;
    const image = item.unlockedAt && details.unlockedIcon || item.icon;
    const row = el('article', `achievement-card ${item.unlockedAt ? 'unlocked' : ''}`);
    const art = el('div', 'achievement-icon'); art.append(icon('trophy'));
    const sources = achievementImageSources(image);
    if (sources.length) {
      const img = el('img'); img.alt = ''; img.loading = 'lazy';
      img.onerror = () => { if (sources.length) setImageSource(img, sources.shift()); else img.remove(); };
      setImageSource(img, sources.shift()); art.append(img);
    }
    const text = el('div', 'achievement-copy'); text.append(el('h3', '', name), el('p', '', description || (item.hidden ? '隐藏成就' : '完成游戏中的对应挑战')));
    const extra = [Number.isFinite(details.xp) ? `${details.xp} XP` : '', details.tier || '', Number.isFinite(details.rarity) ? `解锁率 ${details.rarity.toFixed(1)}%` : ''].filter(Boolean);
    if (extra.length) text.append(el('small', '', extra.join(' · ')));
    const status = el('div', 'achievement-status');
    if (item.unlockedAt) {
      const badge = el('span', 'unlocked-label'); badge.append(icon('check'), document.createTextNode('已解锁')); status.append(badge);
      status.append(el('small', '', item.unlockSource === 'steam' ? 'Steam 官方' : item.unlockSource === 'epic' ? 'Epic 官方' : item.unlockSource === 'manual' ? '手动记录' : item.unlockSource === 'xbox-local' ? 'Xbox 本地事件' : '本地检测'));
      status.title = `记录时间：${new Date(item.unlockedAt).toLocaleString('zh-CN')}`;
    } else status.append(el('span', 'locked-label', detected ? '尚未解锁' : '尚未检测'));
    if (game.source === 'local' && (!item.unlockedAt || item.unlockSource === 'manual')) status.append(button(item.unlockedAt ? '撤销记录' : '手动解锁', 'manual-button', () => actions.manual(game, item)));
    row.append(art, text, status); list.append(row);
  });
  if (!shown.length) list.append(el('div', 'empty-small', achievementEmptyMessage(game, achievements)));
  root.append(list); return root;
}
