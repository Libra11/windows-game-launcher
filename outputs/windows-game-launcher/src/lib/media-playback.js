import { episodeLabel } from './media-detail-model.js';

// 播放通知只更新进度，不改变用户正在浏览的季集、版本或观影标记。
export function applyMediaPlayback(state, value) {
  if (!value || typeof value !== 'object') return false;
  const previous = state.playerStatus || {};
  if (value.sampleAt != null && previous.sampleAt != null && value.sampleAt < previous.sampleAt)
    return false;
  state.playerStatus = value;
  const switched = previous.resourceId !== value.resourceId || previous.active !== value.active;
  if (!value.active) return switched;
  if (!value.resourceId || !value.itemId || !Number.isFinite(value.positionSeconds))
    return switched;
  const items = state.data.items;
  const current = items.find((item) => item.id === value.itemId);
  const patch = {
    positionSeconds: value.positionSeconds,
    durationSeconds: value.durationSeconds || current?.durationSeconds || 0,
    resumeResourceId: value.resourceId,
  };
  if (
    value.active &&
    patch.durationSeconds > 0 &&
    patch.positionSeconds < patch.durationSeconds * 0.95
  )
    patch.playbackCompleted = false;
  if (value.lastPlayed) patch.lastPlayed = value.lastPlayed;
  if (current) {
    const byId = new Map(items.map((item) => [item.id, item]));
    const seen = new Set();
    let item = current;
    while (item && !seen.has(item.id)) {
      seen.add(item.id);
      Object.assign(item, patch);
      if (item.id !== current.id && current.kind === 'episode')
        item.currentEpisodeTitle = `${episodeLabel(current)} · ${current.title}`;
      item = byId.get(item.parentId);
    }
  }
  for (const resource of state.detail?.resources || []) {
    if (resource.id === value.resourceId)
      Object.assign(resource, {
        positionSeconds: patch.positionSeconds,
        durationSeconds: patch.durationSeconds || resource.durationSeconds,
        ...(patch.playbackCompleted === false ? { playbackCompleted: false } : {}),
      });
  }
  if (state.detail?.item) {
    const updated = items.find((item) => item.id === state.detail.item.id);
    if (updated) Object.assign(state.detail.item, updated);
  }
  return switched;
}
