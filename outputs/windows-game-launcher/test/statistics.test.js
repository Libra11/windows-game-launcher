import test from 'node:test';
import assert from 'node:assert/strict';
import { createStatisticsController, monthDays, shiftMonth, rankedGames, duration } from '../src/lib/statistics.js';
import { createStatisticsPreview } from '../src/lib/statistics-preview.js';
const data=(source='all')=>({from:'2026-10-01',to:'2026-10-02',generatedAt:'now',summary:{source}});
const turn=()=>new Promise(resolve=>setImmediate(resolve));

test('日历遵循周一开周，闰年与跨年翻月正确',()=>{
  assert.equal(monthDays('2024-02').dates.length,29);
  assert.equal(monthDays('2026-10').offset,3);
  assert.equal(shiftMonth('2026-01',-1),'2025-12');
  assert.equal(shiftMonth('2026-12',1),'2027-01');
});
test('时长口径互不相加，缺失数据不进入排行',()=>{
  const games=[{title:'A',localSeconds:30,steamSeconds:100,periodSeconds:10},{title:'B',localSeconds:60,steamSeconds:null,periodSeconds:20}];
  assert.equal(rankedGames(games,'local')[0].title,'B');
  assert.deepEqual(rankedGames(games,'steam').map(g=>g.title),['A']);
  assert.equal(duration(null),'未获取');
  assert.equal(duration(0),'0 分钟');
});
test('隐藏页面不请求，五秒内事件去重；失败保留上次统计',async()=>{
  let visible=false,clock=0,calls=0,fail=false;
  const controller=createStatisticsController({visible:()=>visible,now:()=>clock,render:()=>{},
    command:async name=>{calls++;if(fail)throw Error('离线');return name==='get_statistics'?data():{items:[],total:0};}});
  await controller.refresh(); assert.equal(calls,0);
  visible=true;await controller.refresh();assert.equal(calls,2);
  await controller.refresh();assert.equal(calls,2);
  clock=5000;fail=true;await controller.refresh();assert.equal(calls,4);
  assert.equal(controller.state.data.summary.source,'all');assert.match(controller.state.error,/离线/);
});
test('筛选竞态丢弃旧结果，并请求最新平台；分页每页二十条',async()=>{
  let resolveOld,requests=[];
  const old=new Promise(resolve=>{resolveOld=resolve;});
  const controller=createStatisticsController({visible:()=>true,render:()=>{},command:(name,args)=>{
    requests.push({name,...args});
    if(name==='get_statistics')return args.query.source==='all'?old:Promise.resolve(data(args.query.source));
    return Promise.resolve({items:[],total:44});
  }});
  const first=controller.refresh();
  controller.filter('source','epic');
  resolveOld(data());await first;await turn();
  assert.equal(controller.state.data.summary.source,'epic');
  await controller.page(20);await turn();
  assert.equal(requests.at(-1).offset,20);assert.equal(requests.at(-1).limit,20);
  assert.equal(controller.state.query.source,'epic');
  await controller.page(80);assert.equal(controller.state.offset,40);
});
test('切到详情期间完成的请求，在返回统计时刷新页面而不重复请求',async()=>{
  let visible=true,resolveData,renders=0,calls=0;
  const payload=new Promise(resolve=>{resolveData=resolve;});
  const controller=createStatisticsController({visible:()=>visible,render:()=>renders++,command:name=>{
    calls++;return name==='get_statistics'?payload:Promise.resolve({items:[],total:0});
  }});
  const request=controller.refresh();visible=false;resolveData(data());await request;
  const before=renders;visible=true;await controller.refresh();
  assert.equal(renders,before+1);assert.equal(calls,2);
});
test('示例平台筛选不改变同一游戏的累计和完成度',()=>{
  const games=['local','steam','steam','epic'].map((source,i)=>({id:String(i),title:String(i),source,metadataJson:'{}',playedSeconds:0,playtime:{seconds:3600}}));
  const preview=createStatisticsPreview(games);
  const all=preview('get_statistics',{query:{source:'all',period:'30'}}).games;
  const steam=preview('get_statistics',{query:{source:'steam',period:'30'}}).games;
  for(const item of steam)assert.deepEqual(item,all.find(game=>game.gameId===item.gameId));
});
