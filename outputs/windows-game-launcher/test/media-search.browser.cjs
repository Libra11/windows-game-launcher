const { chromium } = require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'msedge' });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await page.route('**/test/media-search', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><html><body><main id="host"></main></body></html>' }));
    await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/media-search');
    await page.evaluate(async () => {
      const { createMediaController } = await import('/src/ui/media/index.js');
      const items = Array.from({ length: 85 }, (_, index) => ({ id: `movie-${index}`, kind: 'movie', title: `共享电影${index}`, recognized: true, playable: false }));
      const people = Array.from({ length: 100000 }, (_, index) => ({ id: index + 1, name: `共享演员${index}`, aliases: [], departments: ['cast'], credits: [{ itemId: 'movie-0', roles: [] }] }));
      people.push({ id: 200001, name: '唯一演员', aliases: [], departments: ['cast'], credits: [{ itemId: 'movie-0', roles: [] }] });
      window.searchTicks = 0;
      window.commandCalls = [];
      window.tickTimer = setInterval(() => searchTicks++, 10);
      window.controller = createMediaController({ command: async (name, args) => {
        commandCalls.push(name);
        if (name === 'media_snapshot') return { items, ...(args.input.includePeople === false ? {} : { people }), sources: [{ id:'s', path:'C:/media', label:'媒体', available:true }], peopleIndex: { total: 85, completed: 85, failed: 0 } };
        if (name === 'media_people_snapshot') return { people, peopleIndex: { total: 85, completed: 85, failed: 0 } };
        if (name === 'media_player_status') return { state: 'idle', active: false };
        if (name === 'media_browse') return { source: { id:'s', path:'C:/media', label:'媒体', available:true }, path:'C:/media', entries: [] };
        throw Error('unexpected: ' + name);
      } });
      document.querySelector('#host').append(controller.element);
      await controller.refresh();
    });
    assert.equal(await page.evaluate(() => commandCalls.includes('media_people_snapshot')), false, '进入影视库不应读取完整人员索引');
    const input = page.getByRole('searchbox', { name: '搜索全影视库的影视和演职员' });
    await input.fill('共享');
    await page.waitForTimeout(350);
    assert.equal(await page.evaluate(() => controller.state.page), 'library');
    const ticksBefore = await page.evaluate(() => searchTicks);
    await input.press('Enter');
    await page.waitForFunction(() => controller.state.page === 'search' && !controller.state.searchPending, { timeout: 30000 });
    const first = await page.evaluate(() => ({
      peopleCount: controller.state.searchResults.people.length,
      itemCount: controller.state.searchResults.items.length,
      visiblePeople: document.querySelectorAll('.media-person-card').length,
      visibleItems: document.querySelectorAll('[data-media-surface="search"] [data-media-item-id]').length,
      people: [...document.querySelectorAll('.media-person-card')].map(node => node.dataset.focusKey),
      ticks: searchTicks,
    }));
    assert.equal(first.peopleCount, 100000);
    assert.equal(first.itemCount, 85);
    assert.equal(first.visiblePeople, 30);
    assert.equal(first.visibleItems, 30);
    assert.ok(first.ticks > ticksBefore, '搜索期间事件循环应继续运行');
    const snapshots = await page.evaluate(() => commandCalls.filter(name => name === 'media_snapshot').length);
    await page.waitForTimeout(5500);
    assert.equal(await page.evaluate(() => commandCalls.filter(name => name === 'media_snapshot').length), snapshots);
    await page.getByRole('button', { name: '刷新结果' }).click();
    await page.waitForFunction((before) => commandCalls.filter(name => name === 'media_snapshot').length > before, snapshots);
    await page.waitForFunction(() => !controller.state.searchPending, { timeout: 30000 });
    const peopleNav = page.getByRole('navigation', { name: '演职员搜索结果分页' });
    await peopleNav.getByRole('button', { name: '下一页' }).click();
    const second = await page.locator('.media-person-card').evaluateAll(nodes => nodes.map(node => node.dataset.focusKey));
    assert.equal(second.length, 30);
    assert.equal(second.some(id => first.people.includes(id)), false);
    await peopleNav.getByRole('spinbutton', { name: '演职员结果页码' }).fill('3334');
    await peopleNav.getByRole('button', { name: '跳转' }).click();
    assert.equal(await page.evaluate(() => controller.state.searchPeoplePage), 3333);
    assert.equal(await page.locator('.media-person-card').count(), 10);
    await peopleNav.getByRole('spinbutton', { name: '演职员结果页码' }).fill('2');
    await peopleNav.getByRole('button', { name: '跳转' }).click();
    await page.getByRole('navigation', { name: '影视搜索结果分页' }).getByRole('button', { name: '下一页' }).click();
    assert.equal(await page.evaluate(() => controller.state.searchItemsPage), 1);
    assert.equal(await page.evaluate(() => controller.state.searchPeoplePage), 1);
    await page.locator('.media-person-card').first().click();
    assert.equal(await page.evaluate(() => controller.state.page), 'person');
    await page.getByRole('button', { name: '返回上一页' }).click();
    await page.waitForFunction(() => controller.state.page === 'search' && !controller.state.searchPending, { timeout: 30000 });
    assert.equal(await page.evaluate(() => controller.state.searchPeoplePage), 1);
    assert.equal(await page.evaluate(() => controller.state.searchItemsPage), 1);
    const newInput = page.getByRole('searchbox', { name: '搜索全影视库的影视和演职员' });
    await newInput.fill('唯一');
    await newInput.evaluate(input => {
      input.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
      input.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true }));
      input.form.requestSubmit();
    });
    await page.waitForTimeout(350);
    assert.equal(await page.evaluate(() => controller.state.searchQuery), '共享');
    await newInput.evaluate(input => input.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true })));
    assert.equal(await page.evaluate(() => controller.state.searchQuery), '共享');
    await page.getByRole('button', { name: '搜索', exact: true }).click();
    await page.waitForFunction(() => controller.state.searchQuery === '唯一' && !controller.state.searchPending, { timeout: 30000 });
    assert.equal(await page.evaluate(() => controller.state.searchResults.people.length), 1);
    assert.equal(await page.evaluate(() => controller.state.searchPeoplePage), 0);
    await page.evaluate(() => controller.browse('s'));
    const localInput = page.getByRole('searchbox', { name: '搜索当前目录' });
    await localInput.fill('视频');
    await page.waitForTimeout(350);
    assert.equal(await page.evaluate(() => controller.state.search), '');
    await localInput.press('Enter');
    assert.equal(await page.evaluate(() => controller.state.search), '视频');
    assert.deepEqual(errors, []);
    await page.evaluate(() => { clearInterval(tickTimer); controller.dispose(); });
    process.stdout.write('手动搜索、十万人员分段处理、分页和返回状态通过\n');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
