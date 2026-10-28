import { el, button, icon } from '../../lib/dom.js';
import {
  mediaKindName,
  mediaKindIcon,
  progressPercent,
  secondsLabel,
  safeImageUrl,
  recognitionLabel,
} from '../../lib/media-query.js';
import { cachedMediaArtwork, loadMediaArtwork, retryMediaArtwork } from '../../lib/media-artwork.js';
import { localArtworkKey, mediaArtworkSource } from '../../lib/media-artwork-model.js';

export function panel(title, subtitle = '') {
  const root = el('section', 'media-panel');
  const heading = el('div', 'media-panel-heading');
  const copy = el('div');
  copy.append(el('h2', '', title));
  if (subtitle) copy.append(el('p', '', subtitle));
  heading.append(copy);
  root.append(heading);
  return root;
}
export function emptyState(title, copy, action) {
  const root = el('div', 'empty-state media-empty');
  root.append(icon('film'), el('h2', '', title), el('p', '', copy));
  if (action) root.append(action);
  return root;
}
export function mediaArtwork(item, className = '', role = 'poster') {
  const art = el('div', 'media-art ' + className);
  const url = safeImageUrl(mediaArtworkSource(item, role));
  const placeholder = el('div', 'media-art-placeholder');
  placeholder.append(icon(mediaKindIcon(item.kind)), el('span', '', item.title || '未命名视频'));
  art.append(placeholder);
  if (url || localArtworkKey(item, role)) {
    const image = el('img');
    image.alt = '';
    image.loading = 'lazy';
    image.decoding = 'async';
    image.referrerPolicy = 'no-referrer';
    image.onload = () => {
      art.classList.add('loaded');
      if (className === 'media-cinema-backdrop' && image.naturalWidth && image.naturalHeight)
        art
          .closest('.media-cinema-hero')
          ?.style.setProperty(
            '--media-detail-backdrop-ratio',
            String(image.naturalWidth / image.naturalHeight)
          );
    };
    image.onerror = () => {
      art.classList.remove('loaded');
      if (localArtworkKey(item, role)) image.style.opacity = '0';
      else image.remove();
    };
    art.append(image);
    loadMediaArtwork(image, item, role);
    const source = cachedMediaArtwork(item, role) || url;
    if (source) {
      image.src = source;
      if (image.complete && image.naturalWidth > 0) art.classList.add('loaded');
    }
  }
  return art;
}
export function retainedMediaArtwork(views, key, item, className = '', role = 'poster') {
  const art = views.use(`art:${key}`, [item.id, item.kind, item.title, role,
    mediaArtworkSource(item, role), localArtworkKey(item, role)], () => mediaArtwork(item, className, role));
  const image = art.querySelector('img');
  if (image) retryMediaArtwork(image);
  return art;
}
export function mediaCard(
  item,
  actions,
  { selected = false, batch = false, list = false, displayTitle = item.title, artwork = mediaArtwork } = {}
) {
  const tile = el(
    'article',
    `media-tile ${selected ? 'selected' : ''} ${list ? 'media-tile-list' : ''}`
  );
  tile.draggable = true;
  tile.dataset.mediaItemId = item.id;
  tile.ondragstart = (event) => {
    event.dataTransfer.setData(
      'application/x-youji-media-items',
      JSON.stringify(actions.dragIds?.(item.id) || [item.id])
    );
    event.dataTransfer.effectAllowed = 'copy';
  };
  const open = button(
    '',
    'media-card',
    () => (batch ? actions.toggleSelection(item.id) : actions.open(item)),
    ''
  );
  const recognition = recognitionLabel(item);
  open.setAttribute('aria-label', [displayTitle, mediaKindName(item.kind), recognition].filter(Boolean).join('，'));
  open.dataset.focusKey = 'media-' + item.id;
  const art = artwork({ ...item, title: displayTitle });
  for (const node of art.querySelectorAll('.media-card-badges,.media-card-progress')) node.remove();
  const badges = el('div', 'media-card-badges');
  if (item.watched) badges.append(el('span', 'media-badge watched', '已看'));
  if (recognition) {
    const badge = el('span', 'media-badge media-recognition-badge', recognition);
    if (item.recognitionError && item.recognitionStatus !== 'pending') badge.title = item.recognitionError;
    badges.append(badge);
  }
  if (!item.playable)
    badges.append(el('span', 'media-badge', item.resourceCount ? '离线' : '无资源'));
  art.append(badges);
  if (Number(item.positionSeconds) > 0) {
    const bar = el('div', 'media-card-progress');
    const fill = el('i');
    fill.style.width = progressPercent(item) + '%';
    bar.append(fill);
    bar.setAttribute('role', 'img');
    bar.setAttribute('aria-label', '播放位置 ' + secondsLabel(item.positionSeconds));
    art.append(bar);
  }
  const copy = el('div', 'media-card-copy');
  const title = el('h3', '', displayTitle || '未命名视频');
  title.title = displayTitle || '未命名视频';
  copy.append(title);
  const sub = [item.year, mediaKindName(item.kind), item.rating ? `我的 ${item.rating} 星` : null]
    .filter(Boolean)
    .join(' · ');
  copy.append(el('span', 'media-card-meta', sub));
  if (item.positionSeconds > 0)
    copy.append(
      el('small', 'media-card-position', `上次看到 ${secondsLabel(item.positionSeconds)}`)
    );
  if (item.currentEpisodeTitle)
    copy.append(el('small', 'media-card-episode', item.currentEpisodeTitle));
  open.append(art, copy);
  tile.append(open);
  const favorite = button(
    '',
    `favorite-button media-favorite ${item.favorite ? 'active' : ''}`,
    () => actions.favorite(item),
    'star'
  );
  favorite.title = item.favorite ? '取消收藏' : '收藏';
  favorite.setAttribute('aria-label', favorite.title);
  favorite.setAttribute('aria-pressed', String(!!item.favorite));
  tile.append(favorite);
  if (batch) {
    const choose = el('input', 'media-selection');
    choose.type = 'checkbox';
    choose.checked = selected;
    choose.setAttribute('aria-label', '选择 ' + item.title);
    choose.onchange = () => actions.toggleSelection(item.id);
    tile.append(choose);
  }
  if (item.playable && !batch) {
    const play = button(
      '',
      `media-card-play ${list ? 'in-list' : ''}`,
      () => actions.play?.(item),
      'play'
    );
    play.setAttribute(
      'aria-label',
      `${item.playbackCompleted ? '重新观看' : item.positionSeconds > 0 ? '继续观看' : '播放'} ${displayTitle}`
    );
    play.title = item.playbackCompleted
      ? '重新观看'
      : item.positionSeconds > 0
        ? '继续观看'
        : '立即播放';
    tile.append(play);
  }
  tile.oncontextmenu = (event) => {
    event.preventDefault();
    actions.context?.(item, event);
  };
  tile.addEventListener('keydown', (event) => {
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      event.preventDefault();
      event.stopPropagation();
      actions.context?.(item, event);
    }
  });
  return tile;
}
export function field(label, control, hint = '') {
  const node = el('label', 'field');
  node.append(el('span', '', label), control);
  if (hint) node.append(el('small', 'form-hint', hint));
  return node;
}
export function input(value = '', type = 'text', placeholder = '') {
  const node = el('input');
  node.type = type;
  node.value = value ?? '';
  node.placeholder = placeholder;
  return node;
}
export function check(label, checked = false) {
  const node = el('label', 'media-check');
  const control = input('', 'checkbox');
  control.checked = checked;
  node.append(control, el('span', '', label));
  node.checkbox = control;
  return node;
}
export function jobList(jobs = []) {
  const root = el('div', 'media-jobs');
  if (!jobs.length) {
    root.append(el('p', 'form-hint', '暂时没有后台任务'));
    return root;
  }
  for (const job of [...jobs]
    .sort((a, b) => (b.createdAt || '').localeCompare(a.createdAt || ''))
    .slice(0, 8)) {
    const row = el('div', 'media-job');
    const top = el('div');
    top.append(
      el(
        'strong',
        '',
        job.message ||
          {
            scan: '扫描媒体',
            douban: '更新豆瓣',
            metadata: '识别资料',
            probe: '读取媒体信息',
          }[job.kind] ||
          job.kind
      ),
      el(
        'span',
        '',
        {
          queued: '等待中',
          running: '进行中',
          done: '已完成',
          completed: '已完成',
          failed: '失败',
          paused: '已暂停',
          cancelled: '已取消',
        }[job.state] || job.state
      )
    );
    row.append(top);
    if (job.total > 0) {
      const progress = el('progress');
      progress.max = job.total;
      progress.value = job.completed || 0;
      progress.setAttribute('aria-label', '任务进度');
      row.append(progress, el('small', '', `${job.completed || 0} / ${job.total}`));
    }
    if (job.error) row.append(el('p', 'media-error', job.error));
    root.append(row);
  }
  return root;
}
