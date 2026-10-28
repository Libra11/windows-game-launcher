import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const script = readFileSync(new URL('../src-tauri/src/media/douban_extract.js', import.meta.url), 'utf8');
function extract(info, { url = 'https://movie.douban.com/subject/42/', body = '', login = false, summaries = {} } = {}) {
  const nodes = {
    '#info': { innerText: info },
    h1: { textContent: '某作品 (2024)' },
    'h1 [property="v:itemreviewed"]': { textContent: '某作品' },
    ...summaries,
  };
  return runInNewContext(script, {
    URL, location: new URL(url),
    document: {
      readyState: 'complete', body: { innerText: body || info },
      querySelector: selector => selector.startsWith('input[type="password"]') && login ? {} : nodes[selector] || null,
      querySelectorAll: () => [],
    },
  });
}

test('公开豆瓣详情从文字读取IMDb和类型，不要求登录或IMDb超链接', () => {
  const page = extract('导演: 某导演\n上映日期: 2024-01-01\nIMDb: tt12345678\n', { login: true });
  assert.equal(page.loggedIn, false);
  assert.equal(page.detail.subjectId, '42');
  assert.equal(page.detail.imdbId, 'tt12345678');
  assert.equal(page.detail.kind, 'movie');
  assert.equal(page.detail.seasonNumber, null);
});

test('简介覆盖真实页面的link-report-intra和旧容器，优先完整简介', () => {
  for (const root of ['#link-report-intra', '#link-report']) {
    const summaries = { [`${root} [property="v:summary"]`]: { textContent: ' 简介内容 ' } };
    assert.equal(extract('上映日期: 2024-01-01', { summaries }).detail.overview, '简介内容');
    for (const full of [`${root} [property="v:summary"].all`, `${root} .all [property="v:summary"]`]) {
      assert.equal(extract('上映日期: 2024-01-01', { summaries: {
        ...summaries, [full]: { textContent: ' 完整简介\n 后半段 ' },
      } }).detail.overview, '完整简介 后半段');
    }
  }
});

test('剧集详情的季数是具体季线索，不从集数猜季号', () => {
  const page = extract('首播: 2023-11-29\n季数: 3\n集数: 6\nIMDb: tt20778346\n');
  assert.equal(page.detail.seasonNumber, 3);
  assert.equal(page.detail.kind, 'series');
  assert.equal(extract('首播: 2023-11-29\n集数: 6\n').detail.seasonNumber, null);
});

test('只读取当前subject的信息栏，不从评论或列表误提取IMDb', () => {
  assert.equal(extract('上映日期: 2024-01-01\n', { body: '某评论 IMDb: tt1234567' }).detail.imdbId, '');
  const page = extract('IMDb: tt1234567\n', { url: 'https://movie.douban.com/people/a/collect' });
  assert.equal(page.isSubject, false);
  assert.equal(page.detail, null);
});
