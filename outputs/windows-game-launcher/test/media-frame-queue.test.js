import test from 'node:test';
import assert from 'node:assert/strict';
import { createFrameQueue } from '../src/lib/media-frame-queue.js';

test('大量截图仅两个并发，启动前丢弃已离屏请求，失败释放队列', async () => {
  const queue = createFrameQueue();
  let active = 0, maximum = 0, ran = 0;
  const requests = Array.from({ length: 2000 }, (_, index) => queue(async () => {
    ran++;
    maximum = Math.max(maximum, ++active);
    await new Promise(setImmediate);
    active--;
    if (index === 0) throw new Error('抽帧失败');
    return index;
  }, () => index < 20));
  const results = await Promise.all(requests);
  assert.equal(maximum, 2);
  assert.equal(ran, 20);
  assert.equal(results[0], null);
  assert.equal(results[19], 19);
  assert.ok(results.slice(20).every(value => value === null));
});
