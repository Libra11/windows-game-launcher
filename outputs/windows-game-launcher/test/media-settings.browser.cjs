const { chromium } = require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'msedge' });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await page.route('**/test/media-settings', route => route.fulfill({
      contentType: 'text/html', body: '<!doctype html><html><body><main id="host"></main></body></html>'
    }));
    await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/media-settings');
    await page.evaluate(async () => {
      const { settingsMedia } = await import('/src/ui/settings-media.js');
      window.calls = [];
      window.mediaCalls = 0;
      window.nextJobs = [];
      window.deferNext = false;
      window.part = settingsMedia({
        command: async (name) => {
          calls.push(name);
          if (name === 'media_settings_snapshot') {
            const data = {
              settings: { playerPath: 'C:/saved.exe', playerKind: 'potplayer' },
              sources: [{ id: 'source', path: 'C:/media', label: '电影', available: true, smartFolders: true }],
              jobs: structuredClone(nextJobs),
            };
            if (deferNext) {
              deferNext = false;
              return new Promise(resolve => { window.releaseSnapshot = () => resolve(data); });
            }
            return data;
          }
          if (name === 'media_douban_status') return { connected: false };
          if (name === 'media_scan') return { jobId: 'scan' };
          throw Error('unexpected: ' + name);
        },
        chooseFile: async () => null,
        toast: () => {},
        media: () => { mediaCalls++; return { browse: () => {} }; },
      });
      document.querySelector('#host').append(part.element);
      await part.activate();
    });
    assert.equal(await page.locator('.settings-media').count(), 1);
    assert.equal(await page.evaluate(() => mediaCalls), 0);
    assert.equal(await page.evaluate(() => calls.filter(name => name === 'media_settings_snapshot').length), 1);
    assert.equal(await page.evaluate(() => calls.includes('media_snapshot')), false);
    const path = page.locator('label.field').filter({ hasText: '播放器程序' }).locator('input');
    await path.fill('C:/unsaved.exe');
    await page.evaluate(() => { nextJobs = [{ id: 'j', kind: 'scan', state: 'running', message: '正在扫描媒体文件', completed: 1, total: 2 }]; });
    await page.getByRole('button', { name: '刷新任务状态' }).click();
    await page.getByText('正在扫描媒体文件').waitFor();
    assert.equal(await path.inputValue(), 'C:/unsaved.exe');
    await page.getByRole('button', { name: '扫描', exact: true }).click();
    await page.waitForFunction(() => calls.includes('media_scan'));
    await page.waitForFunction(() => calls.filter(name => name === 'media_settings_snapshot').length >= 3);
    assert.equal(await path.inputValue(), 'C:/unsaved.exe');
    assert.equal(await page.evaluate(() => mediaCalls), 0);
    assert.equal(await page.evaluate(() => calls.includes('media_snapshot')), false);
    const scanCount = await page.evaluate(() => { deferNext = true; return calls.filter(name => name === 'media_scan').length; });
    await page.getByRole('button', { name: '刷新任务状态' }).click();
    await page.waitForFunction(() => typeof releaseSnapshot === 'function');
    await page.evaluate(() => { nextJobs = [{ id: 'j2', kind: 'scan', state: 'completed', message: '新扫描已完成' }]; });
    await page.getByRole('button', { name: '扫描', exact: true }).click();
    await page.waitForFunction((before) => calls.filter(name => name === 'media_scan').length > before, scanCount);
    await page.evaluate(() => { releaseSnapshot(); releaseSnapshot = null; });
    await page.getByText('新扫描已完成').waitFor();
    assert.equal(await path.inputValue(), 'C:/unsaved.exe');
    const beforeInactive = await page.evaluate(() => {
      part.deactivate();
      return calls.filter(name => name === 'media_settings_snapshot').length;
    });
    await page.waitForTimeout(5500);
    assert.equal(await page.evaluate(() => calls.filter(name => name === 'media_settings_snapshot').length), beforeInactive);
    await page.evaluate(() => part.activate());
    await page.waitForFunction((before) => calls.filter(name => name === 'media_settings_snapshot').length > before, beforeInactive);
    assert.equal(await path.inputValue(), 'C:/unsaved.exe');
    await page.getByRole('button', { name: '浏览', exact: true }).click();
    assert.equal(await page.evaluate(() => mediaCalls), 1);
    await page.evaluate(() => { part.deactivate(); part.dispose(); });
    assert.deepEqual(errors, []);
    process.stdout.write('影视设置轻量加载、任务刷新、扫描和未保存输入保护通过\n');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
