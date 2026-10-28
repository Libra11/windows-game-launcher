import test from 'node:test';
import assert from 'node:assert/strict';
import { menuFocusIndex } from '../src/lib/tray-menu-view.js';

test('托盘菜单方向键循环移动，Home 与 End 直达首尾', () => {
  assert.equal(menuFocusIndex(3, -1, 'ArrowDown'), 0);
  assert.equal(menuFocusIndex(3, -1, 'ArrowUp'), 2);
  assert.equal(menuFocusIndex(3, 2, 'ArrowDown'), 0);
  assert.equal(menuFocusIndex(3, 0, 'ArrowUp'), 2);
  assert.equal(menuFocusIndex(3, 1, 'Home'), 0);
  assert.equal(menuFocusIndex(3, 1, 'End'), 2);
});

test('托盘菜单没有可操作项时不产生焦点索引', () => {
  assert.equal(menuFocusIndex(0, -1, 'ArrowDown'), -1);
  assert.equal(menuFocusIndex(0, -1, 'End'), -1);
});
