import { reconcileMediaChildren } from '../../lib/media-view.js';
import { el, button } from '../../lib/dom.js';
import { customSelect } from '../custom-select.js';
import {
  mediaCategories,
  queryMedia,
  mediaKindName,
  mediaItemTitle,
  mediaGenres,
  secondsLabel,
} from '../../lib/media-query.js';
import { retainedMediaArtwork, emptyState } from './components.js';
import { organizeDialog, tagFilterDialog } from './forms.js';
import { statusFilters } from './filters.js';
import { titleTreatment } from './presentation.js';
import { presentationKey } from '../../lib/media-presentation.js';
import './library.css';

function libraryFeature(item, controller) {
  const title = mediaItemTitle(item, controller.state.data.items);
  const presentationItem =
    (item.kind === 'season' &&
      controller.state.data.items.find((value) => value.id === item.parentId)) ||
    item;
  const resume = Number(item.positionSeconds) > 0 && !item.playbackCompleted;
  const hero = el('section', 'media-feature media-cinema-feature');
  hero.dataset.mediaPresentationKey = presentationKey(presentationItem);
  hero.setAttribute('aria-label', `${title}，${resume ? '继续观看' : '库中精选'}`);
  const open = button('', 'media-feature-open', () => controller.open(item));
  open.setAttribute('aria-label', `查看 ${title} 详情`);
  open.dataset.focusKey = 'media-feature-' + item.id;
  hero.append(open);
  hero.append(
    retainedMediaArtwork(controller.views, 'library-feature', item.backdropUrl ? item : presentationItem, 'media-feature-backdrop', 'backdrop')
  );
  const copy = el('div', 'media-feature-copy');
  const heading = titleTreatment(
    { ...presentationItem, title },
    controller.presentation(presentationItem),
    'h2'
  );
  heading.classList.add('media-feature-title');
  copy.append(
    el('div', 'media-feature-kicker', resume ? '接着上次的故事' : '今晚，看点喜欢的'),
    heading
  );
  controller.loadPresentation(presentationItem);
  if (item.kind === 'season' && presentationItem.seasonCount !== 1)
    copy.append(el('p', 'media-feature-season', item.title));
  copy.append(
    el(
      'p',
      'media-feature-meta',
      [
        item.year,
        mediaKindName(item.kind),
        ...mediaGenres(item, controller.state.data.items).slice(0, 3),
      ]
        .filter(Boolean)
        .join(' · ')
    ),
    el('p', 'media-feature-description', item.overview || '属于你的私人片库，随时开启下一段故事。')
  );
  const actions = el('div', 'media-feature-actions');
  if (item.playable)
    actions.append(
      button(
        item.playbackCompleted ? '重新观看' : resume ? '继续观看' : '立即播放',
        'primary',
        () => controller.play(item),
        'play'
      )
    );
  copy.append(actions);
  if (resume) {
    const information = el('p', 'media-feature-information', '已看到 ');
    const position = el('span', '', secondsLabel(item.positionSeconds));
    position.dataset.playbackItemPosition = item.id;
    information.append(position);
    if (item.currentEpisodeTitle)
      information.append(document.createTextNode(' · ' + item.currentEpisodeTitle));
    copy.append(information);
  }
  hero.append(copy);
  return hero;
}

function continueWatching(items, controller) {
  const root = el('section', 'media-continue');
  root.setAttribute('aria-label', '继续观看');
  const heading = el('div', 'media-section-heading');
  heading.append(el('h2', '', '继续观看'), el('span', 'media-continue-hint', '故事，停在这里'));
  root.append(heading);
  const rail = el('div', 'media-continue-rail');
  for (const item of items) {
    const card = controller.views.use('continue:' + item.id, [item, mediaItemTitle(item, controller.state.data.items)], () => {
      const title = mediaItemTitle(item, controller.state.data.items);
      const card = el('article', 'media-continue-card media-tile');
      card.dataset.mediaItemId = item.id;
      const open = button('', 'media-continue-open', () => controller.open(item));
      open.setAttribute('aria-label', `查看 ${title} 详情`);
      const backdropItem = item.backdropUrl
        ? item
        : controller.state.data.items.find((value) => value.id === item.parentId) || item;
      const art = retainedMediaArtwork(controller.views, 'continue:' + item.id, backdropItem, 'media-continue-art', 'backdrop');
      art.querySelector('.media-card-progress')?.remove();
      const progress = el('div', 'media-card-progress');
      const fill = el('i');
      fill.style.width =
        Math.min(
          100,
          item.durationSeconds ? (item.positionSeconds / item.durationSeconds) * 100 : 0
        ) + '%';
      progress.append(fill);
      art.append(progress);
      const copy = el('div', 'media-continue-copy');
      const name = el('strong', '', title);
      name.title = title;
      copy.append(name);
      if (item.currentEpisodeTitle) copy.append(el('small', '', item.currentEpisodeTitle));
      copy.append(
        el('small', 'media-card-position', `上次看到 ${secondsLabel(item.positionSeconds)}`)
      );
      open.append(art, copy);
      card.append(open);
      const play = button(
        '',
        'media-card-play media-continue-play',
        () => controller.play(item),
        'play'
      );
      play.setAttribute('aria-label', `继续观看 ${title}`);
      card.append(play);
      return card;
    });
    rail.append(card);
  }
  root.append(rail);
  return root;
}

export function libraryPage(controller) {
  const { state } = controller;
  const data = state.data;
  const root = controller.views.use('library-root', null, () => el('div', 'media-library'));
  root.classList.remove('media-library-with-feature');
  const children = [];
  const title =
    data.collections.find((value) => value.id === state.collectionId)?.name ||
    mediaCategories.find(([value]) => value === state.category)?.[1] ||
    '全部影视';
  const byId = controller.itemContext().byId;
  const shown = queryMedia(data.items, { ...state, itemIndex: byId }, data.collections);
  const toolbar = controller.views.use('library-toolbar', [title, shown.length, state.category, state.sort, state.view, state.onlyRecognized, state.onlyPlayable, state.watchedFilter, state.tagIds, state.batch], () => {
    const toolbar = el('div', 'collection-toolbar');
    const copy = el('div', 'collection-title');
    copy.append(el('h2', '', title), el('span', 'count-pill', String(shown.length)));
    const controls = el('div', 'collection-controls media-library-controls');
    statusFilters(
      controls,
      state,
      () => {
        state.selection.clear();
        controller.render();
      },
      { category: state.category }
    );
    const sort = customSelect(
      [
        ['az', '名称 A → Z'],
        ['za', '名称 Z → A'],
        ['added', '最近添加优先'],
        ['recent', '最近观看优先'],
        ['rating', '个人评分优先'],
      ],
      state.category === 'recent' ? 'recent' : state.category === 'added' ? 'added' : state.sort,
      '影视排序',
      'sort-select'
    );
    sort.disabled = ['recent', 'added'].includes(state.category);
    sort.onchange = () => {
      state.sort = sort.value;
      controller.render();
    };
    controls.append(sort);
    controls.append(
      button(
        state.tagIds.length ? `标签 · ${state.tagIds.length}` : '标签',
        'secondary',
        () => tagFilterDialog(controller),
        'tags'
      )
    );
    controls.append(
      button(
        state.batch ? '结束整理' : '批量整理',
        'secondary',
        () => {
          state.batch = !state.batch;
          state.selection.clear();
          controller.render();
        },
        'layers'
      )
    );
    const views = el('div', 'view-switch');
    for (const [value, glyph, label] of [
      ['grid', 'library', '封面视图'],
      ['list', 'list', '列表视图'],
    ]) {
      const node = button(
        '',
        state.view === value ? 'icon-button active' : 'icon-button',
        () => {
          state.view = value;
          controller.render();
        },
        glyph
      );
      node.title = label;
      node.setAttribute('aria-label', label);
      node.setAttribute('aria-pressed', String(state.view === value));
      views.append(node);
    }
    controls.append(views);
    toolbar.append(copy, controls);
    return toolbar;
  });
  children.push(toolbar);
  // 顶部展示独立于工具栏筛选；有匹配作品时优先展示，否则保留当前分类的推荐。
  if (!state.collectionId && ['all', 'added', 'recent'].includes(state.category)) {
    const candidates = shown.length
      ? shown
      : queryMedia(data.items, { category: state.category, sort: state.sort });
    const resumable = (items) =>
      items
        .filter(
          (item) => item.playable && Number(item.positionSeconds) > 0 && !item.playbackCompleted
        )
        .sort((a, b) => (b.lastPlayed || '').localeCompare(a.lastPlayed || ''));
    let continuing = resumable(candidates);
    if (state.category === 'recent') {
      if (!continuing.length)
        continuing = resumable(queryMedia(data.items, { category: state.category }));
      if (continuing.length)
        children.unshift(controller.views.use('library-continue', continuing.slice(0, 6).map(item => [item, mediaItemTitle(item, data.items)]), () => continueWatching(continuing.slice(0, 6), controller)));
    } else if (candidates.length) {
      const feature =
        state.category === 'added'
          ? candidates[0]
          : continuing[0] || candidates.find((item) => item.favorite) || candidates[0];
      root.classList.add('media-library-with-feature');
      const parent = data.items.find(item => item.id === feature.parentId);
      children.unshift(controller.views.use('library-feature', [feature, parent, controller.presentation(parent || feature)], () => libraryFeature(feature, controller)));
    }
  }
  if (state.tagIds.length) {
    const chips = el('div', 'media-tags');
    for (const id of state.tagIds) {
      const tag = data.tags.find((tag) => tag.id === id);
      if (tag)
        chips.append(
          button(
            tag.name,
            'organization-chip',
            () => {
              state.tagIds = state.tagIds.filter((value) => value !== id);
              controller.render();
            },
            'close'
          )
        );
    }
    children.push(chips);
  }
  if (state.batch) {
    const batch = el('div', 'media-batch-bar');
    const selected = shown.filter((item) => state.selection.has(item.id));
    batch.append(
      el('strong', '', `已选择 ${selected.length} 部`),
      button(selected.length === shown.length ? '取消全选' : '全选当前结果', 'secondary', () => {
        if (selected.length === shown.length)
          shown.forEach((item) => state.selection.delete(item.id));
        else shown.forEach((item) => state.selection.add(item.id));
        controller.render();
      }),
      button(
        '整理所选',
        'secondary',
        () => (selected.length ? organizeDialog(controller, selected) : undefined),
        'tags'
      )
    );
    if (state.collectionId && selected.length)
      batch.append(
        button(
          '移出当前收藏夹',
          'secondary',
          async () => {
            await controller.run('media_organization', {
              action: 'setCollectionItems',
              collectionId: state.collectionId,
              itemIds: selected.map((item) => item.id),
              mode: 'remove',
            });
            state.selection.clear();
            await controller.refresh();
          },
          'close'
        )
      );
    children.push(batch);
  }
  if (!shown.length) {
    controller.libraryGrid.hide();
    children.push(
      emptyState(
        '没有符合条件的影视',
        '可以调整筛选，添加媒体目录，或导入豆瓣已看记录。',
        button('清除筛选', 'secondary', () => {
          Object.assign(state, {
            search: '',
            onlyRecognized: false,
            onlyPlayable: false,
            tagIds: [],
            watchedFilter: 'all',
          });
          controller.render();
        })
      )
    );
    reconcileMediaChildren(root, children);
    return root;
  }
  const makeCard = (item) =>
      controller.card(item, {
        displayTitle: mediaItemTitle(item, data.items, byId),
        selected: state.selection.has(item.id),
        batch: state.batch,
        list: state.view === 'list',
      });
  let grid;
  if (shown.length > 120) grid = controller.libraryGrid.show(shown, makeCard, state.view);
  else {
    controller.libraryGrid.hide();
    grid = controller.views.use('library-grid', state.view, () => el('div', state.view === 'list' ? 'media-list' : 'media-grid'));
    reconcileMediaChildren(grid, shown.map(makeCard));
  }
  children.push(grid);
  reconcileMediaChildren(root, children);
  return root;
}
