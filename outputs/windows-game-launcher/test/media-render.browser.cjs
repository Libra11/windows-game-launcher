const {chromium}=require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({headless:true,channel:'msedge'});
 const page=await browser.newPage({viewport:{width:1280,height:900}});
 const errors=[]; page.on('pageerror',error=>errors.push(String(error)));
 await page.route('https://image.tmdb.org/**',route=>route.fulfill({contentType:'image/png',body:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=','base64')}));
 await page.route('**/test/render-performance',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><html><body><aside id="sidebar"></aside><main id="host"></main></body></html>'}));
 await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/render-performance');
 await page.evaluate(async()=>{
  const {createMediaController}=await import('/src/ui/media/index.js');
  const {createMediaPreview}=await import('/src/lib/media-preview.js');
  const preview=createMediaPreview(); window.calls=[];window.patches={};window.pending=[];window.jobs=[];window.peopleExtra=0;window.injectSeasonFile=false;window.failDetail=false;
  const patch=item=>Object.assign(item,window.patches[item.id]||{});
  window.controller=createMediaController({command:async(name,args)=>{
    calls.push(name);
    if(name==='media_detail' && failDetail)throw Error('暂时无法读取详情');
    if(name==='media_presentation')await new Promise(resolve=>pending.push(resolve));
    const response=await preview(name,args);
    if(name==='media_snapshot'){response.items.forEach(patch);response.jobs=jobs;response.peopleIndex.completed+=peopleExtra;}
    if(name==='media_detail'){patch(response.item);response.children.forEach(patch);}
    if(name==='media_browse' && injectSeasonFile){const item=controller.state.data.items.find(item=>item.id==='preview-season');response.entries.push({kind:'item',name:'测试季',path:'M:/test-season.mkv',item});}
    return response;
  },toast:message=>console.log('toast',message)});
  document.querySelector('#host').append(controller.element);
  controller.mountSidebar(document.querySelector('#sidebar'),{});
  await controller.refresh();
  window.savedCards=new Map([...document.querySelectorAll('.media-grid>[data-media-item-id]')].map(node=>[node.dataset.mediaItemId,node]));
  window.savedHeader=document.querySelector('.media-page-header');
  window.savedGrid=document.querySelector('.media-grid');
  window.savedSidebar=document.querySelector('[data-media-collection]');
  window.snapshotBefore=calls.filter(v=>v==='media_snapshot').length;
  pending.splice(0).forEach(resolve=>resolve());
 });
 await page.waitForTimeout(60);
 const presentation=await page.evaluate(()=>({cards:[...savedCards].every(([id,node])=>document.querySelector(`.media-grid>[data-media-item-id="${id}"]`)===node),header:document.querySelector('.media-page-header')===savedHeader,snapshots:calls.filter(v=>v==='media_snapshot').length-snapshotBefore}));
 assert.deepEqual(presentation,{cards:true,header:true,snapshots:0});
 const results={presentation};
 const tile=page.locator('.media-grid>[data-media-item-id="preview-movie-1"]');
 await tile.dispatchEvent('contextmenu',{clientX:1268,clientY:885});
 const menu=page.getByRole('menu',{name:'影视操作'});
 await menu.waitFor({state:'visible'});
 const menuBounds=await menu.boundingBox();
 assert.ok(menuBounds.x+menuBounds.width<=1273 && menuBounds.y+menuBounds.height<=893);
 assert.equal(await page.locator('dialog[open]').count(),0);
 await page.keyboard.press('End');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'清理条目');
 await page.keyboard.press('ArrowUp');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'识别／更正资料');
 await page.keyboard.press('Escape');
 await menu.waitFor({state:'detached'});
 await tile.locator('.media-card').first().focus();
 await page.keyboard.press('Shift+F10');
 await menu.waitFor({state:'visible'});
 await page.mouse.click(1,1);
 await menu.waitFor({state:'detached'});
 await tile.dispatchEvent('contextmenu',{clientX:100,clientY:120});
 await menu.waitFor({state:'visible'});
 await page.evaluate(()=>controller.setActive(false));
 await menu.waitFor({state:'detached'});
 await page.evaluate(()=>controller.setActive(true));
 results.contextMenu={position:true,keyboard:true,outside:true,deactivate:true};
 results.single=await page.evaluate(async()=>{
  patches['preview-movie-2']={posterUrl:'https://image.tmdb.org/t/p/w500/new.jpg'};
  await controller.refresh();
  const changed=[...savedCards].filter(([id,node])=>document.querySelector(`.media-grid>[data-media-item-id="${id}"]`)!==node).map(([id])=>id);
  return {changed,header:document.querySelector('.media-page-header')===savedHeader,grid:document.querySelector('.media-grid')===savedGrid};
 });
 assert.deepEqual(results.single,{changed:['preview-movie-2'],header:true,grid:true});
 results.favorite=await page.evaluate(async()=>{
  const selector='.media-grid>[data-media-item-id="preview-movie-1"]';
  const image=document.querySelector(selector+' img');
  await controller.run('media_set_record',{itemId:'preview-movie-1',patch:{favorite:true}});
  await controller.refresh();
  // 原卡片回调仍需按新快照取消收藏，图片继续复用。
  document.querySelector(selector+' .media-favorite').click();
  await new Promise(resolve=>setTimeout(resolve,0));
  return {favorite:controller.state.data.items.find(item=>item.id==='preview-movie-1').favorite,
    image:document.querySelector(selector+' img')===image,
    badges:document.querySelectorAll(selector+' .media-card-badges').length};
 });
 assert.deepEqual(results.favorite,{favorite:false,image:true,badges:1});
 results.background=await page.evaluate(async()=>{
  const before=[...document.querySelectorAll('.media-grid>[data-media-item-id]')];
  jobs=[{id:'j',kind:'metadata',state:'running',completed:4,total:100}];peopleExtra++;
  await controller.refresh();
  return {cards:before.every(node=>node.isConnected),header:document.querySelector('.media-page-header')===savedHeader,sidebar:document.querySelector('[data-media-collection]')===savedSidebar};
 });
 assert.deepEqual(results.background,{cards:true,header:true,sidebar:true});
 results.seasonPoster=await page.evaluate(async()=>{
  const selector='.media-grid>[data-media-item-id="preview-season"]';
  const parent=controller.state.data.items.find(item=>item.id==='preview-series');
  const original=parent.posterUrl;
  const before=document.querySelector(selector+' img')?.getAttribute('src');
  const other=document.querySelector('.media-grid>[data-media-item-id="preview-movie-1"]');
  patches['preview-series']={posterUrl:'https://image.tmdb.org/t/p/w500/series-updated.jpg'};
  await controller.refresh();
  const updated=document.querySelector(selector+' img')?.getAttribute('src');
  const otherRetained=other.isConnected;
  patches['preview-season']={posterUrl:'https://image.tmdb.org/t/p/w500/season-own.jpg'};
  await controller.refresh();
  const own=document.querySelector(selector+' img')?.getAttribute('src');
  delete patches['preview-series']; delete patches['preview-season'];
  await controller.refresh();
  return {original,before,updated,own,otherRetained};
 });
 assert.equal(results.seasonPoster.before,results.seasonPoster.original);
 assert.equal(results.seasonPoster.updated,'https://image.tmdb.org/t/p/w500/series-updated.jpg');
 assert.equal(results.seasonPoster.own,'https://image.tmdb.org/t/p/w500/season-own.jpg');
 assert.equal(results.seasonPoster.otherRetained,true);
 results.recognition=await page.evaluate(async()=>{
  const id='preview-unidentified';
  const labels=[];
  for(const status of ['pending','unmatched','failed']){
    patches[id]={recognitionStatus:status,recognitionError:'测试原因'};
    await controller.refresh();
    labels.push(document.querySelector(`[data-media-item-id="${id}"] .media-recognition-badge`)?.textContent);
  }
  await controller.open({id});
  const detail=document.querySelector('.media-cinema-meta .media-recognition-badge')?.textContent;
  controller.back();
  patches[id]={recognized:true,recognitionStatus:'recognized'};
  await controller.refresh();
  const recognized=document.querySelector(`[data-media-item-id="${id}"] .media-recognition-badge`) === null;
  delete patches[id];
  await controller.refresh();
  return {labels,detail,recognized};
 });
 assert.deepEqual(results.recognition,{labels:['待识别','未匹配','识别失败'],detail:'识别失败',recognized:true});
 await page.evaluate(async()=>{await controller.open({id:'preview-slow-horses'}); pending.splice(0).forEach(resolve=>resolve());});
 await page.waitForTimeout(60);
 results.detailUnrelated=await page.evaluate(async()=>{
  window.detailHero=document.querySelector('.media-cinema-hero'); window.detailImage=document.querySelector('.media-cinema-backdrop img');window.detailCredits=document.querySelector('.media-credits');
  const count=calls.filter(v=>v==='media_detail').length;
  patches['preview-movie-2'].overview='后台更新另一部电影';peopleExtra++;
  await controller.refresh();
  return {requests:calls.filter(v=>v==='media_detail').length-count,hero:document.querySelector('.media-cinema-hero')===detailHero,image:document.querySelector('.media-cinema-backdrop img')===detailImage,credits:document.querySelector('.media-credits')===detailCredits};
 });
 assert.deepEqual(results.detailUnrelated,{requests:0,hero:true,image:true,credits:true});
 results.detailRecord=await page.evaluate(async()=>{
  const id=controller.state.detailSeasonId;
  await controller.run('media_set_record',{itemId:id,patch:{watched:true}});
  const count=calls.filter(v=>v==='media_detail').length;
  await controller.refreshDetail();
  return {requests:calls.filter(v=>v==='media_detail').length-count,image:document.querySelector('.media-cinema-backdrop img')===detailImage,credits:document.querySelector('.media-credits')===detailCredits,watched:document.querySelector('.media-personal-record').textContent.includes('已观看')};
 });
 assert.deepEqual(results.detailRecord,{requests:1,image:true,credits:true,watched:true});
 results.episode=await page.evaluate(()=>{
  const before=[...document.querySelectorAll('.media-episode-card')];
  document.querySelector('.media-episode-card:not(.active) .media-episode-select').click();
  return {image:document.querySelector('.media-cinema-backdrop img')===detailImage,credits:document.querySelector('.media-credits')===detailCredits,retained:before.filter(node=>node.isConnected).length,total:before.length};
 });
 assert.equal(results.episode.image,true);assert.equal(results.episode.credits,true);assert.equal(results.episode.retained,results.episode.total-2);
 results.detailReturn=await page.evaluate(async()=>{
  controller.back();
  const loading=controller.open({id:'preview-slow-horses'});
  const immediate=document.querySelector('.media-cinema-backdrop img')===detailImage;
  await loading;
  return {immediate,image:document.querySelector('.media-cinema-backdrop img')===detailImage};
 });
 assert.deepEqual(results.detailReturn,{immediate:true,image:true});
 results.detailRefreshFailure=await page.evaluate(async()=>{
  controller.back();failDetail=true;
  await controller.open({id:'preview-slow-horses'});
  failDetail=false;
  return {stillOpen:controller.state.selectedId==='preview-slow-horses',image:document.querySelector('.media-cinema-backdrop img')===detailImage};
 });
 assert.deepEqual(results.detailRefreshFailure,{stillOpen:true,image:true});
 results.files=await page.evaluate(async()=>{
  const source=controller.state.data.sources[0];
  await controller.browse(source.id,'');
  const root=document.querySelector('.media-files');
  const cards=[...document.querySelectorAll('.media-files [data-media-item-id]')];
  const refresh=controller.browse(source.id,controller.state.browsePath);
  const loading=document.querySelector('.media-files')?.textContent.includes('正在打开目录');
  await refresh;
  return {loading,root:document.querySelector('.media-files')===root,cards:cards.every(node=>node.isConnected)};
 });
 assert.deepEqual(results.files,{loading:false,root:true,cards:true});
 await page.evaluate(()=>controller.search('流人'));
 await page.waitForFunction(()=>controller.state.page==='search' && !controller.state.searchPending);
 results.search=await page.evaluate(async()=>{
  const root=document.querySelector('[data-media-surface="search"]');
  const cards=[...root.querySelectorAll('[data-media-item-id]')];
  peopleExtra++;await controller.refresh();
  return {count:cards.length,retained:cards.every(node=>node.isConnected)};
 });
 assert.ok(results.search.count>0);assert.equal(results.search.retained,true);
 await page.evaluate(()=>controller.search('法医秦明'));
 await page.waitForFunction(()=>controller.state.page==='search' && !controller.state.searchPending);
 results.searchPoster=await page.evaluate(async()=>{
  const selector='[data-media-surface="search"] [data-media-item-id="preview-season"] img';
  const before=document.querySelector(selector)?.getAttribute('src');
  patches['preview-series']={posterUrl:'https://image.tmdb.org/t/p/w500/search-parent.jpg'};
  await controller.refresh();
  while(controller.state.searchPending) await new Promise(resolve=>setTimeout(resolve,10));
  const after=document.querySelector(selector)?.getAttribute('src');
  delete patches['preview-series'];
  return {before,after};
 });
 assert.ok(results.searchPoster.before);
 assert.equal(results.searchPoster.after,'https://image.tmdb.org/t/p/w500/search-parent.jpg');
 results.personPoster=await page.evaluate(async()=>{
  patches['preview-slow-season-1']={posterUrl:''};
  await controller.refresh();
  const season=controller.state.data.items.find(item=>item.id==='preview-slow-season-1');
  controller.openPerson({id:999999,name:'测试演员',department:'cast'},season);
  const selector='[data-media-surface="person"] [data-media-item-id="preview-slow-season-1"] img';
  const before=document.querySelector(selector)?.getAttribute('src');
  patches['preview-slow-horses']={posterUrl:'https://image.tmdb.org/t/p/w500/person-parent.jpg'};
  await controller.refresh();
  const after=document.querySelector(selector)?.getAttribute('src');
  return {before,after};
 });
 assert.ok(results.personPoster.before);
 assert.equal(results.personPoster.after,'https://image.tmdb.org/t/p/w500/person-parent.jpg');
 results.filesPoster=await page.evaluate(async()=>{
  injectSeasonFile=true;
  await controller.browse(controller.state.data.sources[0].id,'');
  const selector='.media-files [data-media-item-id="preview-season"] img';
  const before=document.querySelector(selector)?.getAttribute('src');
  patches['preview-series']={posterUrl:'https://image.tmdb.org/t/p/w500/files-parent.jpg'};
  await controller.refresh();
  const after=document.querySelector(selector)?.getAttribute('src');
  return {before,after};
 });
 assert.ok(results.filesPoster.before);
 assert.equal(results.filesPoster.after,'https://image.tmdb.org/t/p/w500/files-parent.jpg');
 await page.evaluate(async()=>{await controller.open({id:'preview-season'});pending.splice(0).forEach(resolve=>resolve());});
 await page.getByRole('button',{name:'重新识别',exact:true}).click();
 assert.equal(await page.locator('dialog.media-identify-modal input[type="search"]').inputValue(),'法医秦明');
 assert.ok((await page.locator('dialog.media-identify-modal .modal-header').innerText()).includes('法医秦明'));
 await page.getByRole('dialog').getByRole('button',{name:'关闭',exact:true}).click();
 await page.locator('dialog.media-identify-modal').waitFor({state:'detached'});
 await page.evaluate(async()=>{await controller.open({id:'preview-unidentified'});pending.splice(0).forEach(resolve=>resolve());});
 await page.getByRole('button',{name:'识别',exact:true}).click();
 await page.locator('dialog.media-identify-modal input[type="search"]').fill('法医秦明');
 await page.locator('dialog.media-identify-modal').getByRole('button',{name:'搜索',exact:true}).click();
 await page.locator('dialog.media-identify-modal .media-identify-result').first().waitFor();
 results.linkPoster=await page.locator('dialog.media-identify-modal .media-identify-result').evaluateAll(nodes=>nodes.slice(0,2).map(node=>node.querySelector('img')?.getAttribute('src')));
 assert.deepEqual(results.linkPoster,['https://image.tmdb.org/t/p/w500/files-parent.jpg','https://image.tmdb.org/t/p/w500/files-parent.jpg']);
 await page.evaluate(()=>controller.dispose());
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify(results,null,2));await browser.close();
})().catch(error=>{console.error(error);process.exit(1)});
