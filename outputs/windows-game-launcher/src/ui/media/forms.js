import { el, button, icon } from '../../lib/dom.js';
import { modal, closeModal } from '../modal.js';
import { customSelect } from '../custom-select.js';
import './forms.css';
import { doubanMetadataDialog } from './douban-metadata.js';
import { metadataSearchTitle } from '../../lib/media-detail-model.js';
export { newItemDialog } from './add.js';
import { field, input, check, mediaArtwork, emptyState } from './components.js';
import {
  mediaKindName,
  mediaItemTitle,
  sizeLabel,
  secondsLabel,
  resourceTechnical,
} from '../../lib/media-query.js';

function actions(form, save, cancel) {
  const row = el('div', 'media-form-actions');
  row.append(cancel, save);
  form.append(row);
}
export function confirmAction(title, copy, action, label = '确认') {
  const content = el('div', 'form');
  content.append(el('p', 'form-hint', copy));
  let dialog;
  actions(
    content,
    button(label, 'primary', async () => {
      await action();
      await closeModal(dialog);
    }),
    button('取消', 'secondary', () => closeModal(dialog))
  );
  dialog = modal(title, '', content);
  return dialog;
}
export function recordDialog(controller, item) {
  const content = el('form', 'form media-record-form');
  const watched = check('已观看', !!item.watched);
  let rating = item.rating ?? null;
  const ratingField = el('fieldset', 'media-record-rating');
  ratingField.append(el('legend', '', '我的评分'));
  const stars = el('div', 'media-record-stars');
  const ratingCopy = el('span', 'media-record-rating-copy');
  ratingCopy.setAttribute('aria-live', 'polite');
  const choices = [];
  const labels = ['很差', '较差', '还行', '推荐', '力荐'];
  const updateRating = () => {
    for (const [index, choice] of choices.entries()) {
      choice.control.checked = rating === index + 1;
      choice.label.classList.toggle('filled', rating >= index + 1);
    }
    ratingCopy.textContent = rating == null ? '未评分' : `${rating} 星 · ${labels[rating - 1]}`;
    clearRating.hidden = rating == null;
  };
  for (let value = 1; value <= 5; value++) {
    const label = el('label', 'media-record-star');
    const control = input(String(value), 'radio');
    control.name = 'media-record-rating';
    control.setAttribute('aria-label', `${value} 星，${labels[value - 1]}`);
    control.onchange = () => {
      rating = value;
      updateRating();
    };
    label.title = `${value} 星 · ${labels[value - 1]}`;
    label.append(control, icon('star'));
    choices.push({ label, control });
    stars.append(label);
  }
  const clearRating = button('清除', 'media-record-rating-clear', () => {
    rating = null;
    updateRating();
  });
  updateRating();
  const ratingInfo = el('div', 'media-record-rating-info');
  ratingInfo.append(ratingCopy, clearRating);
  ratingField.append(stars, ratingInfo);
  const comment = el('textarea', 'media-textarea');
  comment.value = item.comment || '';
  comment.rows = 5;
  comment.maxLength = 10000;
  comment.placeholder = '写下你的感受…';
  content.append(watched, ratingField, field('短评', comment));
  let dialog;
  const save = button('保存记录', 'primary', async () => {
    await controller.run('media_set_record', {
      itemId: item.id,
      patch: {
        watched: watched.checkbox.checked,
        rating,
        comment: comment.value,
      },
    });
    await closeModal(dialog);
    await controller.refreshDetail();
  });
  actions(
    content,
    save,
    button('取消', 'secondary', () => closeModal(dialog))
  );
  content.onsubmit = (event) => {
    event.preventDefault();
    save.click();
  };
  const title = mediaItemTitle(item, controller.state.data.items);
  dialog = modal(title, '观影记录', content);
  dialog.classList.add('media-record-modal');
  dialog.setAttribute('aria-label', `${title}，观影记录`);
  return dialog;
}
export function collectionDialog(controller, collection = null) {
  const content = el('form', 'form');
  const name = input(collection?.name || '', 'text', '收藏夹名称');
  name.required = true;
  name.maxLength = 80;
  content.append(field('名称', name));
  let dialog;
  const save = button(collection ? '保存' : '创建', 'primary', async () => {
    if (!name.value.trim()) {
      name.reportValidity();
      return;
    }
    await controller.run('media_organization', {
      action: collection ? 'renameCollection' : 'createCollection',
      id: collection?.id,
      name: name.value.trim(),
    });
    await closeModal(dialog);
    await controller.refresh();
  });
  actions(
    content,
    save,
    button('取消', 'secondary', () => closeModal(dialog))
  );
  content.onsubmit = (event) => {
    event.preventDefault();
    save.click();
  };
  dialog = modal(collection ? '重命名收藏夹' : '新建收藏夹', '一部作品可以加入多个收藏夹', content);
  return dialog;
}
export function collectionManager(controller) {
  const content = el('div', 'form');
  const list = el('div', 'media-collection-manager');
  let dialog;
  for (const collection of controller.state.data.collections) {
    const row = el('div', 'media-manage-row');
    row.append(
      el('strong', '', collection.name),
      el('small', '', `${collection.itemIds?.length || 0} 部`),
      button(
        '重命名',
        'secondary',
        async () => {
          await closeModal(dialog);
          collectionDialog(controller, collection);
        },
        'edit'
      ),
      button(
        '删除',
        'media-text-button',
        async () => {
          await closeModal(dialog);
          confirmAction(
            '删除收藏夹',
            `删除「${collection.name}」只移除收藏夹归属，影视、文件及观看记录均保留。`,
            async () => {
              await controller.run('media_organization', {
                action: 'deleteCollection',
                id: collection.id,
              });
              await controller.refresh();
            },
            '删除收藏夹'
          );
        },
        'trash'
      )
    );
    list.append(row);
  }
  if (!controller.state.data.collections.length)
    list.append(el('p', 'form-hint', '还没有收藏夹。'));
  content.append(
    list,
    button(
      '新建收藏夹',
      'primary',
      async () => {
        await closeModal(dialog);
        collectionDialog(controller);
      },
      'plus'
    )
  );
  dialog = modal('管理影视收藏夹', '整理个人清单', content);
  return dialog;
}
export function organizeDialog(controller, items) {
  const content = el('div', 'form');
  const groups = el('div', 'media-organize-groups');
  const checks = [];
  const tagChecks = [];
  let dialog;
  const collectionGroup = el('div');
  collectionGroup.append(el('h3', '', '收藏夹'));
  for (const collection of controller.state.data.collections) {
    const all = items.every((item) => (collection.itemIds || []).includes(item.id));
    const some = items.some((item) => (collection.itemIds || []).includes(item.id));
    const row = check(collection.name, all);
    row.checkbox.indeterminate = some && !all;
    row.checkbox.dataset.initial = all ? 'all' : some ? 'some' : 'none';
    row.checkbox.onchange = () => (row.checkbox.dataset.changed = 'true');
    checks.push([collection, row.checkbox]);
    collectionGroup.append(row);
  }
  if (!checks.length) collectionGroup.append(el('p', 'form-hint', '先在侧栏创建收藏夹。'));
  groups.append(collectionGroup);
  const tags = el('div');
  tags.append(el('h3', '', '标签'));
  const tagIds = new Set(items.length === 1 ? (items[0].tags || []).map((tag) => tag.id) : []);
  for (const tag of controller.state.data.tags) {
    const row = check(tag.name, tagIds.has(tag.id));
    tagChecks.push([tag, row.checkbox]);
    tags.append(row);
  }
  const tagName = input('', 'text', '新标签名称');
  const tagRow = el('div', 'input-row');
  tagRow.append(
    tagName,
    button(
      '创建',
      'secondary',
      async () => {
        if (!tagName.value.trim()) return;
        await controller.run('media_organization', {
          action: 'createTag',
          name: tagName.value.trim(),
        });
        await controller.refresh();
        await closeModal(dialog);
        organizeDialog(
          controller,
          items.map(
            (item) => controller.state.data.items.find((next) => next.id === item.id) || item
          )
        );
      },
      'plus'
    )
  );
  tags.append(tagRow);
  groups.append(tags);
  content.append(groups);
  const save = button('保存整理', 'primary', async () => {
    for (const [collection, control] of checks) {
      if (control.dataset.changed === 'true')
        await controller.run('media_organization', {
          action: 'setCollectionItems',
          collectionId: collection.id,
          itemIds: items.map((item) => item.id),
          mode: control.checked ? 'add' : 'remove',
        });
    }
    const selected = tagChecks.filter(([, control]) => control.checked).map(([tag]) => tag.id);
    if (items.length === 1)
      await controller.run('media_organization', {
        action: 'setTags',
        itemIds: [items[0].id],
        tagIds: selected,
      });
    else if (selected.length)
      await controller.run('media_organization', {
        action: 'setTags',
        itemIds: items.map((item) => item.id),
        tagIds: selected,
        mode: 'add',
      });
    await closeModal(dialog);
    await controller.refreshDetail();
  });
  if (items.length > 1)
    content.append(
      el('p', 'form-hint', '多选整理仅更改勾选过的收藏夹；勾选标签会批量添加，其他标签保留。')
    );
  actions(
    content,
    save,
    button('取消', 'secondary', () => closeModal(dialog))
  );
  dialog = modal(
    '整理影视',
    items.length === 1 ? items[0].title : `已选 ${items.length} 部`,
    content
  );
  return dialog;
}
export function tagFilterDialog(controller) {
  const content = el('div', 'form');
  const selected = new Set(controller.state.tagIds);
  for (const tag of controller.state.data.tags) {
    const row = check(tag.name, selected.has(tag.id));
    row.checkbox.onchange = () =>
      row.checkbox.checked ? selected.add(tag.id) : selected.delete(tag.id);
    content.append(row);
  }
  if (!controller.state.data.tags.length)
    content.append(el('p', 'form-hint', '还没有标签，可在影视详情的「整理」中创建。'));
  let dialog;
  actions(
    content,
    button('应用筛选', 'primary', async () => {
      controller.state.tagIds = [...selected];
      await closeModal(dialog);
      controller.render();
    }),
    button('清空', 'secondary', async () => {
      controller.state.tagIds = [];
      await closeModal(dialog);
      controller.render();
    })
  );
  dialog = modal('按标签筛选', '选中的标签同时满足', content);
  return dialog;
}
export function identifyDialog(controller, item) {
  const content = el('div', 'form');
  const row = el('form', 'input-row');
  const displayTitle = mediaItemTitle(item, controller.state.data.items);
  const query = input(metadataSearchTitle(item, [...controller.state.data.items, ...(controller.state.detail?.parents || [])]), 'search', '搜索电影或电视剧');
  query.required = true;
  const kind = customSelect(
    [
      ['all', '全部类型'],
      ['movie', '电影'],
      ['series', '电视剧'],
    ],
    ['series', 'season', 'episode'].includes(item.kind)
      ? 'series'
      : item.kind === 'movie'
        ? 'movie'
        : 'all',
    '搜索类型'
  );
  const results = el('div', 'media-identify-results');
  let generation = 0,
    dialog;
  async function search() {
    const version = ++generation;
    if (!query.value.trim()) return;
    results.replaceChildren(el('p', 'form-hint', '正在搜索资料…'));
    try {
      let remoteError = false;
      const response = await controller
        .run(
          'media_search_metadata',
          { itemId: item.id, query: query.value.trim(), kind: kind.value },
          false
        )
        .catch(() => {
          remoteError = true;
          return [];
        });
      if (version !== generation || !dialog?.open) return;
      const candidates = Array.isArray(response)
        ? response
        : response.results || response.items || [];
      results.replaceChildren();
      const local = controller.state.data.items
        .filter(
          (other) =>
            other.id !== item.id &&
            !item.recognized && other.recognized &&
            !(item.externalIds || []).some(left => (other.externalIds || []).some(right =>
              left.provider === right.provider && left.providerType === right.providerType && left.externalId !== right.externalId)) &&
            (!item.externalIds?.some((id) => id.provider === 'douban') ||
              ['movie', 'season'].includes(other.kind)) &&
            mediaItemTitle(other, controller.state.data.items)
              ?.toLocaleLowerCase()
              .includes(query.value.trim().toLocaleLowerCase())
        )
        .slice(0, 12);
      if (local.length) {
        results.append(el('h3', '', '合并到已识别作品'));
        for (const other of local) {
          const choice = button('', 'media-identify-result', async () => {
            const linked = await controller.run('media_identify', {
              itemId: item.id,
              targetItemId: other.id,
            });
            controller.state.selectedId = linked.itemId || linked.id || other.id;
            await closeModal(dialog);
            await controller.refreshDetail();
          });
          const copy = el('div');
          copy.append(
            el('strong', '', mediaItemTitle(other, controller.state.data.items)),
            el('small', '', `${other.year || ''} · ${mediaKindName(other.kind)}`),
            el('p', '', '合并资源与个人记录并移除原条目；本地记录优先，空缺补齐。合并后不能直接撤销。')
          );
          const poster = controller.posterItem?.(other) || other;
          choice.append(mediaArtwork({ ...poster, title: other.title }), copy);
          results.append(choice);
        }
      }
      if (candidates.length) results.append(el('h3', '', 'TMDB 搜索结果'));
      for (const candidate of candidates) {
        const choice = button('', 'media-identify-result', async () => {
          const linked = await controller.run('media_identify', {
            itemId: item.id,
            candidate,
          });
          controller.state.selectedId = linked.itemId || linked.id || item.id;
          await closeModal(dialog);
          await controller.refreshDetail();
        });
        const copy = el('div');
        copy.append(
          el('strong', '', candidate.seriesTitle ? `${candidate.seriesTitle} · ${candidate.seasonTitle || `第 ${candidate.seasonNumber} 季`}` : candidate.title || candidate.name),
          el('small', '', `${candidate.year || ''} · ${mediaKindName(candidate.kind)}`),
          el('p', '', candidate.overview || '暂无简介')
        );
        choice.append(mediaArtwork(candidate), copy);
        results.append(choice);
      }
      if (remoteError)
        results.append(
          el('p', 'media-error', 'TMDB 搜索暂不可用，请检查凭据和网络；仍可关联库内已有条目。')
        );
      else if (!candidates.length && !local.length)
        results.append(emptyState('没有找到匹配资料', '试试原名、别名或去掉年份后重新搜索。'));
    } catch {
      if (dialog?.open)
        results.replaceChildren(el('p', 'media-error', '搜索失败，请检查 TMDB 凭据和网络后重试。'));
    }
  }
  row.append(query, button('搜索', 'primary', search, 'search'));
  row.onsubmit = (event) => {
    event.preventDefault();
    search();
  };
  content.append(
    row,
    kind,
    el('p', 'form-hint', '选择候选后关联资料；评分、短评和日期以本地记录优先，空缺自动补齐。'),
    results
  );
  if (['other', 'movie', 'season'].includes(item.kind) && !item.externalIds?.some(source => source.provider === 'tmdb')) {
    const fallback = el('details');
    fallback.append(el('summary', '', 'TMDB 没有这部作品？'), button('从豆瓣获取资料', 'secondary', async () => {
      await closeModal(dialog);
      doubanMetadataDialog(controller, item);
    }, 'search'));
    content.append(fallback);
  }
  dialog = modal('识别／更正影视资料', displayTitle, content);
  dialog.classList.add('media-identify-modal');
  dialog.addEventListener('close', () => generation++);
  return dialog;
}
export function technicalDialog(controller, resource) {
  const content = el('div', 'form');
  const body = el('div');
  content.append(body);
  const draw = (value) => {
    const info = resourceTechnical(value);
    body.replaceChildren();
    const list = el('dl', 'media-info-list');
    for (const [label, text] of [
      ['文件', value.name || value.path?.split(/[\\/]/).pop()],
      ['路径', value.path],
      ['大小', sizeLabel(value.size)],
      ['修改时间', value.modifiedAt ? new Date(value.modifiedAt).toLocaleString('zh-CN') : '未知'],
      [
        '总码率',
        info.tech.totalBitrate ? `${(info.tech.totalBitrate / 1000000).toFixed(2)} Mbps` : '未知',
      ],
      ['时长', value.durationSeconds ? secondsLabel(value.durationSeconds) : '未知'],
      ['容器', info.format || '未知'],
      ['视频', info.summary || '尚未探测'],
    ]) {
      list.append(el('dt', '', label), el('dd', '', text || '未知'));
    }
    body.append(list);
    for (const [title, tracks] of [
      ['视频轨道', info.videos],
      ['音频轨道', info.audios],
      ['字幕轨道', info.subtitles],
    ]) {
      if (!tracks.length) continue;
      body.append(el('h3', '', title));
      for (const track of tracks) {
        const row = el('div', 'media-track');
        const entries = Object.entries(track).filter(
          ([, v]) => typeof v !== 'object' && v !== null && v !== ''
        );
        const details = el('dl', 'media-info-list');
        for (const [key, val] of entries)
          details.append(
            el(
              'dt',
              '',
              {
                codec: '编码',
                codecLongName: '编码全称',
                index: '轨道编号',
                profile: '编码规格',
                width: '宽',
                height: '高',
                bitrate: '码率',
                channels: '声道',
                channelLayout: '声道布局',
                frameRate: '帧率',
                bitDepth: '位深',
                pixelFormat: '像素格式',
                hdr: '动态范围',
                sampleRate: '采样率',
                language: '语言',
                title: '名称',
                colorTransfer: '传递特性',
                colorPrimaries: '色彩原色',
                colorSpace: '色彩空间',
                default: '默认轨道',
                forced: '强制轨道',
              }[key] || key
            ),
            el('dd', '', String(val))
          );
        row.append(details);
        body.append(row);
      }
    }
    if (info.tech.error) body.append(el('p', 'media-error', info.tech.error));
  };
  draw(resource);
  if (resource.available && !resource.missing)
    content.append(
      button(
        resource.playbackCompleted ? '重新观看' : resource.positionSeconds > 0 ? '续播' : '播放',
        'primary',
        () => controller.play({}, resource, true),
        'play'
      )
    );
  content.append(
    button(
      '重新读取媒体信息',
      'secondary',
      async () => {
        const result = await controller.run('media_probe', {
          resourceId: resource.id,
        });
        draw(result);
        await controller.refresh();
      },
      'refresh'
    )
  );
  const dialog = modal('媒体技术信息', '从文件实际读取，缺失字段保持未知', content);
  dialog.classList.add('media-technical-modal');
  return dialog;
}
