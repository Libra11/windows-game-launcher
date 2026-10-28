import { reconcileMediaChildren } from '../../lib/media-view.js';
import { el, button, icon } from '../../lib/dom.js';
import { customSelect } from '../custom-select.js';
import {
  mediaKindName,
  mediaItemTitle,
  mediaGenres,
  secondsLabel,
  durationLabel,
  resourceTechnical,
  resourceOwnerId,
  recognitionLabel,
} from '../../lib/media-query.js';
import {
  detailModel,
  episodeLabel,
  episodeAvailability,
  metadataSource,
} from '../../lib/media-detail-model.js';
import { retainedMediaArtwork, emptyState } from './components.js';
import {
  recordDialog,
  organizeDialog,
  identifyDialog,
  technicalDialog,
  confirmAction,
} from './forms.js';
import './detail.css';
import { titleTreatment, creditsSection } from './presentation.js';
import { displayMediaPath } from '../../lib/media-path.js';
import { doubanMetadataDialog } from './douban-metadata.js';
import { mediaSourceLinks } from '../../lib/media-source-links.js';
import { localArtworkKey, mediaArtworkSource } from '../../lib/media-artwork-model.js';

function disclosure(title, className = '') {
  const root = el('details', className);
  root.dataset.disclosure = className;
  root.append(el('summary', '', title));
  return root;
}
function versionControl(controller, model) {
  if (model.versions.length < 2) return null;
  const choices = model.versions.filter((value) => value.available && !value.missing);
  if (!choices.length) return null;
  const select = customSelect(
    choices.map((value) => [
      value.id,
      [resourceTechnical(value).summary, value.name || value.path?.split(/[\\/]/).pop()]
        .filter(Boolean)
        .join(' · '),
    ]),
    model.resource?.id,
    model.item.kind === 'season' && !model.episodes.length ? '选择视频文件' : '播放版本'
  );
  select.dataset.focusKey = 'media-playback-version';
  select.onchange = () => {
    controller.state.detailResourceId = select.value;
    controller.render();
  };
  const row = el('div', 'media-version-control');
  row.append(el('span', '', model.item.kind === 'season' && !model.episodes.length ? '视频文件' : '播放版本'), select);
  return row;
}
function livePosition(resource, position) {
  const node = el('span', '', secondsLabel(position));
  if (resource) node.dataset.playbackResource = resource.id;
  return node;
}

function management(controller, item) {
  const actions = el('div', 'media-detail-tools');
  actions.setAttribute('role', 'group');
  actions.setAttribute('aria-label', '作品管理');
  const organize = button(
    '整理',
    'media-detail-tool',
    () => organizeDialog(controller, [item]),
    'tags'
  );
  organize.title = '整理收藏夹和标签';
  const identify = button(
    item.recognized ? '重新识别' : '识别',
    'media-detail-tool',
    () => identifyDialog(controller, item),
    'search'
  );
  identify.title = '识别或更正作品资料';
  actions.append(organize, identify);
  const tmdb = item.externalIds?.some((source) => source.provider === 'tmdb');
  const douban = item.externalIds?.some((source) => source.provider === 'douban');
  if (tmdb || (douban && item.recognized)) {
    actions.append(
      button(
        tmdb ? '刷新资料' : '更新豆瓣资料',
        'media-detail-tool',
        async () => {
          if (!tmdb) return doubanMetadataDialog(controller, item);
          await controller.run('media_refresh_metadata', { itemId: item.id });
          await controller.refreshDetail();
          await controller.loadPresentation(item, true);
        },
        'refresh'
      ),
      button(
        '解除识别',
        'media-detail-tool media-detail-unlink',
        () =>
          confirmAction(
            '解除资料识别',
            '保留本地资源、评分和观看记录，之后可以重新识别。',
            async () => {
              await controller.run('media_identify', {
                itemId: item.id,
                unlink: true,
              });
              await controller.refreshDetail();
            },
            '解除识别'
          ),
        'close'
      )
    );
  }
  actions.append(button('清理条目', 'media-detail-tool media-detail-unlink', () => controller.cleanup(item), 'trash'));
  return actions;
}

function personalRecord(item) {
  const root = el('section', 'media-personal-record');
  const copy = el('div');
  copy.append(
    el(
      'h2',
      '',
      item.kind === 'season'
        ? item.seasonNumber == null ? `我的观影 · ${item.title}` : `我的观影 · 第 ${item.seasonNumber} 季`
        : item.kind === 'series'
          ? '我的观影 · 整剧'
          : '我的观影'
    ),
    el(
      'p',
      '',
      [
        item.watched ? '已观看' : '未标记已看',
        item.rating ? `我的评分 ${item.rating} / 5` : null,
        item.markedAt ? item.markedAt.slice(0, 10) : null,
        item.recordSource === 'douban' ? '来自豆瓣的个人记录' : null,
      ]
        .filter(Boolean)
        .join(' · ')
    )
  );
  root.append(copy);
  if (item.comment) root.append(el('blockquote', 'media-personal-comment', item.comment));
  if (item.tags?.length) {
    const tags = el('div', 'media-tags');
    for (const tag of item.tags) tags.append(el('span', 'media-tag', tag.name || tag));
    root.append(tags);
  }
  return root;
}

function resourcesSection(controller, model) {
  const { item, target, versions } = model;
  const root = disclosure(
    `本地资源${versions.length ? ` · ${versions.length} 个${item.kind === 'season' && !model.episodes.length ? '文件' : '版本'}` : ''}`,
    'media-resource-disclosure'
  );
  if (!versions.length)
    root.append(el('p', 'form-hint', '暂无关联的视频文件。作品资料与个人记录仍然保留。'));
  for (const resource of versions) {
    const row = el('article', 'media-resource-row');
    const text = el('div');
    text.append(
      el('strong', '', resource.name || resource.path?.split(/[\\/]/).pop() || '视频文件'),
      el(
        'small',
        '',
        [
          resource.ignored
            ? '已忽略'
            : resource.missing
              ? '文件已移除'
              : resource.available
                ? '可播放'
                : '离线',
          resourceTechnical(resource).summary,
        ]
          .filter(Boolean)
          .join(' · ')
      ),
      el('small', 'media-path', displayMediaPath(resource.path))
    );
    const tools = el('div', 'media-resource-tools');
    if (resource.available && !resource.missing) {
      tools.append(
        button(
          resource.playbackCompleted
            ? '重看此版本'
            : resource.positionSeconds > 0
              ? '续播此版本'
              : '播放此版本',
          'secondary',
          () => controller.play(target, resource, true),
          'play'
        )
      );
      if (resource.positionSeconds > 0 && !resource.playbackCompleted)
        tools.append(
          button('从头播放', 'secondary', () => controller.play(target, resource, false), 'refresh')
        );
    }
    tools.append(
      button('媒体信息', 'secondary', () => technicalDialog(controller, resource), 'info')
    );
    if (!resource.ignored)
      tools.append(
        button('忽略此文件', 'media-text-button', () =>
          controller.ignorePath({
            sourceId: resource.sourceId,
            path: resource.path,
            directory: false,
          })
        )
      );
    const ownerId = resourceOwnerId(target, resource, [...controller.state.data.items, item]);
    if (ownerId)
      tools.append(
        button(
          '解除关联',
          'media-text-button',
          () =>
            confirmAction(
              '解除资源关联',
              '只解除该文件与作品的关联，不删除媒体文件和观影记录。',
              async () => {
                await controller.run('media_identify', {
                  itemId: ownerId,
                  resourceId: resource.id,
                  unlink: true,
                });
                await controller.refreshDetail();
              },
              '解除关联'
            ),
          'close'
        )
      );
    row.append(text, tools);
    root.append(row);
  }
  return root;
}

function episodeSection(controller, model) {
  const { state } = controller;
  const { season, episode, visibleEpisodes, seasons, item, target, resource } = model;
  const root = el('section', 'media-episode-section');
  root.setAttribute('aria-label', '剧集与播放');
  const heading = el('div', 'media-section-heading');
  const title = el('div');
  title.append(
    el('h2', '', '选集'),
    el(
      'span',
      '',
      `共 ${model.seasonEpisodes.length} 集 · ${model.seasonEpisodes.filter((v) => v.playable).length} 集可播放`
    )
  );
  heading.append(title);
  const filter = button(
    '仅看有资源',
    state.detailOnlyResources ? 'secondary active' : 'secondary',
    () => {
      state.detailOnlyResources = !state.detailOnlyResources;
      controller.render();
    }
  );
  filter.setAttribute('aria-pressed', String(!!state.detailOnlyResources));
  heading.append(filter);
  if (seasons.length > 1) {
    const select = customSelect(
      seasons.map((value) => [
        value.id,
        value.seasonNumber === 0 ? '特别篇' : `第 ${value.seasonNumber ?? 1} 季`,
      ]),
      season?.id,
      '选择季'
    );
    select.dataset.focusKey = 'media-season';
    select.onchange = () => {
      state.detailSeasonId = select.value;
      state.detailEpisodeId = '';
      state.detailResourceId = '';
      controller.render();
    };
    heading.append(select);
  }
  root.append(heading);
  if (!episode) {
    root.append(
      el(
        'p',
        'form-hint',
        state.detailOnlyResources && model.seasonEpisodes.length
          ? '这一季暂无本地资源，可关闭筛选查看完整集目。'
          : '暂未获取这一季的分集资料，可点击「刷新资料」重试。'
      )
    );
    return root;
  }
  const selected = el('div', 'media-selected-episode');
  // TMDB 单集 posterUrl 本身就是剧照；没有单集剧照时才回退整剧横幅。
  const still = retainedMediaArtwork(controller.views, 'selected-episode',
    episode.posterUrl ? episode : item,
    'media-selected-still',
    episode.posterUrl ? 'poster' : 'backdrop'
  );
  const copy = el('div', 'media-selected-copy');
  copy.append(
    el('div', 'eyebrow', `当前选择 · ${episodeLabel(episode)}`),
    el('h3', '', episode.title),
    el('p', 'media-episode-description', episode.overview || '这集还没有剧情简介。')
  );
  const position = Number(resource?.positionSeconds ?? episode.positionSeconds);
  const duration = resource?.durationSeconds || episode.durationSeconds;
  const info = el('div', 'media-episode-meta');
  info.append(
    el(
      'span',
      '',
      [
        duration ? durationLabel(duration) : null,
        episode.watched ? '已看' : null,
        resourceTechnical(resource || {}).summary,
        !episode.playable ? episodeAvailability(episode) : null,
      ]
        .filter(Boolean)
        .join(' · ')
    )
  );
  copy.append(info);
  const version = versionControl(controller, model);
  if (version) copy.append(version);
  const live = el('p', 'media-episode-meta', '已看到 ');
  live.append(livePosition(resource, position));
  if (resource) copy.append(live);
  const play = button(
    resource || episode.playable
      ? `${resource?.playbackCompleted ? '重新观看' : position > 0 ? '继续观看' : '播放'} ${episodeLabel(episode)}`
      : episodeAvailability(episode),
    'primary media-watch-button',
    () => controller.play(target, resource, true),
    'play'
  );
  play.disabled = !resource && !episode.playable;
  const actions = el('div', 'media-action-row');
  actions.append(
    play,
    button('本集记录', 'media-text-button', () => recordDialog(controller, episode), 'edit')
  );
  copy.append(actions);
  selected.append(still, copy);
  root.append(selected);
  const rail = el('div', 'media-episode-rail');
  rail.setAttribute('aria-label', `${season?.title || item.title}分集`);
  for (const value of visibleEpisodes) {
    const card = controller.views.use('episode:' + value.id, [value, value.id === episode.id, season?.id, item.backdropUrl, !!state.playerStatus.active && state.playerStatus.itemId === value.id], () => {
      const card = el('article', `media-episode-card ${value.id === episode.id ? 'active' : ''}`);
      const choose = button('', 'media-episode-select', () => {
        state.detailEpisodeId = value.id;
        state.detailSeasonId = season?.id || '';
        state.detailResourceId = '';
        controller.render();
      });
      if (state.playerStatus?.active && state.playerStatus.itemId === value.id)
        card.append(el('small', 'media-episode-playing', '正在播放'));
      choose.setAttribute('aria-label', `选择 ${episodeLabel(value)} ${value.title}`);
      choose.setAttribute('aria-pressed', String(value.id === episode.id));
      choose.dataset.focusKey = 'episode-' + value.id;
      const art = retainedMediaArtwork(controller.views, 'episode:' + value.id,
        value.posterUrl ? value : item,
        'media-episode-still',
        value.posterUrl ? 'poster' : 'backdrop'
      );
      art.querySelector('.media-episode-status')?.remove();
      if (value.watched) art.append(el('span', 'media-episode-status', '已看'));
      else if (value.positionSeconds > 0)
        art.append(el('span', 'media-episode-status', secondsLabel(value.positionSeconds)));
      const text = el('div');
      text.append(el('small', '', episodeLabel(value)), el('strong', '', value.title));
      choose.append(art, text);
      card.append(choose);
      if (value.playable) {
        const play = button(
          '',
          'media-episode-quickplay',
          () => {
            state.detailEpisodeId = value.id;
            state.detailSeasonId = season?.id || '';
            return controller.play(value);
          },
          'play'
        );
        play.setAttribute('aria-label', `播放 ${episodeLabel(value)} ${value.title}`);
        play.title = `播放 ${episodeLabel(value)}`;
        card.append(play);
      } else card.append(el('small', 'media-episode-unavailable', episodeAvailability(value)));
      return card;
    });
    rail.append(card);
  }
  root.append(rail);
  return root;
}

export function detailPage(controller) {
  const { state } = controller;
  if (!state.detail) return emptyState('正在读取影视详情', '请稍候…');
  const model = detailModel(state.detail, state.data.items, {
    seasonId: state.detailSeasonId,
    episodeId: state.detailEpisodeId,
    resourceId: state.detailResourceId,
    onlyWithResources: state.detailOnlyResources,
  });
  const { item, target, resource, episode, recordItem } = model;
  const serial = ['series', 'season'].includes(item.kind);
  const displayItem =
    serial && model.season
      ? {
          ...model.season,
          title: mediaItemTitle(model.season, state.data.items),
          overview: model.season.overview || item.overview,
          genres: mediaGenres(model.season, [item, ...state.data.items]),
        }
      : item;
  if (serial && model.season) controller.loadPresentation(model.season);
  if (serial && episode && !state.detailEpisodeId) {
    state.detailEpisodeId = episode.id;
    state.detailSeasonId = model.season?.id || '';
  }
  if (model.season && !state.detailSeasonId) state.detailSeasonId = model.season.id;
  if (resource && !state.detailResourceId) state.detailResourceId = resource.id;
  const root = controller.views.use('detail-root', item.id, () => el('div', 'media-detail media-cinema-detail'));
  const children = [];
  root.dataset.detailId = item.id;
  root.dataset.navigationTitle = `${mediaKindName(displayItem.kind)} / ${displayItem.title}`;
  const hero = controller.views.use('detail-hero', [item, displayItem, target, resource, episode, recordItem, model.seasons.length, controller.presentation(item)], () => {
    const hero = el('section', 'media-cinema-hero');
    hero.setAttribute('aria-label', `${displayItem.title}作品资料`);
    hero.append(
      retainedMediaArtwork(controller.views, `detail-backdrop:${displayItem.id}:${item.id}`,
        displayItem.backdropUrl || localArtworkKey(displayItem, 'backdrop') ? displayItem : item, 'media-cinema-backdrop', 'backdrop')
    );
    const copy = el('div', 'media-cinema-copy');
    const meta = el('div', 'media-cinema-meta');
    const source = metadataSource(recordItem);
    const sourceBadge = el('span', 'media-source-badge', source);
    sourceBadge.title = `资料来源：${source}`;
    sourceBadge.setAttribute('aria-label', sourceBadge.title);
    meta.append(
      sourceBadge,
      el(
        'span',
        '',
        [
          displayItem.year,
          mediaKindName(displayItem.kind),
          ...mediaGenres(displayItem, state.data.items),
        ]
          .filter(Boolean)
          .join(' · ')
      )
    );
    const recognition = recognitionLabel(recordItem);
    if (recognition) {
      const badge = el('span', 'media-source-badge media-recognition-badge', recognition);
      if (recordItem.recognitionError && recordItem.recognitionStatus !== 'pending') badge.title = recordItem.recognitionError;
      meta.insertBefore(badge, sourceBadge.nextSibling);
    }
    copy.append(
      meta,
      controller.views.use('detail-title', [item.id, displayItem.title, controller.presentation(item).logoUrl], () => titleTreatment({ ...item, title: displayItem.title }, controller.presentation(item)))
    );
    if (serial && model.seasons.length > 1)
      copy.append(el('p', 'media-cinema-meta', model.season?.title || ''));
    if (displayItem.overview) {
      const overview = disclosure('完整简介', 'media-overview-expand');
      overview.append(el('p', '', displayItem.overview));
      copy.append(el('p', 'media-cinema-overview', displayItem.overview), overview);
    } else
      copy.append(el('p', 'media-cinema-overview', '还没有作品简介，可以点击「识别」补充资料。'));
    const actions = el('div', 'media-cinema-actions');
    const position = Number(resource?.positionSeconds ?? target.positionSeconds);
    const completed = resource?.playbackCompleted || (!resource && target.playbackCompleted);
    const play = button(
      resource || target.playable
        ? `${completed ? '重新观看' : position > 0 ? '继续观看' : '立即播放'}${episode ? ' · ' + episodeLabel(episode) : ''}`
        : '暂无可播放资源',
      'primary media-watch-button',
      () => controller.play(target, resource, true),
      'play'
    );
    play.disabled = !resource && !target.playable;
    actions.append(play);
    const favorite = button(
      recordItem.favorite ? '已收藏' : '收藏',
      'media-art-action',
      () => controller.favorite(recordItem),
      'star'
    );
    favorite.setAttribute('aria-pressed', String(!!recordItem.favorite));
    actions.append(favorite);
    const watched = button(
      serial
        ? recordItem.watched
          ? '本季已看'
          : '标记已看'
        : recordItem.watched
          ? '已看'
          : '标记已看',
      'media-art-action',
      async () => {
        await controller.run('media_set_record', {
          itemId: recordItem.id,
          patch: { watched: !recordItem.watched },
        });
        await controller.refreshDetail();
      },
      'circle-check'
    );
    watched.setAttribute('aria-pressed', String(!!recordItem.watched));
    if (serial) watched.title = recordItem.watched ? '取消本季已看' : '标记本季已看';
    actions.append(watched);
    const record = button(
      '记录',
      'media-art-action',
      () => recordDialog(controller, recordItem),
      'edit'
    );
    record.title = serial ? '编辑本季观影记录' : '编辑观影记录';
    record.setAttribute('aria-label', record.title);
    actions.append(record);
    const unidentified = episode && !episode.recognized ? episode : recordItem;
    if (episode === unidentified && !unidentified.recognized)
      actions.append(
        button(
          '识别本集',
          'media-art-action',
          () => identifyDialog(controller, unidentified),
          'search'
        )
      );
    copy.append(actions);
    if (!serial || !model.episodes.length) {
      const version = versionControl(controller, model);
      if (version) copy.append(version);
    }
    if (episode) {
      const playable = !!resource || target.playable;
      const progress = el(
        'p',
        'media-cinema-resume',
        `${playable ? (position > 0 ? '接着看' : '即将播放') : '当前选择'} ${episodeLabel(episode)} · ${episode.title} · `
      );
      if (playable) progress.append(livePosition(resource, position));
      else progress.append(document.createTextNode(episodeAvailability(episode)));
      copy.append(progress);
    } else if (position > 0) {
      const progress = el('p', 'media-cinema-resume', '已看到 ');
      progress.append(livePosition(resource, position));
      if (resource?.durationSeconds)
        progress.append(document.createTextNode(' / ' + secondsLabel(resource.durationSeconds)));
      copy.append(progress);
    }
    hero.append(copy);
    const backdrop = hero.querySelector('.media-cinema-backdrop img');
    if (backdrop?.naturalWidth && backdrop.naturalHeight)
      hero.style.setProperty('--media-detail-backdrop-ratio', String(backdrop.naturalWidth / backdrop.naturalHeight));
    return hero;
  });
  children.push(hero);
  const subbar = controller.views.use('detail-subbar', [recordItem, model.seasonEpisodes.length, model.seasonEpisodes.filter(v => v.playable).length, resource], () => {
    const subbar = el('div', 'media-detail-subbar');
    subbar.append(
      el(
        'span',
        '',
        serial && model.seasonEpisodes.length
          ? `${model.seasonEpisodes.length} 集 · ${model.seasonEpisodes.filter((v) => v.playable).length} 集可播放`
          : [
              resource?.durationSeconds ? durationLabel(resource.durationSeconds) : null,
              resourceTechnical(resource || {}).summary,
            ]
              .filter(Boolean)
              .join(' · ')
      ),
      management(controller, recordItem)
    );
    return subbar;
  });
  children.push(subbar);
  const frameItem = episode && localArtworkKey(episode, 'backdrop') ? episode : displayItem;
  if (localArtworkKey(frameItem, 'backdrop') && !mediaArtworkSource(frameItem, 'backdrop')) {
    const frame = controller.views.use('detail-local-frame', frameItem, () => {
      const section = el('section', 'media-local-frame-section');
      section.append(el('h2', '', '本地视频截图'),
        retainedMediaArtwork(controller.views, 'detail-local-frame', frameItem, 'media-local-frame', 'backdrop'));
      return section;
    });
    children.push(frame);
  }
  const seasonOnly = recordItem.recognized && !model.episodes.length && recordItem.externalIds?.some(source => source.provider === 'douban')
    && !recordItem.externalIds?.some(source => source.provider === 'tmdb');
  if (serial && !seasonOnly) children.push(controller.views.use('detail-episodes', [model, state.detailOnlyResources, state.playerStatus.active && state.playerStatus.itemId], () => episodeSection(controller, model)));
  const credits = controller.views.use('detail-credits', [recordItem.id, recordItem.externalIds, controller.presentation(recordItem)], () => creditsSection(controller, recordItem));
  if (credits) children.push(credits);
  children.push(
    controller.views.use('detail-personal', recordItem, () => personalRecord(recordItem)),
    controller.views.use('detail-resources', [item, target, model.versions], () => resourcesSection(controller, model))
  );
  const links = mediaSourceLinks(recordItem);
  if (links.length) {
    const sources = controller.views.use('detail-sources', [recordItem.id, links], () => {
      const sources = disclosure('作品链接', 'media-source-details');
      for (const source of links) {
        const link = el('a', 'media-source-link', `${source.label} ↗`);
        link.href = source.href;
        link.target = '_blank';
        link.rel = 'noopener noreferrer';
        link.onclick = event => {
          event.preventDefault();
          Promise.resolve(controller.openSource(source)).catch(() => {});
        };
        sources.append(link);
      }
      return sources;
    });
    children.push(sources);
  }
  reconcileMediaChildren(root, children);
  return root;
}
