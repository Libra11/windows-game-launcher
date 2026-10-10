import test from 'node:test';
import assert from 'node:assert/strict';
import { retryImages, setImageSource, watchNetworkImages, failedArtwork } from '../src/lib/network-images.js';
import { artwork } from '../src/lib/dom.js';

test('图片直接使用原地址，保留查询、中文路径与片段，不包装代理或版本号', () => {
  for (const source of ['https://example.com/中文.png?size=large&token=a+b#image', 'http://example.com/a.png', '/app-icon.png', 'data:image/png;base64,abc']) {
    const image = {dataset:{},addEventListener(){}};
    setImageSource(image, source);
    assert.equal(image.src, source);
    assert.equal(image.dataset.remoteSource, source);
  }
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
    append(...children) { for(const child of children){this.children.push(child);child.parentElement=this;} }
    setAttribute(name,value) { if(name==='class')this.className=String(value); }
    remove() { if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(child => child !== this); this.parentElement = null; }
    closest() { return host; }
  }
  const host = new Element('div'), control = new Element('button');
  globalThis.isTauri = true;
  globalThis.document = {
    createElement: tag => new Element(tag),
    createElementNS: (_namespace,tag) => new Element(tag),
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

test('恢复网络连接后清除失败封面记录，按原地址重试失败封面与图标', () => {
  const previousDocument = globalThis.document;
  const previousTauri = globalThis.isTauri;
  const events = [];
  const host = { children: [], classList: {contains() {return false;}}, dispatchEvent(event) { events.push(event.type); } };
  const image = { dataset: {remoteSource: 'https://example.com/icon.png'}, hidden:true, complete:true, naturalWidth:0, removeAttribute() { delete this.src; }, closest() { return null; } };
  const cover = { closest() { return host; } };
  globalThis.isTauri = true;
  globalThis.document = {querySelectorAll(selector) { return selector === '[data-artwork-key]' ? [host] : [image, cover]; }};
  try {
    failedArtwork.add('https://example.com/cover.png');
    retryImages();
    assert.equal(failedArtwork.size, 0);
    assert.equal(image.hidden, false);
    assert.deepEqual(events, ['network-image-retry']);
    const first = image.src;
    retryImages();
    assert.equal(image.src, first);
    assert.equal(cover.src, undefined);
  } finally {
    globalThis.document = previousDocument;
    globalThis.isTauri = previousTauri;
  }
});

test('恢复网络连接不重试成功或正在加载的图片，新节点继续使用原地址', () => {
  const previousDocument = globalThis.document, previousTauri = globalThis.isTauri;
  const source = 'https://example.com/success.png';
  const changed = [];
  const makeImage = (complete, width) => ({
    dataset:{remoteSource:source}, complete, naturalWidth:width, closest:()=>null,
    removeAttribute() { changed.push('removed'); },
    set src(value) { changed.push(value); },
  });
  const successful = makeImage(true, 100), pending = makeImage(false, 0);
  const hosts = [
    {children:[{tagName:'IMG'}],classList:{contains:()=>true},dispatchEvent(){changed.push('loaded-cover');}},
    {children:[{tagName:'IMG'}],classList:{contains:()=>false},dispatchEvent(){changed.push('pending-cover');}},
  ];
  globalThis.isTauri = true;
  globalThis.document = {querySelectorAll: selector => selector === '[data-artwork-key]' ? hosts : [successful,pending]};
  try {
    retryImages(); retryImages();
    assert.deepEqual(changed, []);
    const recreated = {dataset:{},addEventListener(){}};
    setImageSource(recreated, source);
    assert.equal(recreated.src, source);
  } finally {globalThis.document = previousDocument; globalThis.isTauri = previousTauri;}
});

test('失败图片重试监听 WebView 的联网事件，可以正常取消监听', async () => {
  const previousWindow = globalThis.window, previousDocument = globalThis.document;
  globalThis.window = new EventTarget();
  globalThis.document = {querySelectorAll:()=>[]};
  try {
    const stop = await watchNetworkImages();
    failedArtwork.add('https://example.com/failed.png');
    window.dispatchEvent(new Event('online'));
    assert.equal(failedArtwork.size, 0);
    stop();
    failedArtwork.add('https://example.com/failed.png');
    window.dispatchEvent(new Event('online'));
    assert.equal(failedArtwork.size, 1);
  } finally {globalThis.window = previousWindow; globalThis.document = previousDocument; failedArtwork.clear();}
});
