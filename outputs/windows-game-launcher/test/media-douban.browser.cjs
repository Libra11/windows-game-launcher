const {chromium}=require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const {readFileSync}=require('node:fs');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({headless:true,channel:'msedge'});
 const page=await browser.newPage({viewport:{width:1280,height:900}});
 const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 await page.route('https://movie.douban.com/subject/42/',route=>route.fulfill({contentType:'text/html',body:`
  <meta charset="utf-8"><h1><span property="v:itemreviewed">测试作品</span><span class="year">(2024)</span></h1>
  <div id="info">首播: 2024-01-01<br>IMDb: tt1234567<br><span property="v:genre">剧情</span></div>
  <div id="mainpic"><img src="https://img1.doubanio.com/test.jpg"></div>
  <div id="link-report-intra"><span property="v:summary">截断简介…</span><span class="all"><span property="v:summary">测试简介</span></span></div>`}));
 await page.route('https://img1.doubanio.com/**',route=>route.abort());
 await page.goto('https://movie.douban.com/subject/42/');
 const extracted=await page.evaluate(readFileSync('src-tauri/src/media/douban_extract.js','utf8'));
 assert.deepEqual(extracted.detail,{subjectId:'42',imdbId:'tt1234567',title:'测试作品',seasonNumber:null,
  kind:'series',year:'2024',overview:'测试简介',posterUrl:'https://img1.doubanio.com/test.jpg',genres:['剧情']});
 await page.route('**/test/douban-metadata',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><main id="host"></main>'}));
 await page.route('https://cached.test/**',route=>route.fulfill({contentType:'image/png',body:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=','base64')}));
 await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178')+'/test/douban-metadata');
 await page.evaluate(async()=>{
  const {createMediaController}=await import('/src/ui/media/index.js');
  const {createMediaPreview}=await import('/src/lib/media-preview.js');
  const preview=createMediaPreview();window.doubanCalls=[];window.applied=false;window.failPreview=false;
  window.isTauri=true;window.artworkCalls=[];
  window.__TAURI_INTERNALS__={
   invoke:async(command,args)=>{artworkCalls.push({command,input:args.input});return {path:'preview.png'};},
   convertFileSrc:path=>'https://cached.test/'+path,
  };
  const base={id:'douban-item',title:'未识别目录文件',kind:'other',recognized:false,externalIds:[],parentId:'',
   rating:4,comment:'保留个人短评',positionSeconds:321,durationSeconds:1200,resourceCount:2,playable:true};
  const patch=()=>applied?{...base,title:'测试作品',kind:'season',recognized:true,seasonNumber:null,overview:'豆瓣完整简介',imdbId:'tt1234567',externalIds:[{provider:'douban',providerType:'subject',externalId:'42'}]}:{...base};
  window.controller=createMediaController({command:async(name,args)=>{
   if(name==='media_douban_metadata') {
    doubanCalls.push(args.input);
    if(args.input.action==='preview'){
     if(failPreview)throw Error('豆瓣需要安全验证');
     return {token:'fixture-token',detail:{subjectId:'42',title:'测试作品',kind:'season',year:'2024',seasonNumber:null,overview:'豆瓣完整简介',posterUrl:'https://img1.doubanio.com/test.jpg',genres:['剧情']}};
    }
    window.appliedToken=args.input.token;applied=true;return patch();
   }
   if(name==='media_detail'&&args.input.itemId==='douban-item')return {item:patch(),children:[],parents:[],resources:[
    {id:'a',name:'E01.mkv',path:'M:/E01.mkv',itemIds:['douban-item'],available:true,positionSeconds:321,durationSeconds:1200},
    {id:'b',name:'E02.mkv',path:'M:/E02.mkv',itemIds:['douban-item'],available:true,positionSeconds:90,durationSeconds:1300}]};
   const response=await preview(name,args);
   if(name==='media_snapshot')response.items.push(patch());
   return response;
  }});
  document.querySelector('#host').append(controller.element);
  await controller.refresh();await controller.open({id:'douban-item'});
 });
 await page.getByRole('button',{name:'识别',exact:true}).click();
 await page.getByText('TMDB 没有这部作品？',{exact:true}).click();
 await page.getByRole('button',{name:'从豆瓣获取资料',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'从豆瓣获取作品资料'});
 await dialog.waitFor();
 const confirm=dialog.getByRole('button',{name:'确认使用豆瓣资料'});
 assert.equal(await confirm.isDisabled(),true);
 await dialog.getByRole('textbox',{name:'豆瓣作品链接或编号'}).fill('42');
 await page.evaluate(()=>{failPreview=true;});
 await dialog.getByRole('button',{name:'获取预览'}).click();
 await dialog.getByText('Error: 豆瓣需要安全验证',{exact:true}).waitFor();
 assert.equal(await confirm.isDisabled(),true);
 await page.evaluate(()=>{failPreview=false;});
 await dialog.getByRole('button',{name:'获取预览'}).click();
 await dialog.getByRole('heading',{name:'测试作品'}).waitFor();
 await page.waitForFunction(()=>document.querySelector('.media-douban-preview .media-art.loaded'));
 assert.equal(await dialog.locator('.media-art img').getAttribute('src'),'https://cached.test/preview.png');
 assert.deepEqual(await page.evaluate(()=>JSON.parse(JSON.stringify(artworkCalls[0]))),{command:'media_artwork',input:{itemId:'douban-item',role:'poster',previewToken:'fixture-token',cacheOnly:false}});
 assert.equal(await confirm.isEnabled(),true);
 assert.equal(await page.evaluate(()=>applied),false);
 await dialog.getByRole('textbox',{name:'豆瓣作品链接或编号'}).fill('43');
 assert.equal(await confirm.isDisabled(),true);
 await dialog.getByRole('textbox',{name:'豆瓣作品链接或编号'}).fill('42');
 await dialog.getByRole('button',{name:'获取预览'}).click();
 await confirm.click();await dialog.waitFor({state:'detached'});
 await page.getByRole('button',{name:'更新豆瓣资料',exact:true}).waitFor();
 assert.equal(await page.locator('.media-episode-section').count(),0);
 assert.equal(await page.getByText('我的观影 · 测试作品',{exact:true}).count(),1);
 assert.equal(await page.getByText('保留个人短评',{exact:true}).count(),1);
 await page.getByText('作品链接',{exact:true}).click();
 assert.equal(await page.getByRole('link',{name:'在豆瓣查看 ↗'}).getAttribute('href'),'https://movie.douban.com/subject/42/');
 assert.equal(await page.getByRole('link',{name:'在 IMDb 查看 ↗'}).getAttribute('href'),'https://www.imdb.com/title/tt1234567/');
 assert.equal(await page.getByText('视频文件',{exact:true}).count(),1);
 await page.getByRole('button',{name:'更新豆瓣资料',exact:true}).click();
 await dialog.waitFor();
 assert.equal(await dialog.getByRole('textbox',{name:'豆瓣作品链接或编号'}).inputValue(),'42');
 await dialog.getByRole('button',{name:'取消',exact:true}).click();
 await dialog.waitFor({state:'detached'});
 await page.getByRole('button',{name:'解除识别',exact:true}).click();
 await page.getByRole('dialog',{name:'解除资料识别'}).waitFor();
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({extraction:true,previewFailure:true,explicitConfirm:true,seasonWithoutEpisodes:true,personalRecords:true,sourceLinks:true,refresh:true,unlinkEntry:true}));
 await page.evaluate(()=>controller.dispose());await browser.close();
})().catch(error=>{console.error(error);process.exit(1);});
