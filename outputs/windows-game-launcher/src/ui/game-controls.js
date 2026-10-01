import { el, button, icon } from '../lib/dom.js';
import { playTime, playedDate } from '../lib/library-query.js';
import { launchState, installationHint } from '../lib/launch-state.js';
import { playtimeText, playtimeHint } from '../lib/playtime.js';

export function launchButton(game, actions, className = 'primary', focusKey) {
  const node = el('button', className); node.type = 'button';
  node.dataset.launchId = game.id;
  node.dataset.focusKey = focusKey || `launch-${game.id}`;
  node.append(icon('play'), el('span'));
  updateLaunch(node,game);
  node.onclick = async () => {
    if (node.disabled || launchState(actions.game(game.id) || game).disabled) return;
    node.disabled = true;
    try { await actions.launch(game); } catch { /* 启动错误由统一提示显示。 */ }
    finally { updateLaunch(node, actions.game(game.id) || game); }
  };
  return node;
}
function updateLaunch(node, game) {
  const {label,disabled,reason} = launchState(game);
  node.disabled = disabled;
  node.dataset.launchState = ['starting','running'].includes(game.runtime?.state) ? game.runtime.state : game.installation?.state || 'local';
  node.querySelector('span').textContent = label;
  node.setAttribute('aria-label', label);
  node.title = reason || label;
}
export function installationBadge(game, compact = false) {
  const node = el('span','installation-hint'); node.dataset.installationId = game.id;
  node.dataset.installationCompact = String(compact);
  updateInstallation(node,game); return node;
}
function updateInstallation(node,game) {
  const hint = installationHint(game);
  node.hidden = !hint; node.textContent = hint && node.dataset.installationCompact === 'true' ? launchState(game).label : hint;
  node.title = hint;
  node.dataset.state = game.installation?.state || 'checking';
}
export function favoriteButton(game, actions, compact = false, focusKey) {
  const label = game.favorite ? '取消收藏' : '收藏置顶';
  const node = button(compact ? '' : label, compact ? 'favorite-button' : 'secondary favorite-button-wide', () => actions.favorite(game), 'star');
  node.dataset.favoriteId = game.id;
  node.setAttribute('aria-label', `${label} ${game.title}`); node.setAttribute('aria-pressed',String(!!game.favorite));
  node.title = label;
  node.dataset.focusKey = focusKey || `favorite-${game.id}`;
  return node;
}
export function runtimeBadge(game) {
  const node = el('span', 'runtime-badge'); node.dataset.runtimeId = game.id;
  updateBadge(node,game); return node;
}
function updateBadge(node, game) {
  const state = game.runtime?.state || 'idle';
  node.dataset.state = state;
  node.hidden = state === 'idle';
  node.textContent = state === 'running' ? `运行中 · ${playTime(game.runtime.elapsedSeconds)}`
    : state === 'starting' ? '正在启动…' : state === 'error' ? '启动失败' : '未确认运行';
  node.title = game.runtime?.message || '';
}
export function activitySummary(game) {
  const node = el('div', 'play-summary');
  node.append(el('span','',`上次游玩 · ${playedDate(game.playtime?.lastPlayed || game.lastPlayed)}`),playtimeBadge(game),runtimeBadge(game),installationBadge(game));
  return node;
}
export function playtimeBadge(game) {
  const node = el('span', 'game-playtime'); node.dataset.playtimeId = game.id;
  node.append(icon('clock'), el('span'));
  updatePlaytime(node, game); return node;
}
function updatePlaytime(node, game) {
  node.querySelector('span').textContent = playtimeText(game);
  node.title = playtimeHint(game);
  node.dataset.state = game.playtime?.state || (game.source === 'local' ? 'ready' : 'pending');
}
export function patchRuntime(root, games) {
  const byId = new Map(games.map(game => [game.id,game]));
  root.querySelectorAll('[data-launch-id]').forEach(node => { const game = byId.get(node.dataset.launchId); if (game) updateLaunch(node,game); });
  root.querySelectorAll('[data-runtime-id]').forEach(node => { const game = byId.get(node.dataset.runtimeId); if (game) updateBadge(node,game); });
  root.querySelectorAll('[data-installation-id]').forEach(node => { const game = byId.get(node.dataset.installationId); if (game) updateInstallation(node,game); });
  root.querySelectorAll('[data-playtime-id]').forEach(node=> {const game = byId.get(node.dataset.playtimeId); if (game) updatePlaytime(node,game); });
}
