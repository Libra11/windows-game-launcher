import test from 'node:test';
import assert from 'node:assert/strict';
import { createOnlineDetailController, resourceQrCode } from '../src/lib/online-detail.js';

test('返回期间丢弃详情响应，下一款详情不会被旧请求覆盖',async()=>{
  const pending=[];let changes=0;
  const controller=createOnlineDetailController((name,args)=>new Promise((resolve,reject)=>pending.push({name,args,resolve,reject})),()=>changes++);
  const first=controller.load({url:'https://www.gamer520.com/1.html'});
  controller.cancel();pending[0].resolve({id:'1'});await first;
  assert.equal(controller.state.data,null);assert.equal(changes,1);
  const old=controller.load({url:'https://www.gamer520.com/1.html'});
  const latest=controller.load({url:'https://www.gamer520.com/2.html'});
  pending[2].resolve({id:'2'});await latest;pending[1].reject(Error('旧请求错误'));await old;
  assert.equal(controller.state.data.id,'2');assert.equal(controller.state.error,'');
});

test('同一详情重复加载合并，刷新失败保留已读取的正文和下载信息',async()=>{
  let resolve,calls=0,fail=false;
  const controller=createOnlineDetailController(()=>{calls++;return fail?Promise.reject(Error('离线')):new Promise(done=>{resolve=done;});},()=>{});
  const first=controller.load({url:'https://www.gamer520.com/1.html'});
  await controller.load({url:'https://www.gamer520.com/1.html'});assert.equal(calls,1);
  resolve({id:'1',downloads:{links:[]}});await first;fail=true;await controller.load();
  assert.equal(controller.state.data.id,'1');assert.match(controller.state.error,/离线/);
});

test('二维码本地生成，拒绝可执行地址和包含账号凭据的地址',()=>{
  const image=resourceQrCode('https://pan.baidu.com/s/example?pwd=abcd');
  assert.match(image,/^data:image\/svg\+xml;charset=utf-8,/);
  assert.match(decodeURIComponent(image.split(',')[1]),/<svg[^>]+viewBox=/);
  for(const value of ['javascript:alert(1)','http://pan.baidu.com/s/example','https://user:password@pan.baidu.com/s/example'])assert.throws(()=>resourceQrCode(value));
});
