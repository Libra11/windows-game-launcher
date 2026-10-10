import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';

const source = await readFile(new URL('../src/main.js', import.meta.url), 'utf8');
// 执行实际导航函数，避开 main.js 的桌面桥接初始化及后台定时器。
const selectSource = source.slice(source.indexOf('let selectionVersion ='), source.indexOf('async function launch('));
const backSource = source.slice(source.indexOf('  back: () => {', source.indexOf('const actions =')), source.indexOf('  sync: async game =>'));
const sidebarSource = source.slice(source.indexOf('  back:()=>{', source.indexOf('mountSidebar(')), source.indexOf('  settings: actions.settings, preview'));

function setup({ bigScreen = false, page = 'library', load = async () => [] } = {}) {
  const state = { bigScreen, page, selectedId: '', filter: 'favorites', search: 'game', sort: 'az', view: 'list' };
  const window = { scrollY: 1400, scrollTo(_x, y) { this.scrollY = y; } };
  const document = { activeElement: { dataset: { focusKey: 'game-a' } } };
  let content = { scrollTop: 1100 }, refreshed = 0;
  const renders = [];
  const statistics = { state: { scroll: 900, returnFocus: 'stat-game-a' }, refresh() { refreshed++; } };
  const navigation = runInNewContext(`${selectSource}\nconst actions = {${backSource}}; const sidebar = {${sidebarSource}}; ({select, actions, sidebar});`, {
    state, window, document, statistics, run: load, $: () => content,
    render(focus) {
      renders.push(focus);
      // 模拟短详情导致位置收缩，以及大屏重建容器、恢复焦点时的滚动。
      window.scrollY = state.selectedId ? Math.min(window.scrollY, 250) : 0;
      content = { scrollTop: focus ? 120 : 0 };
    },
  });
  return { ...navigation, state, window, document, renders, statistics, get content() { return content; }, get refreshed() { return refreshed; } };
}

test('详情从顶部打开，返回普通游戏库恢复进入前的位置、焦点和筛选', async () => {
  const app = setup();
  await app.select({ id: 'a' });
  assert.equal(app.window.scrollY, 0);
  app.window.scrollY = 190;
  app.actions.back();
  assert.equal(app.window.scrollY, 1400);
  assert.equal(app.renders.at(-1), 'game-a');
  assert.equal(app.state.selectedId, '');
  assert.equal(app.state.filter, 'favorites');
  assert.equal(app.state.search, 'game');
  assert.equal(app.state.view, 'list');
  app.window.scrollY = 1750;
  await app.select({ id: 'b' });
  app.actions.back();
  assert.equal(app.window.scrollY, 1750, '下一次进入详情重新记录当前位置');
});

test('返回大屏游戏库恢复滚动容器的位置，不被焦点自动滚动覆盖', async () => {
  const app = setup({ bigScreen: true });
  app.window.scrollY = 0;
  await app.select({ id: 'a' });
  app.content.scrollTop = 450;
  app.actions.back();
  assert.equal(app.content.scrollTop, 1100);
  assert.equal(app.window.scrollY, 0);
  assert.equal(app.renders.at(-1), 'game-a');
});

test('从详情点击侧栏品牌返回游戏库也恢复原位置', async () => {
  const app = setup();
  await app.select({ id: 'a' });
  app.sidebar.back();
  assert.equal(app.window.scrollY, 1400);
  assert.equal(app.state.page, 'library');
});

test('详情内切换游戏不覆盖游戏库位置，切换显示模式不误用另一种坐标', async () => {
  const app = setup();
  await app.select({ id: 'a' });
  app.window.scrollY = 190;
  await app.select({ id: 'b' });
  app.actions.back();
  assert.equal(app.window.scrollY, 1400);
  await app.select({ id: 'a' });
  app.state.bigScreen = true;
  app.actions.back();
  assert.notEqual(app.content.scrollTop, 1400);
});

test('异步加载期间返回不会打开过时详情，加载完成前滚动保存最新位置', async () => {
  let finish;
  const app = setup({ load: () => new Promise(resolve => { finish = resolve; }) });
  const cancelled = app.select({ id: 'a' });
  app.actions.back();
  finish([]); await cancelled;
  assert.equal(app.state.selectedId, '');
  const pending = app.select({ id: 'b' });
  app.window.scrollY = 1650;
  finish([]); await pending;
  app.actions.back();
  assert.equal(app.window.scrollY, 1650);
});

test('统计页返回仍恢复统计位置与焦点，并刷新统计', async () => {
  for (const bigScreen of [false, true]) {
    const app = setup({ page: 'statistics', bigScreen });
    await app.select({ id: 'a' });
    app.actions.back();
    assert.equal(bigScreen ? app.content.scrollTop : app.window.scrollY, 900);
    assert.equal(app.renders.at(-1), 'stat-game-a');
    assert.equal(app.refreshed, 1);
    assert.equal(app.state.page, 'statistics');
  }
});
