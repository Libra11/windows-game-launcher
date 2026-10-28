import { el, button, icon } from '../../lib/dom.js';
import { customSelect } from '../custom-select.js';
import { durationLabel, mediaKindName } from '../../lib/media-query.js';
import { monthDays, shiftMonth } from '../../lib/statistics.js';
import { mediaHistoryRecords, mediaHistorySessions } from '../../lib/media-history.js';
import { panel, emptyState } from './components.js';
import '../statistics.css';

function metric(label, value, unit = '', hint = '') {
  const node = el('section', 'stats-summary-module media-stat-card');
  node.append(el('h2', 'stats-summary-label', label));
  const number = el('div', 'stats-number');
  number.append(el('strong', '', String(value)), el('span', '', unit));
  node.append(number);
  if (hint) node.append(el('p', 'form-hint', hint));
  return node;
}
function distribution(title, rows, valueKey = 'name', format = (value) => String(value)) {
  const section = panel(title);
  const max = Math.max(1, ...rows.map((row) => Number(row.count) || 0));
  const list = el('div', 'media-distribution');
  for (const row of rows) {
    const item = el('div');
    item.append(el('span', '', format(row[valueKey])));
    const bar = el('div', 'media-distribution-track');
    const fill = el('i');
    fill.style.width = ((Number(row.count) || 0) / max) * 100 + '%';
    bar.append(fill);
    item.append(bar, el('strong', '', String(row.count || 0)));
    list.append(item);
  }
  if (!rows.length) list.append(el('p', 'form-hint', '所选范围暂无记录'));
  section.append(list);
  return section;
}
function trend(days, onSelect) {
  const section = panel('观影足迹', '按记录日期统计；点击日期查看当天记录');
  const root = el('div', 'media-history-trend');
  const max = Math.max(1, ...days.map((day) => Number(day.count) || 0));
  for (const day of days) {
    const bar = button('', 'media-trend-day', () => onSelect(day.date));
    bar.title = `${day.date} · ${day.count || 0} 条记录 · 有效播放 ${durationLabel(day.seconds)}`;
    bar.setAttribute('aria-label', bar.title);
    const fill = el('i');
    fill.style.height = Math.max(day.count ? 8 : 2, ((day.count || 0) / max) * 110) + 'px';
    bar.append(fill, el('small', '', day.date.slice(5)));
    root.append(bar);
  }
  if (!days.length) root.append(el('p', 'form-hint', '暂无逐日观影记录'));
  section.append(root);
  return section;
}
function calendar(controller, onSelect) {
  const { state } = controller,
    range = state.statistics;
  const from = range.from.slice(0, 7),
    to = range.to.slice(0, 7);
  state.historyMonth = state.historyMonth || to;
  if (state.historyMonth < from) state.historyMonth = from;
  if (state.historyMonth > to) state.historyMonth = to;
  const section = panel('观影日历', '按实际月份展示，颜色深浅表示当天记录数');
  const header = el('div', 'stats-calendar-header');
  const change = (delta) => {
    state.historyMonth = shiftMonth(state.historyMonth, delta);
    controller.render();
  };
  const previous = button('上个月', 'stats-small-button', () => change(-1), 'back');
  const next = button('下个月', 'stats-small-button', () => change(1), 'arrow');
  previous.disabled = state.historyMonth <= from;
  next.disabled = state.historyMonth >= to;
  previous.dataset.focusKey = 'media-month-prev';
  next.dataset.focusKey = 'media-month-next';
  header.append(previous, el('strong', '', state.historyMonth.replace('-', ' 年 ') + ' 月'), next);
  section.append(header);
  const grid = el('div', 'stats-calendar-grid');
  grid.setAttribute('aria-label', '观影日历');
  for (const day of ['一', '二', '三', '四', '五', '六', '日'])
    grid.append(el('span', 'stats-weekday', day));
  const { offset, dates } = monthDays(state.historyMonth);
  for (let index = 0; index < offset; index++) grid.append(el('span'));
  const days = new Map((range.daily || []).map((day) => [day.date, day]));
  const max = Math.max(1, ...(range.daily || []).map((day) => Number(day.count) || 0));
  for (const date of dates) {
    const day = days.get(date),
      count = Number(day?.count) || 0,
      seconds = Number(day?.seconds) || 0;
    const inRange = date >= range.from && date <= range.to;
    const node = button(String(Number(date.slice(-2))), 'stats-calendar-day', () => onSelect(date));
    node.disabled = !inRange || (!count && !seconds);
    node.dataset.focusKey = 'media-day-' + date;
    node.dataset.level = inRange
      ? String(count ? Math.max(1, Math.ceil((count / max) * 4)) : 0)
      : 'outside';
    node.classList.toggle('today', date === range.to);
    node.title =
      date +
      ' · ' +
      (!inRange
        ? '不在所选周期'
        : `${count} 条观影记录${seconds ? ' · 有效播放 ' + durationLabel(seconds) : ''}`);
    node.setAttribute('aria-label', node.title);
    grid.append(node);
  }
  section.append(grid);
  return section;
}
export function historyPage(controller) {
  const { state } = controller;
  const root = el('div', 'statistics-view media-history');
  const header = el('header', 'stats-page-header');
  const copy = el('div');
  copy.append(el('div', 'stats-kicker', '你的影视生活'), el('h1', '', '观看记录'));
  const range = state.statistics;
  header.append(
    copy,
    el('div', 'stats-date-range', range ? `${range.from} — ${range.to}` : '正在读取观影记录')
  );
  root.append(header);
  const controls = el('div', 'stats-filter-bar media-history-filters');
  for (const [key, label, options] of [
    [
      'period',
      '统计周期',
      [
        ['week', '最近 7 天'],
        ['month', '最近 30 天'],
        ['year', '最近一年'],
        ['all', '全部记录'],
      ],
    ],
    [
      'source',
      '记录来源',
      [
        ['all', '全部来源'],
        ['local', '本机记录'],
        ['douban', '豆瓣导入'],
      ],
    ],
    [
      'kind',
      '作品类型',
      [
        ['all', '全部类型'],
        ['movie', '电影'],
        ['series', '电视剧'],
      ],
    ],
  ]) {
    const select = customSelect(options, state.historyQuery[key], label);
    select.onchange = () => {
      state.historyQuery[key] = select.value;
      state.historyDay = '';
      controller.loadStatistics();
    };
    controls.append(select);
  }
  controls.append(button('刷新', 'secondary', () => controller.loadStatistics(), 'refresh'));
  root.append(controls);
  if (state.statisticsError) root.append(el('p', 'media-error', state.statisticsError));
  if (!range) {
    root.append(emptyState('正在读取观看记录', '首次统计可能需要片刻。'));
    return root;
  }
  const summary = range.summary || {};
  const watched = (range.records || []).filter((record) => record.watchedInRange);
  const cards = el('div', 'media-stat-grid');
  cards.append(
    metric('已看条目', watched.length, '条'),
    metric('记录天数', summary.activeDays || 0, '天'),
    metric(
      '我的平均评分',
      summary.averageRating == null ? '—' : Number(summary.averageRating).toFixed(1),
      '星'
    ),
    metric('有效播放时长', durationLabel(summary.playedSeconds), '')
  );
  root.append(cards);
  const breakdown = el('details', 'media-history-breakdown');
  breakdown.append(el('summary', '', '查看本期已看构成'));
  const breakdownGrid = el('div', 'media-history-breakdown-grid');
  for (const [label, value, unit] of [
    ['电影', summary.watchedMovies || 0, '部'],
    ['电视剧', (summary.watchedSeries || 0) + (summary.watchedSeasons || 0), '条'],
    ['分集记录', summary.watchedEpisodes || 0, '集'],
    ['类型待识别', watched.filter((record) => record.kind === 'other').length, '条'],
  ]) {
    const item = el('div');
    item.append(el('span', '', label), el('strong', '', `${value} ${unit}`));
    breakdownGrid.append(item);
  }
  breakdown.append(breakdownGrid);
  root.append(breakdown);
  const activity = el('div', 'stats-activity-grid media-history-activity');
  activity.append(
    trend(range.daily || [], (date) => {
      state.historyDay = date;
      controller.render();
    }),
    calendar(controller, (date) => {
      state.historyDay = date;
      controller.render();
    })
  );
  root.append(activity);
  const charts = el('div', 'media-distribution-grid');
  charts.append(
    distribution(
      '个人评分分布',
      range.ratingDistribution || [],
      'rating',
      (value) => `${value} 星`
    ),
    distribution('题材分布', range.genreDistribution || [])
  );
  root.append(charts);
  const records = panel(
    '观影记录',
    state.historyDay
      ? `${state.historyDay} 的记录`
      : '所选范围内的观影标记与本机播放活动；来源标记日期独立保留'
  );
  if (state.historyDay)
    records.querySelector('.media-panel-heading').append(
      button(
        '显示全部日期',
        'secondary',
        () => {
          state.historyDay = '';
          controller.render();
        },
        'close'
      )
    );
  const shown = mediaHistoryRecords(range, state.historyDay);
  const list = el('div', 'media-history-records');
  for (const record of shown) {
    const row = button('', 'media-history-record', () =>
      controller.open({ id: record.itemId || record.id })
    );
    const text = el('div');
    text.append(
      el('strong', '', record.title),
      el(
        'small',
        '',
        `${mediaKindName(record.kind)} · ${record.source === 'douban' ? '豆瓣标记' : '本机记录'}`
      )
    );
    if (
      record.markedAt &&
      record.activityAt &&
      record.markedAt.slice(0, 10) !== record.activityAt.slice(0, 10)
    )
      text.append(el('small', '', `来源标记日期：${record.markedAt.slice(0, 10)}`));
    if (record.comment) text.append(el('p', '', record.comment));
    const side = el('div');
    side.append(
      el(
        'time',
        '',
        (record.activityAt || record.markedAt || record.lastPlayed || '').slice(0, 10) || '日期未知'
      ),
      el('small', '', record.rating ? `${record.rating} 星` : '未评分')
    );
    row.append(text, side);
    list.append(row);
  }
  if (!shown.length) list.append(el('p', 'form-hint', '所选范围暂无观影记录'));
  records.append(list);
  root.append(records);
  const shownSessions = mediaHistorySessions(range, state.historyDay);
  if (shownSessions.length) {
    const sessions = panel('本机播放明细', '实际播放期间累计，暂停、跳转与未观测时间不补算');
    for (const session of shownSessions.slice(0, 60)) {
      const row = el('div', 'media-session');
      row.append(
        el('span', '', session.title || '视频'),
        el(
          'time',
          '',
          session.startedAt ? new Date(session.startedAt).toLocaleString('zh-CN') : ''
        ),
        el('strong', '', durationLabel(session.seconds))
      );
      sessions.append(row);
    }
    root.append(sessions);
  }
  const notes = el('details', 'stats-methodology');
  notes.append(
    el('summary', '', '统计说明'),
    el(
      'p',
      'form-hint',
      '豆瓣日期表示来源标记日期，不代表精确播放时间。导入列表中类型未知的条目归入其他，不根据片名推断类型。个人评分按五星保存。有效播放时长仅来自播放器反馈，不根据影片标称时长或已看标记推算。剧、季、集按各自记录范围统计，不由整季已看虚构逐集播放。'
    )
  );
  root.append(notes);
  return root;
}
