import test from 'node:test';
import assert from 'node:assert/strict';
import { nextFocus, gridNeighbor, gamepadActions, repeatedActions } from '../src/lib/big-screen-navigation.js';

const controller = (buttons = [], axes = [0, 0]) => ({
  mapping: 'standard', axes,
  buttons: Array.from({ length: 17 }, (_, index) => ({ pressed: buttons.includes(index) })),
});

test('空间导航优先移向相邻控件，边界不会跳到反方向', () => {
  const cards = [{ x: 50, y: 100 }, { x: 200, y: 100 }, { x: 350, y: 100 }, { x: 250, y: 10 }];
  assert.equal(nextFocus(cards, 0, 'right'), 1);
  assert.equal(nextFocus(cards, 1, 'right'), 2);
  assert.equal(nextFocus(cards, 0, 'left'), -1);
  assert.equal(nextFocus(cards, 1, 'up'), 3);
  assert.equal(nextFocus([], -1, 'down'), -1);
});

test('游戏网格上下跨行，左右到边界停留，不足一行时选择最近的游戏', () => {
  assert.equal(gridNeighbor(1, 8, 3, 'right'), 2);
  assert.equal(gridNeighbor(2, 8, 3, 'right'), -1);
  assert.equal(gridNeighbor(3, 8, 3, 'left'), -1);
  assert.equal(gridNeighbor(1, 8, 3, 'down'), 4);
  assert.equal(gridNeighbor(4, 8, 3, 'up'), 1);
  assert.equal(gridNeighbor(5, 8, 3, 'down'), 7);
  assert.equal(gridNeighbor(7, 8, 3, 'down'), -1);
  assert.equal(gridNeighbor(0, 8, 3, 'up'), -1);
  assert.equal(gridNeighbor(1, 8, 1, 'down'), 2);
  assert.equal(gridNeighbor(1, 8, 1, 'right'), -1);
  assert.equal(gridNeighbor(-1, 0, 3, 'down'), -1);
});

test('摇杆使用死区和主方向，方向键和面键映射正确', () => {
  assert.deepEqual(gamepadActions(controller([], [.3, -.2])), []);
  assert.deepEqual(gamepadActions(controller([], [.8, .6])), ['right']);
  assert.deepEqual(gamepadActions(controller([], [.4, -.8])), ['up']);
  assert.deepEqual(gamepadActions(controller([12, 0, 1, 4, 5])), ['up', 'confirm', 'back', 'previous', 'next']);
  assert.deepEqual(gamepadActions(null), []);
  assert.deepEqual(gamepadActions({ ...controller([0]), mapping: '' }), []);
});

test('按住确认或返回不重复，松开后可再次触发', () => {
  const held = new Map();
  assert.deepEqual(repeatedActions(['confirm', 'back'], held, 0), ['confirm', 'back']);
  assert.deepEqual(repeatedActions(['confirm', 'back'], held, 1000), []);
  assert.deepEqual(repeatedActions([], held, 1100), []);
  assert.deepEqual(repeatedActions(['confirm'], held, 1200), ['confirm']);
});

test('按住方向在初始延迟后连续移动，松开或失焦可重置', () => {
  const held = new Map();
  assert.deepEqual(repeatedActions(['right'], held, 0), ['right']);
  assert.deepEqual(repeatedActions(['right'], held, 359), []);
  assert.deepEqual(repeatedActions(['right'], held, 360), ['right']);
  assert.deepEqual(repeatedActions(['right'], held, 509), []);
  assert.deepEqual(repeatedActions(['right'], held, 510), ['right']);
  held.clear();
  assert.deepEqual(repeatedActions(['right'], held, 520), ['right']);
});
