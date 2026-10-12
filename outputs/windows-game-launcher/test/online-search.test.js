import test from 'node:test';
import assert from 'node:assert/strict';
import { createOnlineSearchController, gamer520SearchUrl } from '../src/lib/online-search.js';

const result = (query, page = 1) => ({query,page,items:[{id:'1',isPc:true}],hasNext:true,searchUrl:gamer520SearchUrl(query)});

test('搜索关键词编码并校验，不向网站发送空白或超长关键词',async()=>{
  assert.equal(new URL(gamer520SearchUrl(' A & 中文 ')).searchParams.get('s'),'A & 中文');
  let calls=0;
  const controller=createOnlineSearchController(async()=>{calls++;});
  for(const query of [' ', '中'.repeat(101), 'a\nb'])await controller.search(query);
  assert.equal(calls,0);assert.match(controller.state.error,/100/);
});

test('重复请求合并，较晚完成的旧请求不会覆盖新结果或结束新请求',async()=>{
  const pending=[];
  const controller=createOnlineSearchController((name,args)=>new Promise((resolve,reject)=>pending.push({name,args,resolve,reject})));
  const first=controller.search('旧游戏');await controller.search('旧游戏');
  assert.equal(pending.length,1);
  const second=controller.search('新游戏');
  pending[0].resolve(result('旧游戏'));await first;
  assert.equal(controller.state.loading,true);assert.equal(controller.state.data,null);
  pending[1].resolve(result('新游戏'));await second;
  assert.equal(controller.state.data.query,'新游戏');assert.equal(controller.state.loading,false);
});

test('翻页使用已提交关键词，失败保留结果，重试恢复失败页',async()=>{
  let fail=false;const requests=[];
  const controller=createOnlineSearchController(async(name,args)=>{
    requests.push({name,...args});if(fail)throw Error('网络断开');return result(args.query,args.page);
  });
  await controller.search('空洞骑士');controller.state.draft='未提交的文本';
  fail=true;await controller.page(2);
  assert.equal(requests.at(-1).query,'空洞骑士');assert.equal(controller.state.data.page,1);
  assert.match(controller.state.error,/网络断开/);
  fail=false;await controller.retry();
  assert.equal(controller.state.data.page,2);assert.equal(controller.state.error,'');
  assert.equal(controller.state.draft,'未提交的文本');
});

test('类型切换保留数据，不重复联网；过时错误不影响当前成功结果',async()=>{
  let rejectOld,calls=0;
  const controller=createOnlineSearchController((name,args)=>{
    calls++;return args.query==='旧' ? new Promise((resolve,reject)=>{rejectOld=reject;}) : Promise.resolve(result(args.query));
  });
  const old=controller.search('旧');await controller.search('新');
  rejectOld(Error('旧请求失败'));await old;
  controller.scope('all');controller.scope('无效');
  assert.equal(controller.state.scope,'all');assert.equal(calls,2);
  assert.equal(controller.state.data.query,'新');assert.equal(controller.state.error,'');
});
