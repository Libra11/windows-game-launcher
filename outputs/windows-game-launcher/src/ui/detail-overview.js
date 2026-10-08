import { el, button, icon, metadata } from '../lib/dom.js';
import { hasNoSteamAchievements, epicUnlockError, epicDefinitionsUnavailable } from '../lib/achievement-state.js';
import './detail-overview.css';
import { detectionState } from '../lib/detection-state.js';

function progressSection(game, achievements, detected) {
  const unlocked = achievements.filter(item => item.unlockedAt).length;
  const total = achievements.length;
  const noAchievements = hasNoSteamAchievements(game, achievements) || (game.source === 'epic' && game.schemaSource === 'Epic 官方成就（暂无成就）' && !achievements.length);
  const platform = game.source === 'epic' ? 'Epic' : 'Steam';
  const partial = ['本地记录（仅已解锁）', 'Xbox 本地事件（仅已确认）'].includes(game.schemaSource);
  const known = total > 0 && detected && !partial;
  const progress = known ? Math.round(unlocked / total * 100) : 0;
  const panel = el('section', 'collection-progress');
  panel.append(el('h2', '', '成就进度'));

  if (!noAchievements) {
    const stats = el('div', 'collection-stats');
    for (const [value, label] of [[detected ? unlocked : '—', '已解锁'], [total || '—', partial ? '已发现' : '全部成就']]) {
      const stat = el('div'); stat.append(el('strong', '', value), el('span', '', label)); stats.append(stat);
    }
    panel.append(stats);
  }

  const summary = el('div', 'collection-completion');
  summary.append(el('span', '', noAchievements ? `暂无 ${platform} 成就` : epicDefinitionsUnavailable(game) && !total ? '成就资料暂不可用' : epicUnlockError(game) ? '解锁状态待确认' : known ? '完成度' : partial ? '等待补全定义' : '等待检测'));
  if (known) summary.append(el('strong', '', `${progress}%`));
  panel.append(summary);
  if (known) {
    const track = el('div', 'collection-track');
    track.setAttribute('role', 'progressbar');
    track.setAttribute('aria-label', '成就完成度');
    track.setAttribute('aria-valuemin', '0');
    track.setAttribute('aria-valuemax', '100');
    track.setAttribute('aria-valuenow', String(progress));
    const fill = el('span'); fill.style.width = `${progress}%`; track.append(fill); panel.append(track);
  }
  const hint = noAchievements ? `${platform} 当前未为此游戏提供成就。`
    : partial ? '目前仅发现已解锁项，补全定义后可计算进度。'
    : epicDefinitionsUnavailable(game) && !total ? 'Epic 尚未提供此游戏的商店成就列表，当前无法确认成就总数。'
    : !total ? '成就资料尚未就绪，请查看记录状态。'
    : epicUnlockError(game) ? '成就列表已获取，账号解锁记录暂不可用；已有记录保留，当前无法计算完成度。'
    : !detected ? '成就定义已就绪，等待读取解锁记录。'
    : progress === 100 ? '全部成就已解锁。' : `剩余 ${total - unlocked} 个成就`;
  panel.append(el('p', 'collection-hint', hint));
  return panel;
}

export function collectionOverview(game, achievements, detected, actions) {
  const aside = el('aside', 'detail-overview');
  aside.setAttribute('aria-label', '成就进度与记录状态');
  aside.append(progressSection(game, achievements, detected));
  const status = el('section', 'collection-detection');
  const detection = detectionState(game);
  const stateLabel = el('p', 'detection-state', detection.label); stateLabel.dataset.state = detection.state;
  status.append(el('h3', '', '记录状态'), stateLabel, el('p', 'detection-explanation', detection.detail));
  if (game.achievementPlatform?.definitionError && !epicDefinitionsUnavailable(game)) status.append(el('p', 'form-error', `成就资料获取失败：${game.achievementPlatform.definitionError}。已有缓存和本地解锁仍保留。`));
  const diagnostics = el('details', 'diagnostics');
  const summary = el('summary'); summary.append(el('span', '', '检测详情'), icon('chevron')); diagnostics.append(summary);
  const fields = el('div', 'diagnostic-fields');
  const identity = game.source === 'epic' ? [['Epic 游戏 ID', metadata(game).epicAppName || '—'], ['Epic Sandbox ID', metadata(game).epicNamespace || '—']] : [['Xbox Title ID', game.achievementPlatform?.titleId || '—'], ['Steam AppID', game.appid || '未匹配'], ['公开资料地址', game.achievementPlatform?.publicSource || '—']];
  for (const [label, value] of [['成就平台', game.achievementPlatform?.platform || '尚未确认'], ...identity, ['成就定义', game.schemaSource || '尚未获取'], ['记录路径', game.sourceFile || '尚未找到'], ['最近检查', game.lastScan ? new Date(game.lastScan).toLocaleString('zh-CN') : '尚未检查']]) {
    const field = el('div'); field.append(el('small', '', label), el('span', '', value)); fields.append(field);
  }
  diagnostics.append(fields); status.append(diagnostics);
  if (game.source === 'local') status.append(button('成就检测引导', 'secondary', () => actions.guide(game), 'info'), button('扫描本地记录', 'secondary', () => actions.scan(game), 'refresh'));
  aside.append(status);
  return aside;
}
