import test from 'node:test';
import assert from 'node:assert/strict';
import { parseUpdateNotes,downloadPercent,updateAvailable,updateBusy } from '../src/lib/app-update.js';
import { createAppUpdateController } from '../src/lib/app-update-controller.js';

test('说明只生成受控文本块，不把 HTML 或图片转换成可执行内容',()=>{
  const blocks=parseUpdateNotes('# 新版本\n\n- 优化封面\n- 修复计时\n\n<script>alert(1)</script>\n![远程图片](https://example.com/a.png)\n\n```js\n<script>ignored</script>\n```');
  assert.equal(blocks[0].type,'heading');assert.deepEqual(blocks[1].items,['优化封面','修复计时']);
  assert.equal(blocks[2].type,'paragraph');assert.match(blocks[2].text,/<script>/);assert.equal(blocks[3].type,'code');
});
test('大小未知时使用不定进度，进度和更新提醒状态有界',()=>{
  assert.equal(downloadPercent({downloaded:10,total:null}),null);assert.equal(downloadPercent({downloaded:150,total:100}),100);
  assert.equal(updateAvailable({preparationId:'',version:'1.1.0',currentVersion:'1.0.0'}),false);
  assert.equal(updateAvailable({preparationId:'ready',version:'1.1.0',currentVersion:'1.0.0'}),true);
  assert.equal(updateBusy({phase:'verifying'}),true);assert.equal(updateBusy({phase:'ready'}),false);
});
test('离开设置不停止后台事件，重新订阅能获得已下载状态，迟到事件不回退状态',async()=>{
  let event,stopped=false;const controller=createAppUpdateController(async()=>({revision:1,phase:'idle'}),async handler=>{event=handler;return ()=>{stopped=true;};});
  await controller.start();let paints=0;const unsubscribe=controller.subscribe(()=>paints++);assert.equal(paints,1);unsubscribe();
  event({payload:{revision:3,phase:'ready',downloadReady:true}});assert.equal(paints,1);
  const reopened=[];controller.subscribe(status=>reopened.push(status.phase));assert.deepEqual(reopened,['ready']);
  event({payload:{revision:2,phase:'downloading'}});assert.equal(controller.status.phase,'ready');assert.equal(stopped,false);
});
test('初始化读取不能覆盖先到的实时状态，取消使用当前操作 ID',async()=>{
  let resolve,event,args;const controller=createAppUpdateController((name,input)=>{if(name==='get_app_update_status')return new Promise(done=>resolve=done);args=[name,input];return Promise.resolve(true);},async handler=>{event=handler;return ()=>{};});
  const start=controller.start();await Promise.resolve();event({payload:{revision:4,phase:'downloading',operationId:'download-one'}});
  resolve({revision:1,phase:'idle'});await start;assert.equal(controller.status.phase,'downloading');
  await controller.cancel(controller.status.operationId);assert.deepEqual(args,['cancel_app_update_download',{operationId:'download-one'}]);
});

test('发布验签绑定安装包和可信版本注释，拒绝篡改与版本错配',async()=>{
  const {readFileSync}=await import('node:fs');const {verifyUpdateSignature}=await import('../../../.github/scripts/verify-update-signature.mjs');
  const fixture=JSON.parse(readFileSync(new URL('../src-tauri/src/app_update/fixtures/signed.json',import.meta.url),'utf8'));
  const bytes=Buffer.from(fixture.payload,'base64');verifyUpdateSignature(bytes,fixture.signature,fixture.publicKey,'1.1.0');
  assert.throws(()=>verifyUpdateSignature(Buffer.concat([bytes,Buffer.from('tampered')]),fixture.signature,fixture.publicKey,'1.1.0'));
  assert.throws(()=>verifyUpdateSignature(bytes,fixture.signature,fixture.publicKey,'1.2.0'));
  const decoded=Buffer.from(fixture.signature,'base64').toString().replace('version:1.1.0','version:1.2.0');
  assert.throws(()=>verifyUpdateSignature(bytes,Buffer.from(decoded).toString('base64'),fixture.publicKey,'1.2.0'));
});
