import test from 'node:test';
import assert from 'node:assert/strict';
import { canPollCloud } from '../src/lib/cloud-polling.js';

test('云状态轮询仅在已登录、页面可见且未编辑时运行', () => {
  const element = { isConnected: true, hidden: false };
  const doc = { hidden: false, activeElement: { matches: () => false } };
  assert.equal(canPollCloud(element, true, doc), true);
  assert.equal(canPollCloud(element, false, doc), false);
  assert.equal(canPollCloud({ ...element, isConnected: false }, true, doc), false);
  assert.equal(canPollCloud({ ...element, hidden: true }, true, doc), false);
  assert.equal(canPollCloud(element, true, { ...doc, hidden: true }), false);
  assert.equal(canPollCloud(element, true, { ...doc, activeElement: { matches: () => true } }), false);
});
