const { chromium } = require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'msedge' });
  try {
    const page = await browser.newPage();
    const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=', 'base64');
    await page.route('https://raw.test/**', route => route.request().url().includes('missing') ? route.fulfill({ status: 404 }) : route.fulfill({ contentType: 'image/png', body: png }));
    await page.route('https://local.test/**', route => route.request().url().includes('invalid') ? route.fulfill({ status: 404 }) : route.fulfill({ contentType: 'image/png', body: png }));
    await page.route('**/test/artwork', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><html><head><style>.media-art{width:40px;height:40px}.media-art img{width:40px;height:40px}</style></head><body></body></html>' }));
    await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/artwork');
    await page.evaluate(async () => {
      window.isTauri = true;
      window.pendingArtwork = [];
      window.__TAURI_INTERNALS__ = {
        invoke: () => new Promise(resolve => pendingArtwork.push(resolve)),
        convertFileSrc: path => `https://local.test/${path}`,
      };
      window.mediaArtwork = (await import('/src/ui/media/components.js')).mediaArtwork;
      window.item = { id: 'fast', kind: 'movie', title: '测试', posterUrl: 'https://raw.test/fast.png' };
      document.body.append(mediaArtwork(item));
    });
    await page.waitForFunction(() => pendingArtwork.length && document.querySelector('.media-art.loaded'));
    await page.evaluate(() => pendingArtwork.shift()({ path: 'cached.png' }));
    await page.waitForTimeout(80);
    assert.equal(await page.locator('.media-art img').first().getAttribute('src'), 'https://raw.test/fast.png', '已经显示的图片不切换来源');
    await page.evaluate(() => document.body.append(mediaArtwork(item)));
    assert.equal(await page.locator('.media-art img').nth(1).getAttribute('src'), 'https://local.test/cached.png', '新节点直接使用已知的本地缓存');
    await page.evaluate(() => {
      window.missing = { id: 'missing', kind: 'movie', title: '缺图', posterUrl: 'https://raw.test/missing.png' };
      document.body.append(mediaArtwork(missing));
    });
    await page.waitForFunction(() => pendingArtwork.length);
    await page.evaluate(() => pendingArtwork.shift()({ path: 'fallback.png' }));
    await page.waitForFunction(() => document.querySelectorAll('.media-art.loaded').length === 3);
    assert.equal(await page.locator('.media-art img').nth(2).getAttribute('src'), 'https://local.test/fallback.png');
    await page.evaluate(() => {
      window.invalid = { id: 'invalid', kind: 'movie', title: '失效缓存', posterUrl: 'https://raw.test/valid.png' };
      document.body.append(mediaArtwork(invalid));
    });
    await page.waitForFunction(() => pendingArtwork.length);
    await page.evaluate(() => pendingArtwork.shift()({ path: 'invalid.png' }));
    await page.waitForTimeout(80);
    await page.evaluate(() => document.body.append(mediaArtwork(invalid)));
    await page.waitForFunction(() => document.querySelectorAll('.media-art.loaded').length === 5);
    assert.equal(await page.locator('.media-art img').nth(4).getAttribute('src'), 'https://raw.test/valid.png', '失效本地缓存回退网络地址');
    console.log('图片来源切换、本地缓存复用和失败回退通过');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
