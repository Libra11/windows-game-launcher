import test from 'node:test';
import assert from 'node:assert/strict';
import { mediaPathMatches } from '../src/lib/media-path.js';
import { createMediaPreview } from '../src/lib/media-preview.js';

test('忽略路径匹配包含 Windows 前缀和目录边界，文件不匹配同名前缀', () => {
  assert.ok(mediaPathMatches(String.raw`\\?\D:\视频\Extras`, 'd:/视频/extras/a.mkv'));
  assert.ok(!mediaPathMatches('D:/视频/Extras', 'D:/视频/Extras2/a.mkv'));
  assert.ok(mediaPathMatches(String.raw`\\?\UNC\NAS\电影\片段`, String.raw`\\nas\电影\片段\a.mkv`));
  assert.ok(!mediaPathMatches('D:/a.mkv', 'D:/a.mkv.bak', false));
  assert.ok(!mediaPathMatches('', 'D:/a.mkv'));
});

test('忽略目录后从本地浏览消失，列表可见，取消后重新出现且不影响相邻目录', async () => {
  const command = createMediaPreview();
  const source = (await command('media_snapshot')).sources[0];
  const browse = () => command('media_browse', { sourceId: source.id });
  const before = await browse();
  const entry = before.entries.find((entry) => entry.name === '花絮与样片');
  await command('media_ignore_path', { sourceId: source.id, path: entry.path, directory: true });
  const after = await browse();
  assert.equal(after.entries.length, before.entries.length - 1);
  assert.ok(after.entries.some((entry) => entry.name === '法医秦明'));
  await assert.rejects(
    command('media_browse', { sourceId: source.id, path: entry.path }),
    /已忽略/
  );
  const list = await command('media_ignored_paths');
  assert.equal(list.length, 1);
  assert.equal(list[0].path, entry.path);
  assert.equal((await command('media_snapshot')).sources[0].ignoredCount, 1);
  await command('media_ignore_path', { sourceId: source.id, path: entry.path, remove: true });
  await command('media_scan', { sourceId: source.id });
  assert.equal((await browse()).entries.length, before.entries.length);
  assert.deepEqual(await command('media_ignored_paths'), []);
});

test('文件与目录忽略规则独立，父目录覆盖子项时给出明确标记', async () => {
  const command = createMediaPreview();
  const source = (await command('media_snapshot')).sources[0];
  const folder = source.path + String.raw`\花絮与样片`;
  const file = folder + String.raw`\Sample.mkv`;
  await command('media_ignore_path', { sourceId: source.id, path: file, directory: false });
  assert.equal(
    (await command('media_browse', { sourceId: source.id, path: folder })).entries.length,
    1
  );
  await command('media_ignore_path', { sourceId: source.id, path: folder, directory: true });
  const list = await command('media_ignored_paths', { sourceId: source.id });
  assert.equal(list.find((rule) => rule.path === file).covered, true);
  await command('media_ignore_path', { sourceId: source.id, path: file, remove: true });
  assert.equal((await command('media_ignored_paths')).length, 1);
  await assert.rejects(
    command('media_ignore_path', { sourceId: source.id, path: source.path, directory: true }),
    /子目录/
  );
});

test('预览忽略纯本地已识别资源会删除实体，有记录资源保留历史引用', async () => {
  const command = createMediaPreview();
  const { sources } = await command('media_snapshot');
  const pure = await command('media_detail', { itemId: 'preview-film-3' });
  await command('media_ignore_path', {
    sourceId: sources[0].id,
    path: pure.resources[0].path,
    directory: false,
  });
  await assert.rejects(command('media_detail', { itemId: pure.item.id }), /不存在/);
  const recorded = await command('media_detail', { itemId: 'preview-movie-1' });
  for (const resource of recorded.resources)
    await command('media_ignore_path', {
      sourceId: sources[0].id,
      path: resource.path,
      directory: false,
    });
  const retained = await command('media_detail', { itemId: recorded.item.id });
  assert.equal(retained.item.rating, 5);
  assert.equal(retained.item.playable, false);
  assert.equal(retained.resources.length, 2);
  assert.ok(
    retained.resources.every(
      (resource) => resource.ignored && resource.missing && !resource.available
    )
  );
});
