import test from 'node:test';
import assert from 'node:assert/strict';
import { librarySnapshot, preserveArtwork } from '../src/lib/library-refresh.js';

test('计时和同步时间变化不触发重绘，封面和成就状态变化仍可检测', () => {
  const game = { id:'epic-a', title:'A', metadataJson:'{"cover":"a.png"}', playtime:{seconds:1, checkedAt:'first', state:'ready'}, installation:{state:'unknown',reason:'Epic',checkedAt:'first'}, lastScan:'first', runtime:{state:'idle'} };
  const updated = { ...game, lastScan:'second', runtime:{state:'running'}, playtime:{...game.playtime, seconds:2, checkedAt:'second'}, installation:{...game.installation, checkedAt:'second'} };
  assert.equal(librarySnapshot([game]), librarySnapshot([updated]));
  assert.notEqual(librarySnapshot([game]), librarySnapshot([{...updated,metadataJson:'{"cover":"b.png"}'}]));
  assert.notEqual(librarySnapshot([game]), librarySnapshot([{...updated,scanStatus:'Epic 成就已同步'}]));
});

test('保留原图片节点和加载状态，同时保留新页面的交互控件', () => {
  const node = (tagName, classes = []) => {
    const names = new Set(classes);
    return { tagName, classList:{contains:name=>names.has(name),toggle:(name,value)=>value ? names.add(name) : names.delete(name)}, remove() { this.parent.children = this.parent.children.filter(child=>child!==this); } };
  };
  const host = (key, children, loaded) => {
    const element = { ...node('DIV', loaded ? ['art-loaded'] : []), dataset:{artworkKey:key}, children, prepend(child) { child.remove(); child.parent=this; this.children.unshift(child); } };
    children.forEach(child=>child.parent=element); return element;
  };
  const image = node('IMG'), button = node('BUTTON');
  const previous = host('same', [image], true);
  const next = host('same', [node('IMG'), button], false);
  preserveArtwork({querySelectorAll:()=>[previous]}, {querySelectorAll:()=>[next]});
  assert.equal(next.children[0], image);
  assert.ok(next.children.includes(button));
  assert.ok(next.classList.contains('art-loaded'));
  const changed = host('different', [node('IMG')], false);
  preserveArtwork({querySelectorAll:()=>[next]}, {querySelectorAll:()=>[changed]});
  assert.notEqual(changed.children[0], image);
});
