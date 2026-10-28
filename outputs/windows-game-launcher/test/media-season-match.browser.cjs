const {chromium}=require(process.env.YOUJI_PLAYWRIGHT_PATH||'playwright');
const assert=require('node:assert/strict');
(async()=>{
const browser=await chromium.launch({headless:true,channel:'msedge'});
try{
 const page=await browser.newPage({viewport:{width:1280,height:900}});const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 await page.route('**/test/season-match',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><html><body><main id="host"></main></body></html>'}));
 await page.goto((process.env.YOUJI_TEST_URL||'http://127.0.0.1:5178')+'/test/season-match');
 await page.evaluate(async()=>{
  await Promise.all([import('/src/style.css'),import('/src/ui/dialogs.css'),import('/src/ui/theme.css')]);
  const {createMediaController}=await import('/src/ui/media/index.js');
  const series={id:'series',kind:'series',title:'测试剧',recognized:true,externalIds:[]};
  const season={id:'season',parentId:'series',kind:'season',title:'第一季',seasonNumber:1,recognized:true,externalIds:[]};
  window.calls=[];window.failApply=false;window.holdPreview=false;window.holdReview=false;window.saves=0;window.discards=[];
  window.controller=createMediaController({command:async(name,args)=>{
   const input=args?.input||{};calls.push([name,input]);
   if(name==='media_snapshot')return {items:[series,season],sources:[{id:'s',path:'C:/media',available:true}],collections:[],tags:[],jobs:[],settings:{}};
   if(name==='media_player_status')return {active:false,state:'idle'};
   if(name==='media_browse')return {path:input.path||'C:/media',entries:[{id:'folder',name:'普通目录',kind:'folder',path:'C:/media/folder',directory:true},{id:'smart',name:'智能剧集目录',kind:'item',path:'C:/media/smart',directory:true,item:season}],parentPath:null};
   if(name==='media_search_metadata')return [{providerType:'tv',externalId:'20',seriesTitle:'测试剧',seasonTitle:'第一季',seasonNumber:1,kind:'season',title:'测试剧 第一季'}];
   if(name==='media_match_season'){
    if(input.action==='inspect')return {count:3};
    if(input.action==='discard'){discards.push(input.token);return {};}
    if(input.action==='preview'){
     if(holdPreview)await new Promise(resolve=>window.releasePreview=resolve);
     return {token:'token',title:'测试剧',seasonTitle:'第一季',seasonNumber:1,episodes:[{episodeNumber:1,title:'开始'},{episodeNumber:2,title:'继续'}],files:[
      {resourceId:'r1',name:'EP01.mkv',path:'C:/media/EP01.mkv',current:[{title:'错误作品',parentTitle:'旧第一季'}],parsedEpisodes:[1],suggestedEpisodes:[1],canSuggest:true,warning:'',disabled:false},
      {resourceId:'r2',name:'EP13.mkv',path:'C:/media/EP13.mkv',current:[],parsedEpisodes:[13],suggestedEpisodes:[],canSuggest:false,canOverrideSeason:true,warning:'文件季号与所选季不同，请手动确认集号',disabled:false},
      {resourceId:'r3',name:'Offline.mkv',path:'C:/media/Offline.mkv',current:[],parsedEpisodes:[],suggestedEpisodes:[],warning:'文件离线',disabled:true}
     ]};
    }
    if(input.action==='review'){
     if(holdReview)await new Promise(resolve=>window.releaseReview=resolve);
     return {reviewId:'review',effects:[{itemId:'old-season',title:'旧第一季',targetKey:input.recordTargets?.['old-season']||'season:20:1',reason:'记录迁移后清理旧条目',options:[{key:'season:20:1',title:'测试剧 第一季'}]}],conflicts:input.recordTargets?.['old-season']==='keep'?[]:[{key:'season:20:1/rating',title:'测试剧 第一季',label:'评分',choices:[{id:'old-season',title:'原条目',value:4},{id:'target',title:'目标',value:5}]}]};
    }
    if(input.action==='apply'){if(failApply)throw Error('预览已过期');saves++;return {matched:2,itemId:'season'};}
   }
   throw Error(name);
  }});document.querySelector('#host').append(controller.element);await controller.refresh();await controller.browse('s');
 });
 const open=async()=>{await page.getByRole('button',{name:'匹配／更正本季剧集',exact:true}).click();await page.getByRole('searchbox',{name:'电视剧名称'}).fill('测试剧');await page.getByRole('button',{name:'搜索电视剧',exact:true}).click();await page.getByRole('button',{name:'测试剧 · 第一季',exact:true}).click();await page.waitForSelector('.media-season-table');};
 // 媒体源根目录也提供键盘菜单入口，不提供后端禁止的忽略根目录操作。
 await page.evaluate(()=>controller.browse(''));
 await page.locator('[data-focus-key="media-source-s"]').focus();await page.keyboard.press('Shift+F10');
 assert.equal(await page.getByRole('menuitem',{name:'匹配／更正本季剧集',exact:true}).count(),1);
 assert.equal(await page.getByRole('menuitem',{name:'忽略此路径',exact:true}).count(),0);
 await page.getByRole('menuitem',{name:'打开目录',exact:true}).click();
 // 工具栏入口，以及普通／智能目录的右键入口。
 await page.locator('.media-directory-tile .media-card').dispatchEvent('contextmenu',{clientX:300,clientY:300});
 assert.equal(await page.getByRole('menuitem',{name:'匹配／更正本季剧集',exact:true}).count(),1);await page.keyboard.press('Escape');
 await page.locator('[data-media-item-id="season"] .media-card').dispatchEvent('contextmenu',{clientX:600,clientY:300});
 assert.equal(await page.getByRole('menuitem',{name:'打开目录',exact:true}).count(),1);assert.equal(await page.getByRole('menuitem',{name:'匹配／更正本季剧集',exact:true}).count(),1);await page.keyboard.press('Escape');
 await open();assert.equal(await page.locator('.media-season-table tbody tr').count(),3);assert.equal(await page.getByLabel('Offline.mkv 的目标集号',{exact:true}).isDisabled(),true);
 assert.equal(await page.evaluate(()=>saves),0,'预览阶段不得保存');await page.screenshot({path:'C:/code/windows-game-launcher/work/media-season-match-preview-20261028.png'});
 await page.getByRole('spinbutton',{name:'集号偏移'}).fill('-12');await page.getByRole('button',{name:'重新建议集号'}).click();
 assert.equal(await page.getByLabel('EP13.mkv 的目标集号',{exact:true}).inputValue(),'');
 await page.getByLabel('已确认所选季，允许按文件集号更正季号差异',{exact:true}).check();await page.getByRole('button',{name:'重新建议集号'}).click();
 assert.equal(await page.getByLabel('EP13.mkv 的目标集号',{exact:true}).inputValue(),'1');
 await page.getByLabel('EP01.mkv 的目标集号',{exact:true}).fill('1-2');await page.getByLabel('EP13.mkv 的目标集号',{exact:true}).fill('2');
 await page.getByRole('button',{name:'预览更正影响'}).click();await page.waitForSelector('.media-season-effect');await page.screenshot({path:'C:/code/windows-game-launcher/work/media-season-match-records-20261028.png'});
 await page.getByRole('button',{name:'确认更正',exact:true}).click();assert.equal(await page.evaluate(()=>saves),0);assert.ok(await page.getByRole('alert').textContent().then(s=>s.includes('请先处理所有记录冲突')));
 await page.getByRole('combobox',{name:'测试剧 第一季 的评分冲突'}).selectOption('old-season');
 await page.getByRole('button',{name:'确认更正',exact:true}).click();await page.waitForFunction(()=>!document.querySelector('dialog[open]'));
 assert.equal(await page.evaluate(()=>saves),1);const saved=await page.evaluate(()=>calls.findLast(([name,input])=>name==='media_match_season'&&input.action==='apply')[1]);assert.equal(saved.reviewId,'review');assert.equal(saved.resolutions['season:20:1/rating'],'old-season');
 // 编辑集号使已生成的最终确认失效；迟到的旧预览也不能恢复确认按钮。
 await open();await page.evaluate(()=>{holdReview=true;});await page.getByRole('button',{name:'预览更正影响'}).click({noWaitAfter:true});
 await page.getByLabel('EP01.mkv 的目标集号',{exact:true}).fill('2');await page.evaluate(()=>{holdReview=false;releaseReview();});
 await page.waitForTimeout(50);assert.equal(await page.getByRole('button',{name:'确认更正',exact:true}).count(),0);
 await page.getByRole('button',{name:'预览更正影响'}).click();await page.waitForSelector('.media-season-effect');
 await page.getByRole('combobox',{name:'旧第一季 的记录归属'}).selectOption('keep');await page.waitForSelector('.media-season-effect');assert.equal(await page.locator('.media-season-conflict').count(),0);
 await page.evaluate(()=>{failApply=true;});await page.getByRole('button',{name:'确认更正',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('[role="alert"]')?.textContent.includes('预览已过期'));assert.equal(await page.evaluate(()=>saves),1);assert.equal(await page.locator('.media-season-table').count(),0);
 await page.getByRole('button',{name:'关闭',exact:true}).click();await page.waitForFunction(()=>!document.querySelector('dialog[open]'));
 // 取消窗口后迟到的资料会被丢弃。
 await page.evaluate(()=>{failApply=false;holdPreview=true;});
 await page.getByRole('button',{name:'匹配／更正本季剧集',exact:true}).click();await page.getByRole('searchbox',{name:'电视剧名称'}).fill('测试剧');await page.getByRole('button',{name:'搜索电视剧',exact:true}).click();
 await page.getByRole('button',{name:'测试剧 · 第一季',exact:true}).click({noWaitAfter:true});await page.getByRole('button',{name:'关闭',exact:true}).click();
 await page.waitForFunction(()=>!document.querySelector('dialog[open]'));await page.evaluate(()=>{holdPreview=false;releasePreview();});await page.waitForTimeout(50);
 assert.ok(await page.evaluate(()=>discards.length>0));assert.deepEqual(errors,[]);
 await page.screenshot({path:'C:/code/windows-game-launcher/work/media-season-match-browser-20261028.png'});
 await page.evaluate(()=>controller.dispose());console.log('单季匹配：目录入口、预览不写入、偏移、多集、多版本、记录冲突、保留选项、失效与取消通过');
}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
