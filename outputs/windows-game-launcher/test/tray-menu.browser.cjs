const { chromium } = require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    for (const scale of [1, 1.25, 1.5, 2]) {
      const page = await browser.newPage({ viewport: { width: 320, height: 532 }, deviceScaleFactor: scale, reducedMotion: 'reduce' });
      await page.route('**/test/tray-layout', route => route.fulfill({ contentType: 'text/html', body:
        '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="/src/ui/tray-menu.css"><div id="menu"></div>' }));
      await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/tray-layout');
      await page.evaluate(async () => {
        window.actions = [];
        window.view = (await import('/src/lib/tray-menu-view.js')).createTrayMenu(document.querySelector('#menu'), {
          activate: key => actions.push(key), dismiss: () => actions.push('dismiss'),
        });
      });
      for (const theme of ['dark', 'light']) {
        for (const [games, media] of [[0, 0], [5, 4], [5, 5]]) {
          const height = 190 + (Math.max(games, 1) + Math.max(media, 1)) * 38;
          await page.setViewportSize({ width: 320, height: height - 1 });
          await page.evaluate(({ games, media, theme }) => {
            document.documentElement.dataset.theme = theme;
            view.render({ games: Array.from({ length: games }, (_, i) => ({ key: `g${i}`, title: ['明日方舟：终末地', '绝区零', '007 初露锋芒', 'Wallpaper Engine：壁纸引擎', '游戏名称'][i] })),
              media: Array.from({ length: media }, (_, i) => ({ key: `m${i}`, title: ['VIVANT · 第 1 季', '世界奇妙物语 2012 年春之特别篇', '世界末日', '影视名称', '另一部影视'][i] })) }, { focus: true });
          }, { games, media, theme });
          const layout = await page.evaluate(() => {
            const card = document.querySelector('.tray-card');
            return { background: getComputedStyle(document.documentElement).backgroundColor,
              scheme: getComputedStyle(document.documentElement).colorScheme,
              scroll: card.scrollHeight - card.clientHeight, horizontal: card.scrollWidth - card.clientWidth,
              bodyScroll: document.body.scrollHeight - innerHeight };
          });
          assert.deepEqual(layout, { background: 'rgba(0, 0, 0, 0)', scheme: 'normal', scroll: 0, horizontal: 0, bodyScroll: 0 }, `${scale}/${theme}/${games}+${media}`);
        }
      }
      await page.keyboard.press('End');
      assert.equal(await page.locator(':focus').getAttribute('data-key'), 'quit');
      await page.keyboard.press('Escape');
      assert.deepEqual(await page.evaluate(() => actions), ['dismiss']);
      if (scale === 2 && process.env.YOUJI_TRAY_SCREENSHOT) {
        await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
        await page.screenshot({ path: process.env.YOUJI_TRAY_SCREENSHOT, omitBackground: true });
      }
      await page.close();
    }
    console.log('托盘透明背景、深浅主题、4种缩放、空列表／9项／10项布局及键盘关闭均通过');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
