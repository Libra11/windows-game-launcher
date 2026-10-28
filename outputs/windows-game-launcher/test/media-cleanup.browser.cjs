const {chromium}=require(process.env.YOUJI_PLAYWRIGHT_PATH||'playwright');
const assert=require('node:assert/strict');
(async()=>{
const browser=await chromium.launch({headless:true,channel:'msedge'});
try {
 for(const initialMode of ['empty','records','blocked','stale']) {
  const page=await browser.newPage({viewport:{width:1280,height:900}});const errors=[];page.on('pageerror',error=>errors.push(String(error)));
  await page.route('**/test/cleanup',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><html><body><main id="host"></main></body></html>'}));
  await page.goto((process.env.YOUJI_TEST_URL||'http://127.0.0.1:5178')+'/test/cleanup');
  await page.evaluate(async mode=>{
   await Promise.all([import('/src/style.css'),import('/src/ui/dialogs.css'),import('/src/ui/theme.css')]);
   const {createMediaController}=await import('/src/ui/media/index.js');
   window.mode=mode;window.calls=[];window.revision=1;
   window.items=[{id:'root',kind:'series',title:'chongfu',recognized:true,externalIds:[]},{id:'season',kind:'season',parentId:'root',seasonNumber:2,title:'Season 2',recognized:true,externalIds:[]},{id:'episode',kind:'episode',parentId:'season',seasonNumber:2,episodeNumber:1,title:'wl',recognized:true,externalIds:[]},{id:'other',kind:'movie',title:'保留的电影',recognized:true,externalIds:[]}];
   window.controller=createMediaController({command:async(name,args)=>{
    const input=args?.input||{};calls.push([name,input]);
    if(name==='media_snapshot')return {items:structuredClone(items),sources:[],collections:[],tags:[],jobs:[],settings:{}};
    if(name==='media_player_status')return {active:false,state:'idle'};
    if(name==='media_presentation')return {people:[]};
    if(name==='media_detail') {const item=items.find(v=>v.id===input.itemId);if(!item)throw Error('条目已不存在');return {item,parents:[],children:items.filter(v=>v.parentId===item.id),resources:[]};}
    if(name==='media_cleanup') {
     if(input.action==='preview')return {itemId:input.itemId,allowed:window.mode!=='blocked',blockedReason:'有关联文件资源，请先解除文件关联',fingerprint:`preview-${revision}`,hasRecords:window.mode==='records',hasDoubanSource:window.mode==='records',entries:[{id:'root',title:'chongfu',kind:'series',scope:'emptyParent'},{id:'season',parentId:'root',title:'Season 2',kind:'season',scope:'selected'},{id:'episode',parentId:'season',title:'wl',kind:'episode',scope:'child'}],summary:{items:3,parents:1,personal:window.mode==='records'?1:0,watched:window.mode==='records'?1:0,favorites:window.mode==='records'?1:0,ratings:window.mode==='records'?1:0,comments:window.mode==='records'?1:0,sessions:window.mode==='records'?2:0,doubanRecords:window.mode==='records'?1:0,collectionLinks:window.mode==='records'?1:0,tagLinks:window.mode==='records'?1:0,linkedResources:window.mode==='blocked'?1:0}};
     if(input.action==='apply'){if(window.mode==='stale')throw Error('条目或相关记录已变化，请重新预览后确认');if(window.mode==='blocked')throw Error('有关联文件');if(window.mode==='records'&&!input.deleteRecords)throw Error('需要同时删除记录');items=items.filter(v=>v.id==='other');return {removed:3};}
    }
    throw Error(name);
   }});document.querySelector('#host').append(controller.element);await controller.refresh();
  },initialMode);
  await page.locator('[data-media-item-id="season"] .media-card').dispatchEvent('contextmenu',{clientX:350,clientY:250});
  assert.equal(await page.getByRole('menuitem',{name:'清理条目',exact:true}).count(),1);
  await page.getByRole('menuitem',{name:'清理条目',exact:true}).click();await page.waitForSelector('.media-cleanup-list');
  assert.equal(await page.locator('.media-cleanup-list li').count(),3);
  assert.equal(await page.evaluate(()=>calls.filter(([name,input])=>name==='media_cleanup'&&input.action==='apply').length),0,'预览不能删除条目');
  if(initialMode==='blocked'){
   assert.equal(await page.getByRole('button',{name:'确认清理',exact:true}).isDisabled(),true);assert.ok((await page.getByRole('alert').textContent()).includes('有关联文件'));
   await page.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await page.evaluate(()=>items.length),4);
  } else {
   if(initialMode==='records'){
    assert.equal(await page.getByRole('button',{name:'确认清理',exact:true}).isDisabled(),true);
    const checkbox=page.getByRole('checkbox',{name:'同时删除相关记录',exact:false});assert.equal(await checkbox.isChecked(),false);
    // 取消不删除；详情管理区也提供相同入口。
    await page.getByRole('button',{name:'取消',exact:true}).click();await page.waitForFunction(()=>!document.querySelector('dialog[open]'));
    await page.evaluate(async()=>{await controller.open({id:'root'});await controller.open({id:'season'});});
    await page.getByRole('button',{name:'清理条目',exact:true}).click();await page.waitForSelector('.media-cleanup-list');
    await page.getByRole('checkbox',{name:'同时删除相关记录',exact:false}).check();
    await page.screenshot({path:'C:/code/windows-game-launcher/work/media-cleanup-records-20261028.png'});
   }
   await page.getByRole('button',{name:'确认清理',exact:true}).click();
   if(initialMode==='stale'){
    await page.waitForFunction(()=>document.querySelector('[role="alert"]')?.textContent.includes('已变化'));
    assert.equal(await page.getByRole('button',{name:'确认清理',exact:true}).count(),0);assert.equal(await page.evaluate(()=>items.length),4);
    await page.evaluate(()=>{mode='empty';revision++;});await page.getByRole('button',{name:'重新预览',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.media-cleanup-confirm')?.disabled===false);
    await page.getByRole('button',{name:'确认清理',exact:true}).click();
   }
   await page.waitForFunction(()=>!document.querySelector('dialog[open]'));
   await page.waitForFunction(()=>controller.state.data.items.length===1);
   assert.deepEqual(await page.evaluate(()=>items.map(v=>v.id)),['other']);
   assert.equal(await page.evaluate(()=>controller.state.selectedId),'');
   if(initialMode==='records')assert.equal(await page.evaluate(()=>calls.findLast(([name,input])=>name==='media_cleanup'&&input.action==='apply')[1].deleteRecords),true);
   assert.equal(await page.locator('[data-media-item-id="season"]').count(),0);
  }
  assert.deepEqual(errors,[]);await page.evaluate(()=>controller.dispose());await page.close();
  console.log(`清理条目浏览器验证通过：${initialMode}`);
 }
}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
