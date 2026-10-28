import './playback-status.css';
import { el, button, icon } from '../../lib/dom.js';
import { secondsLabel, mediaItemTitle } from '../../lib/media-query.js';

function setText(node, value) {
  if (node.textContent !== value) node.textContent = value;
}

export function createPlaybackStatus({ onStop, onOpen, toast, getItem }) {
  const element = el('section', 'media-playback-status');
  element.hidden = true;
  element.setAttribute('role', 'group');
  element.setAttribute('aria-label', '播放器状态');
  const copy = el('div', 'media-playback-copy');
  const title = el('strong', 'media-playback-title');
  const details = el('div', 'media-playback-details');
  const status = el('span');
  status.setAttribute('aria-live', 'polite');
  const time = el('span', 'media-playback-time');
  details.append(status, time);
  const message = el('small', 'media-playback-message');
  message.setAttribute('role', 'status');
  const progress = el('progress');
  progress.setAttribute('aria-label', '当前播放进度');
  copy.append(title, details, message);
  const stop = el('button', 'icon-button');
  stop.type = 'button';
  stop.title = '停止播放';
  stop.setAttribute('aria-label', '停止播放');
  stop.append(icon('close'));
  let current = {},
    stopPending = false;
  const updateStop = () => {
    stop.disabled = stopPending || !current.active || current.state === 'stopping';
  };
  stop.addEventListener('click', async () => {
    if (stop.disabled) return;
    stopPending = true;
    updateStop();
    try {
      await onStop();
    } catch {
      /* 错误由控制器统一提示。 */
    } finally {
      stopPending = false;
      updateStop();
    }
  });
  let currentId = '';
  const locate = button('', 'icon-button', () => onOpen?.(currentId), 'locate');
  locate.title = '定位正在播放';
  locate.setAttribute('aria-label', '定位正在播放');
  element.append(copy, locate, stop, progress);
  let lastError = '';
  function update(value = {}) {
    current = value;
    updateStop();
    element.hidden = !value.active;
    if (!value.active && (value.state === 'error' || value.error)) {
      const error = String(value.error || value.message || '播放器未能启动');
      if (error !== lastError) {
        lastError = error;
        toast(error, true);
      }
    } else lastError = '';
    if (!value.active) return;
    const statusTitle =
      {
        starting: '正在连接播放器',
        playing: '正在播放',
        paused: '播放已暂停',
        buffering: '正在缓冲',
        seeking: '正在跳转',
        stopped: '播放已停止',
        unavailable: '播放记录已暂停',
        untracked: '播放记录已暂停',
        stopping: '正在停止播放器',
      }[value.state] || '播放器运行中';
    setText(status, statusTitle);
    const item = getItem(value.itemId);
    currentId = value.itemId;
    locate.disabled = !item;
    const parent = item && getItem(item.parentId);
    const series = parent && getItem(parent.parentId);
    setText(
      title,
      item ? mediaItemTitle(item, [item, parent, series].filter(Boolean)) : '外部播放器'
    );
    title.title = title.textContent;
    const duration =
      Number.isFinite(value.durationSeconds) && value.durationSeconds > 0
        ? value.durationSeconds
        : 0;
    const position = Number.isFinite(value.positionSeconds)
      ? Math.max(0, value.positionSeconds)
      : 0;
    progress.max = duration || 1;
    progress.value = Math.min(position, progress.max);
    progress.hidden = !duration;
    setText(
      time,
      Number.isFinite(value.positionSeconds)
        ? `${secondsLabel(position)}${duration ? ' / ' + secondsLabel(duration) : ''}`
        : ''
    );
    setText(message, value.message || '');
    message.hidden = !value.message;
    element.dataset.state = value.state || '';
  }
  return { element, update };
}
