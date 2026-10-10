import test from 'node:test';
import assert from 'node:assert/strict';
import { indexOrganization, organizationOptions, reconcileOrganization, clearOrganizationSelection } from '../src/lib/library-organization.js';
import { queryGames } from '../src/lib/library-query.js';
import { createLibraryViewCache } from '../src/lib/library-view-cache.js';
const games=[
  {id:'a',title:'Alpha',source:'steam',installation:{state:'installed'}},
  {id:'b',title:'Beta',source:'epic',installation:{state:'unknown'}},
  {id:'c',title:'Charlie',source:'steam',installation:{state:'installed'}},
];
const organization=()=>indexOrganization({tags:[{id:'t',name:'探索'},{id:'u',name:'联机'}],collections:[{id:'f',name:'周末',position:0}],gameTags:[['a','t'],['a','u'],['b','t'],['c','u']],gameCollections:[['a','f'],['b','f']]});
const ids=options=>queryGames(games,{organization:organization(),...options}).map(game=>game.id);
test('标签交集与并集可组合收藏夹、平台、名称与安装状态',()=>{
  assert.deepEqual(ids({tagIds:['t','u']}),['a']);
  assert.deepEqual(ids({tagIds:['t','u'],tagMatch:'any'}),['a','b','c']);
  assert.deepEqual(ids({collectionId:'f',category:'steam',tagIds:['t'],installedOnly:true,search:'alpha'}),['a']);
  assert.deepEqual(ids({collectionId:'f',tagIds:['u'],search:'beta'}),[]);
  assert.deepEqual(ids({tagIds:[]}),['a','b','c']);
  assert.deepEqual(ids({collectionId:'missing'}),[]);
  const state={organization:organization(),collectionId:'f',tagIds:['t'],tagMatch:'all'};
  const common=organizationOptions(state);
  assert.deepEqual(queryGames(games,{category:'steam',...common}),queryGames(games,{category:'steam',collection:'all',...common}));
  assert.equal(state.organization.byCollection.get('f').size,2);
});
test('删除分类清理条件，后台刷新仅剔除已不存在的选中游戏',()=>{
  const state={games,organization:organization(),collectionId:'f',tagIds:['t','deleted'],filter:'all',organizationSelection:new Set(['a','b','gone'])};
  reconcileOrganization(state);assert.deepEqual(state.tagIds,['t']);assert.equal(state.organizationSelection.size,0);
  state.organizationSelection=new Set(['a','b','gone']);reconcileOrganization(state);assert.deepEqual([...state.organizationSelection],['a','b']);
  state.organization=indexOrganization({tags:[],collections:[],gameTags:[],gameCollections:[]});reconcileOrganization(state);
  assert.equal(state.collectionId,'');assert.equal(state.filter,'all');assert.deepEqual(state.tagIds,[]);assert.equal(state.organizationSelection.size,0);
  state.organizationSelection.add('a');clearOrganizationSelection(state);assert.equal(state.organizationSelection.size,0);
});
test('分类变化使缓存失效，勾选与计时更新保留整个页面',()=>{
  const state={games:structuredClone(games),organization:organization(),organizationRevision:1,collectionId:'',tagIds:[],tagMatch:'all',filter:'all',search:'',sort:'az',installedOnly:false,view:'grid',organizationBatchMode:true,organizationSelection:new Set()};
  const cache=createLibraryViewCache(),build=()=>({});const first=cache('desktop',state,build);
  state.organizationSelection.add('a');assert.equal(cache('desktop',state,build),first);
  state.games[0].runtime={state:'running',elapsedSeconds:15};assert.equal(cache('desktop',state,build),first);
  state.tagIds=['t'];const tagged=cache('desktop',state,build);assert.notEqual(tagged,first);
  state.tagMatch='any';const any=cache('desktop',state,build);assert.notEqual(any,tagged);
  state.organizationRevision++;const revision=cache('desktop',state,build);assert.notEqual(revision,any);
  state.collectionId='f';assert.notEqual(cache('desktop',state,build),revision);
});

test('分类事件合并串行读取，重复数据不重绘，销毁后不提交迟到结果',async()=>{
  const {createOrganizationLoader}=await import('../src/lib/library-organization.js');
  const resolvers=[],commits=[];
  const loader=createOrganizationLoader(()=>new Promise(resolve=>resolvers.push(resolve)),data=>commits.push(data));
  const first=loader.refresh();loader.refresh();
  resolvers.shift()({tags:[],collections:[{id:'old',name:'旧',position:0}],gameTags:[],gameCollections:[]});
  await Promise.resolve();
  const latest={tags:[],collections:[{id:'new',name:'新',position:0}],gameTags:[],gameCollections:[]};
  resolvers.shift()(latest);await first;
  assert.deepEqual(commits.map(data=>data.collections[0].id),['old','new']);
  const unchanged=loader.refresh();resolvers.shift()(latest);await unchanged;assert.equal(commits.length,2);
  const late=loader.refresh();loader.dispose();resolvers.shift()({tags:[],collections:[],gameTags:[],gameCollections:[]});await late;
  assert.equal(commits.length,2);
});
