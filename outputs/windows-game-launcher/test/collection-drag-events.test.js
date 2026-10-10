import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import { collectionDragPayload, collectionDrop, createDragClickGuard } from '../src/lib/collection-drag.js';
import { indexOrganization } from '../src/lib/library-organization.js';

// 加载实际界面挂载函数，替换浏览器表面；不会复制其手势或点击处理逻辑。
const source = (await readFile(new URL('../src/ui/collection-drag.js', import.meta.url), 'utf8'))
  .replace(/^import .*;\r?\n/gm, '')
  .replace('export function mountCollectionDrag', 'function mountCollectionDrag')
  .replaceAll('import.meta.hot', 'undefined');

function surface() {
  const listeners = new Map();
  return {
    addEventListener(type, callback, options) {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type).push({ callback, signal:options.signal });
    },
    fire(type, fields = {}) {
      const event = {
        defaultPrevented:false, stopped:false,
        preventDefault() { this.defaultPrevented=true; },
        stopImmediatePropagation() { this.stopped=true; },
        ...fields,
      };
      for (const item of listeners.get(type) || []) {
        if (!item.signal.aborted) item.callback(event);
        if (event.stopped) break;
      }
      return event;
    },
  };
}

function node() {
  const classes = new Set();
  return {
    style:{}, dataset:{}, disabled:false, offsetWidth:300, offsetHeight:60,
    classList:{
      add:name=>classes.add(name), remove:name=>classes.delete(name), contains:name=>classes.has(name),
      toggle:(name,force)=>{if(force)classes.add(name);else classes.delete(name);},
    },
    append() {}, remove() {}, setAttribute() {}, closest() { return null; },
  };
}

for (const batch of [false,true]) {
  test(`实际界面：${batch?'批量勾选':'游戏详情'}在 Esc 后延迟松开仍拦截，下一次点击恢复`, () => {
    const card=node();card.dataset.organizationDragGame='a';
    card.closest=selector=>['[data-organization-drag-game]','#content .library-view'].includes(selector)?card:null;
    const captured=new Set();
    const document={ ...surface(), body:node(), querySelector:()=>null, elementFromPoint:()=>card };
    document.documentElement={
      setPointerCapture:id=>captured.add(id),
      hasPointerCapture:id=>captured.has(id),
      releasePointerCapture:id=>captured.delete(id),
    };
    const window={ ...surface(), getSelection:()=>null };
    const state={
      page:'library', selectedId:'', bigScreen:false, filter:'all', search:'', collectionId:'', view:'grid',
      games:[{ id:'a', title:'游戏 A' }], organizationBatchMode:batch, organizationSelection:new Set(['a']),
      organization:indexOrganization({tags:[],collections:[{id:'one',name:'收藏夹'}],gameTags:[],gameCollections:[]}),
    };
    let now=0, writes=0, navigations=0, toggles=0;
    const mount=runInNewContext(`${source}\nmountCollectionDrag`, {
      document, window, AbortController, innerWidth:1280, innerHeight:720,
      el:node, icon:node, collectionDragPayload, collectionDrop, createDragClickGuard,
      createCollectionDragPreview:()=>({node:node(),hint:node(),position() {},dispose() {}}),
      performance:{ now:()=>now }, setTimeout:()=>1, clearTimeout() {},
      requestAnimationFrame:()=>1, cancelAnimationFrame() {},
    });
    const control=mount(state,{ toast() {}, organizationBatch:()=>{writes++;} },{
      isAvailable:()=>true, beginDrag() {}, endDrag() {}, reveal() {},
      scrollElement:{ scrollTop:0, getBoundingClientRect:()=>({left:0,right:180,top:0,bottom:720}) },
    });
    const pointer={ target:card, pointerId:7, pointerType:'mouse', isPrimary:true, button:0, clientX:400, clientY:200 };
    const click=()=>{
      const event=document.fire('click',{target:card,detail:1});
      if (!event.stopped) { if(batch)toggles++;else navigations++; }
      return event;
    };
    try {
      document.fire('pointerdown',pointer);
      document.fire('pointermove',{...pointer,clientX:420});
      assert.equal(document.body.classList.contains('collection-drag-active'),true);
      document.fire('keydown',{key:'Escape'});
      assert.equal(document.body.classList.contains('collection-drag-active'),false);
      now=5000;
      const released=document.fire('pointerup',pointer);
      assert.equal(released.defaultPrevented,true);
      assert.equal(click().defaultPrevented,true);
      assert.equal(writes+navigations+toggles,0);
      document.fire('pointerdown',pointer);
      document.fire('pointerup',pointer);
      assert.equal(click().defaultPrevented,false);
      assert.equal(batch?toggles:navigations,1);
    } finally { control.dispose(); }
  });
}
