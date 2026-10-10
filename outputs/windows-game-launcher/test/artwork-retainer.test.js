import test from 'node:test';
import assert from 'node:assert/strict';
import { createArtworkRetainer, restoreArtwork } from '../src/lib/artwork-retainer.js';

function setup() {
  const retain=createArtworkRetainer();let current,requests=0;
  function host(key) {
    const classes=new Set();
    return {dataset:{artworkKey:key,artworkCacheKey:key},children:[],classList:{contains:value=>classes.has(value),toggle:(value,active)=>active?classes.add(value):classes.delete(value)},append(image){
      const parent=image.parentElement;if(parent)parent.children=parent.children.filter(node=>node!==image);
      image.parentElement=this;this.children.push(image);
    }};
  }
  function render(keys,isLibrary,mode='desktop') {
    const previous={firstElementChild:current,querySelectorAll:()=>current?.hosts||[]};
    const next=retain(previous,mode,isLibrary,()=>{
      const hosts=keys.map(key=>{
        const item=host(key);
        if(!restoreArtwork(item)){
          requests++;
          const image={tagName:'IMG',src:key,complete:false,naturalWidth:0};
          image.onload=()=>{image.complete=true;image.naturalWidth=100;image.parentElement.classList.toggle('art-loaded',true);};
          item.append(image);
        }
        return item;
      });
      return {hosts,querySelectorAll:()=>hosts};
    });
    current=next;return next;
  }
  return {render,get requests(){return requests;}};
}

test('游戏库经过详情和设置返回后复用全部封面，不再创建图片请求',()=>{
  const app=setup(),home=app.render(['a','b','c'],true);
  const images=home.hosts.map(host=>host.children[0]);images.forEach(image=>image.onload());
  app.render(['a'],false);
  assert.deepEqual(home.hosts.map(host=>host.children[0]),images,'详情不搬走主页留存的图片');
  app.render([],false);const before=app.requests;
  const returned=app.render(['a','b','c'],true);
  assert.deepEqual(returned.hosts.map(host=>host.children[0]),images);
  assert.ok(returned.hosts.every(host=>host.classList.contains('art-loaded')));
  assert.equal(app.requests,before);
});

test('保留仍在加载的图片，迟到的加载事件更新返回后的新父节点',()=>{
  const app=setup(),home=app.render(['pending'],true),image=home.hosts[0].children[0];
  app.render([],false);const returned=app.render(['pending'],true);
  assert.equal(returned.hosts[0].children[0],image);
  assert.equal(app.requests,1);image.onload();
  assert.ok(returned.hosts[0].classList.contains('art-loaded'));
});

test('相同封面的多个展示位置保持独立图片，普通与大屏不搬空彼此缓存',()=>{
  const app=setup(),home=app.render(['a','a'],true);
  const first=home.hosts[0].children[0],second=home.hosts[1].children[0];assert.notEqual(first,second);
  const big=app.render(['a','a'],true,'big-screen');
  const bigImages=big.hosts.map(host=>host.children[0]);
  assert.notEqual(bigImages[0],first);assert.notEqual(bigImages[1],second);
  assert.deepEqual(home.hosts.map(host=>host.children[0]),[first,second]);
  app.render([],false,'big-screen');const returned=app.render(['a','a'],true);
  assert.deepEqual(returned.hosts.map(host=>host.children[0]),[first,second]);assert.equal(app.requests,4);
  assert.deepEqual(big.hosts.map(host=>host.children[0]),bigImages);
});

test('封面地址发生变化时重新加载，不把旧图片当成新资料',()=>{
  const app=setup(),home=app.render(['old-url'],true);app.render([],false);
  const next=app.render(['new-url'],true);
  assert.notEqual(next.hosts[0].children[0],home.hosts[0].children[0]);assert.equal(app.requests,2);
});
