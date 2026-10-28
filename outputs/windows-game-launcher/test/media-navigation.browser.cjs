const { chromium } = require(process.env.YOUJI_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'msedge' });
  try {
    for (const nested of [false, true]) {
      const page = await browser.newPage({ viewport: { width: 1180, height: 820 } });
      const errors = [];
      page.on('pageerror', error => errors.push(String(error)));
      await page.route('**/test/navigation', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><style>body{margin:0} ${nested ? 'body{height:100vh;overflow:hidden} #host{height:100vh;overflow:auto}' : ''}</style></head><body><main id="host"></main></body></html>` }));
      await page.goto((process.env.YOUJI_TEST_URL || 'http://127.0.0.1:5178') + '/test/navigation');
      await page.evaluate(async nested => {
        const { createMediaController } = await import('/src/ui/media/index.js');
        window.items = Array.from({length:1600}, (_, i) => ({id:`m${String(i).padStart(4,'0')}`,title:`作品${String(i).padStart(4,'0')}`,kind:'movie',recognized:true,resourceCount:1,externalIds:[]}));
        window.calls = []; window.failBrowse = false; window.holdBrowse = false;
        window.peopleDelay = false;
        window.controller = createMediaController({command: async (name, args) => {
          calls.push(name);
          const input = args?.input || {};
          if (name === 'media_snapshot') return {items,sources:[{id:'s',path:'C:/media',available:true}],collections:[],tags:[],jobs:[],settings:{},peopleIndex:{total:0,completed:0,failed:0}};
          if (name === 'media_player_status') return {state:'idle',active:false};
          if (name === 'media_people_snapshot') { if (peopleDelay) await new Promise(resolve => window.releasePeople=resolve); return {people:[]}; }
          if (name === 'media_presentation') return {people:[]};
          if (name === 'media_detail') return {item:items.find(item=>item.id===input.itemId),parents:[],children:[],resources:[]};
          if (name === 'media_browse') {
            if (holdBrowse) await new Promise(resolve=>window.releaseBrowse=resolve);
            if (failBrowse) throw Error('目录离线');
            const path = input.path || 'C:/media';
            return {path,parentPath:path==='C:/media'?null:'C:/media',entries:path==='C:/media'?[
              ...items.map(item=>({id:item.id,kind:'item',name:item.title,path:`C:/media/original-${item.id}.mkv`,item})),
              {id:'folder',kind:'folder',name:'子目录',directory:true,path:'C:/media/sub'}
            ]:[]};
          }
          throw Error(name);
        }});
        document.querySelector('#host').append(controller.element);
        await controller.refresh();
        controller.navigate('files'); await controller.browse('s');
        window.scrollSurface = nested ? document.querySelector('#host') : document.scrollingElement;
        window.setPosition = y => { scrollSurface.scrollTop=y; };
        window.position = () => scrollSurface.scrollTop;
      }, nested);
      const settle = async () => { await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))); await page.waitForTimeout(120); };
      await settle();
      await page.evaluate(()=>setPosition(16000)); await settle();
      const before = await page.evaluate(()=>{ window.savedY=position(); window.savedKey=[...document.querySelectorAll('.media-tile')].find(node=>node.getBoundingClientRect().bottom>0)?.dataset.mediaGridKey; return savedY; });
      await page.evaluate(async()=>{ const visible=[...document.querySelectorAll('[data-media-item-id]')].find(node=>node.getBoundingClientRect().top>0 && node.getBoundingClientRect().top<700); await controller.open({id:visible.dataset.mediaItemId}); });
      await page.evaluate(()=>controller.back()); await settle();
      assert.ok(Math.abs(await page.evaluate(()=>position()) - before)<5, `详情返回位置丢失 nested=${nested}: ${before} -> ${await page.evaluate(()=>position())}`);
      await page.setViewportSize({width:940,height:820});
      await page.evaluate(async()=>{const visible=[...document.querySelectorAll('[data-media-item-id]')].find(node=>node.getBoundingClientRect().top>0&&node.getBoundingClientRect().top<700);window.anchorId=visible.dataset.mediaItemId;await controller.open({id:anchorId});});
      await page.setViewportSize({width:1180,height:820});
      await page.evaluate(()=>controller.back()); await settle();
      assert.ok(await page.locator(`[data-media-item-id="${await page.evaluate(()=>anchorId)}"]`).evaluate(node=>node.getBoundingClientRect().bottom>0&&node.getBoundingClientRect().top<innerHeight),'改变窗口宽度后返回仍显示原作品');
      await page.evaluate(()=>setPosition(savedY));await settle();
      await page.evaluate(()=>controller.browse('s', 'C:/media/sub')); await settle();
      assert.equal(await page.evaluate(()=>position()),0);
      await page.getByRole('button',{name:'上一级',exact:true}).click(); await settle();
      assert.ok(Math.abs(await page.evaluate(()=>position()) - before)<5, '上一级应恢复父目录位置');
      await page.evaluate(()=>controller.browse('s',controller.state.browsePath)); await settle();
      assert.ok(Math.abs(await page.evaluate(()=>position()) - before)<5, '刷新同目录应保持滚动');
      await page.evaluate(()=>{ window.holdBrowse=true; window.pendingBrowse=controller.browse('s','C:/media/sub'); });
      await page.getByRole('button',{name:'返回',exact:true}).click();
      await page.evaluate(async()=>{holdBrowse=false;releaseBrowse();await pendingBrowse;}); await settle();
      assert.equal(await page.evaluate(()=>controller.state.browsePath),'C:/media');
      assert.ok(Math.abs(await page.evaluate(()=>position()) - before)<5, '返回后迟到目录结果不得覆盖');
      for (const surface of ['library','person']) {
        await page.evaluate(surface=>{
          if(surface==='library') controller.navigate('all');
          else {const person={id:9,name:'测试演员',credits:items.map(item=>({itemId:item.id,roles:[],departments:['cast']}))};controller.state.data.people=[person];controller.openPerson(person);}
        },surface);
        await settle();await page.evaluate(()=>setPosition(11000));await settle();
        const y=await page.evaluate(()=>position());
        await page.evaluate(async()=>{const visible=[...document.querySelectorAll('[data-media-item-id]')].find(node=>node.getBoundingClientRect().top>0&&node.getBoundingClientRect().top<700);await controller.open({id:visible.dataset.mediaItemId});});
        await page.evaluate(()=>controller.back());await settle();
        assert.ok(Math.abs(await page.evaluate(()=>position())-y)<5,`${surface} 返回应恢复位置`);
      }
      await page.evaluate(()=>{setPosition(0);controller.search('作品');});
      await page.waitForFunction(()=>!controller.state.searchPending); await settle();
      await page.evaluate(()=>setPosition(450)); await settle();
      const searchY=await page.evaluate(()=>position());
      await page.evaluate(async()=>controller.open({id:'m0005'}));
      await page.evaluate(()=>{peopleDelay=true;controller.back();}); await settle();
      assert.ok(await page.locator('[data-media-surface="search"] [data-media-item-id]').count()>0,'搜索返回应立即保留结果');
      assert.ok(Math.abs(await page.evaluate(()=>position()) - searchY)<5, '搜索返回应立即恢复位置');
      await page.evaluate(()=>{window.releasePeople?.();peopleDelay=false;}); await settle();
      assert.ok(Math.abs(await page.evaluate(()=>position()) - searchY)<5, '搜索异步刷新后不跳动');
      await page.evaluate(()=>controller.navigate('files')); await settle();
      await page.locator('.media-search-box input').fill('尚未提交');
      await page.evaluate(()=>controller.browse('s',controller.state.browsePath));
      assert.equal(await page.evaluate(()=>controller.state.search),'','刷新不应隐式提交搜索草稿');
      assert.equal(await page.locator('.media-search-box input').inputValue(),'尚未提交','刷新保留输入草稿');
      await page.locator('.media-search-box input').fill('original-m1500');
      await page.locator('.media-search-box input').press('Enter');
      assert.equal(await page.locator('.media-files [data-media-item-id="m1500"]').count(),1, '应能用原始文件名搜索已识别作品');
      await page.evaluate(()=>{controller.state.search='没有匹配';controller.render();});
      await page.getByRole('button',{name:'清除搜索',exact:true}).click();
      assert.equal(await page.evaluate(()=>controller.state.search),'');
      await page.evaluate(async()=>{failBrowse=true;await controller.browse('s','C:/media/offline');});
      assert.equal(await page.getByText('目录暂时无法读取',{exact:true}).count(),1);
      assert.equal(await page.getByText('正在打开目录',{exact:true}).count(),0);
      await page.evaluate(()=>{failBrowse=false;});
      await page.getByRole('button',{name:'重试',exact:true}).click();
      await page.waitForFunction(()=>!controller.state.browsePending);
      assert.equal(await page.getByText('目录暂时无法读取',{exact:true}).count(),0);
      assert.deepEqual(errors,[]);
      await page.evaluate(()=>controller.dispose());
      await page.close();
      console.log(`目录／详情／搜索返回、刷新、失败重试、迟到请求通过：${nested?'容器滚动':'窗口滚动'}`);
    }
  } finally { await browser.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
