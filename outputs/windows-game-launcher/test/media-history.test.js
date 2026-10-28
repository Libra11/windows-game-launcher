import test from 'node:test';
import assert from 'node:assert/strict';
import { mediaHistoryRecords, mediaHistorySessions } from '../src/lib/media-history.js';
import { createMediaPreview } from '../src/lib/media-preview.js';

test('预览统计明确区分已看与仅有评分的记录', async () => {
  const preview = createMediaPreview();
  await preview('media_set_record', {
    itemId: 'preview-movie-1',
    patch: { watched: false, rating: 4 },
  });
  const statistics = await preview('media_statistics', { period: 'all' });
  const record = statistics.records.find((record) => record.itemId === 'preview-movie-1');
  assert.equal(record.watchedInRange, false);
  assert.ok(statistics.records.some((record) => record.watchedInRange));
  assert.ok(statistics.records.every((record) => typeof record.watchedInRange === 'boolean'));
});

test('同一作品多天观看时，每个活动日都显示作品，并保留来源标记日期', () => {
  const record = {
    itemId: 'movie',
    activityAt: '2026-10-24',
    markedAt: '2026-10-24',
    activityDates: ['2026-10-24', '2026-10-25', '2026-10-26'],
  };
  const statistics = {
    records: [record],
    sessions: [
      { id: 'first', itemId: 'movie', activityDate: '2026-10-25' },
      { id: 'second', itemId: 'movie', activityDate: '2026-10-26' },
    ],
  };
  for (const date of record.activityDates) {
    const shown = mediaHistoryRecords(statistics, date);
    assert.equal(shown.length, 1);
    assert.equal(shown[0].activityAt, date);
    assert.equal(shown[0].markedAt, '2026-10-24');
  }
  assert.deepEqual(
    mediaHistorySessions(statistics, '2026-10-25').map((row) => row.id),
    ['first']
  );
  assert.deepEqual(mediaHistorySessions(statistics, '2026-10-24'), []);
  assert.deepEqual(mediaHistoryRecords(statistics, '2026-10-23'), []);
  assert.equal(record.activityAt, '2026-10-24');
  assert.equal(mediaHistoryRecords(statistics), statistics.records);
  assert.equal(mediaHistorySessions(statistics).length, 2);
});
