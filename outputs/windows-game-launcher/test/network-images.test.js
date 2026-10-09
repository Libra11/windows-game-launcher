import test from 'node:test';
import assert from 'node:assert/strict';
import { imageSource, retryImages, failedArtwork } from '../src/lib/network-images.js';
import { artwork } from '../src/lib/dom.js';

test('桌面图片走转发协议，完整保留原地址的查询和中文路径', () => {
  const source = 'https://example.com/中文.png?size=large&token=a+b#image';
  const url = new URL(imageSource(source, true, 7));
  assert.equal(url.host, 'youji-image.localhost');
  assert.equal(url.searchParams.get('url'), source);
  assert.equal(url.searchParams.get('v'), '7');
  assert.equal(imageSource(source, false), source);
  assert.equal(imageSource('/app-icon.png', true), '/app-icon.png');
  assert.equal(imageSource('data:image/png;base64,abc', true), 'data:image/png;base64,abc');
});

test('失败封面重试保留交互节点，旧请求不能再次污染失败记录', () => {
  const previousDocument = globalThis.document, previousTauri = globalThis.isTauri;
  class Element extends EventTarget {
    constructor(tag) {
      super(); this.tagName = tag.toUpperCase(); this.children = []; this.dataset = {}; this.className = '';
      this.classList = {
        contains: value => this.className.split(' ').includes(value),
        add: value => { this.className += ' ' + value; },
        remove: value => { this.className = this.className.split(' ').filter(item => item !== value).join(' '); },
      };
    }
    append(child) { this.children.push(child); child.parentElement = this; }
    remove() { if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(child => child !== this); this.parentElement = null; }
    closest() { return host; }
  }
  const host = new Element('div'), control = new Element('button');
  globalThis.isTauri = true;
  globalThis.document = {
    createElement: tag => new Element(tag),
    querySelectorAll: selector => selector === '[data-artwork-key]' ? [host] : host.children.filter(node => node.tagName === 'IMG'),
  };
  try {
    const source = 'https://example.com/cover.png';
    artwork(host, {id:'game',title:'游戏',metadataJson:JSON.stringify({libraryCovers:[source]})});
    host.append(control);
    const first = host.children.find(node => node.tagName === 'IMG');
    first.onerror(); assert.equal(failedArtwork.has(source), true);
    retryImages();
    assert.equal(host.children.includes(control), true);
    const second = host.children.find(node => node.tagName === 'IMG');
    assert.ok(second); assert.notEqual(second, first);
    first.onerror(); assert.equal(failedArtwork.has(source), false);
  } finally {globalThis.document = previousDocument; globalThis.isTauri = previousTauri; failedArtwork.clear();}
});

test('切换代理清除失败封面记录，重试封面及仍在页面中的图标', () => {
  const previousDocument = globalThis.document;
  const previousTauri = globalThis.isTauri;
  const events = [];
  const host = { classList: {contains() {return false;}}, dispatchEvent(event) { events.push(event.type); } };
  const image = { dataset: {remoteSource: 'https://example.com/icon.png'}, closest() { return null; } };
  const cover = { closest() { return host; } };
  globalThis.isTauri = true;
  globalThis.document = {querySelectorAll(selector) { return selector === '[data-artwork-key]' ? [host] : [image, cover]; }};
  try {
    failedArtwork.add('https://example.com/cover.png');
    retryImages();
    assert.equal(failedArtwork.size, 0);
    assert.deepEqual(events, ['network-image-retry']);
    const first = image.src;
    retryImages();
    assert.notEqual(image.src, first);
    assert.equal(cover.src, undefined);
  } finally {
    globalThis.document = previousDocument;
    globalThis.isTauri = previousTauri;
  }
});
