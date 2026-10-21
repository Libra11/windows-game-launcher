import test from 'node:test';
import assert from 'node:assert/strict';
import { describeCloudValue, resourceLabel, parseCloudValue } from '../src/ui/cloud-presentation.js';
import { importSummary, importGamePlan } from '../src/ui/cloud-import-summary.js';

test('冲突以中文业务含义呈现，删除不被当成普通更新',()=>{
  assert.equal(resourceLabel('favorite'),'星标收藏');
  assert.equal(describeCloudValue('favorite',{value:true}),'已收藏');
  assert.equal(describeCloudValue('favorite',{value:false}),'未收藏');
  assert.equal(describeCloudValue('tag',{name:'动作'},true),'已删除');
  assert.equal(describeCloudValue('tag',{name:'动作'}),'动作');
  assert.equal(describeCloudValue('official_snapshot',{unlocks:{A:null,B:null}}),'2 项官方解锁');
  assert.equal(parseCloudValue('{broken'),null);
});
import { readFile } from 'node:fs/promises';
class Node {
  constructor(tag,text=''){this.tag=tag;this.textContent=text;this.children=[];this.value='';}
  append(...nodes){this.children.push(...nodes);}
  replaceChildren(...nodes){this.children=nodes;}
  setAttribute(key,value){this[key]=value;}
  querySelectorAll(tag){return this.children.flatMap(c=>[...(c.tag===tag?[c]:[]),...c.querySelectorAll(tag)]);}
  remove(){}
  addEventListener(){}
}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
test('登录与导入须显式操作，官方冲突不提供保留旧快照',async()=>{
  const el=(tag,_cls,text)=>new Node(tag,text);
  globalThis.cloudUiTest={
    el,button:(label,_cls,click)=>Object.assign(new Node('button',label),{click}),
    field:(_label,value)=>({wrapper:new Node('label'),input:Object.assign(new Node('input'),{value})}),
    settingsSection:()=>new Node('section'),resourceLabel,describeCloudValue,parseCloudValue,importSummary,importGamePlan,
  };
  try{
    const source=(await readFile(new URL('../src/ui/settings-cloud.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
    const module=await import('data:text/javascript;base64,'+Buffer.from('const {el,button,field,settingsSection,resourceLabel,describeCloudValue,parseCloudValue,importSummary,importGamePlan}=globalThis.cloudUiTest;\n'+source).toString('base64'));
    let logged=false,commitResolve,commitReject;const calls=[];
    const actions={run:async(name,args)=>{
      calls.push({name,args});
      if(name==='cloud_status')return {phase:logged?'unbound':'login_required',loggedIn:logged,pending:0,games:[{id:'epic-game',title:'Epic 游戏',archived:false},{id:'steam-game',title:'Steam 游戏',archived:true}],conflicts:logged?[{kind:'official_snapshot',operationId:'op',local:'{"unlocks":{}}',result:'{"reason":"版本已变更","current":{"payload":{"unlocks":{}}}}'}]:[]};
      if(name==='cloud_challenge')return {challengeId:'challenge'};
      if(name==='cloud_verify'){logged=true;return;}
      if(name==='cloud_commit_import')return new Promise((resolve,reject)=>{commitResolve=resolve;commitReject=reject;});
      if(name==='cloud_prepare_import')return {resourceCount:2,resourceCounts:{game:1},mappings:[{localId:'steam-game',cloudId:'internal-cloud-id'}],localGames:[{id:'steam-game',title:'示例游戏',source:'steam'}],cloudGames:[],warnings:[],
        unresolvedCount:args.resolutions.length?0:1,conflicts:args.resolutions.length?[]:[{key:'favorite:local',kind:'favorite',local:{value:true},cloud:{value:false},reason:'状态不同',choices:['local','cloud']}]};
    }};
    const part=module.settingsCloud(actions);await flush();
    assert.deepEqual(calls.map(x=>x.name),['cloud_status']);
    const find=label=>part.element.querySelectorAll('button').find(b=>b.textContent===label);
    await find('发送验证码').click();await flush();
    await find('登录').click();await flush();
    assert.equal(calls.find(c=>c.name==='cloud_verify').args.challengeId,'challenge');
    assert.equal(find('保留本机'),undefined);
    assert.ok(find('采用云端'));
    const originalGames=part.element.querySelectorAll('select').find(n=>n['aria-label']==='选择要归档的游戏');
    originalGames.value='epic-game';
    await find('立即同步').click();await flush();
    assert.equal(part.element.querySelectorAll('select').find(n=>n['aria-label']==='选择要归档的游戏'),originalGames);
    assert.equal(originalGames.value,'epic-game');
    await find('预览资料导入').click();await flush();
    const automatic=part.element.querySelectorAll('details').find(n=>n.children.some(c=>c.tag==='summary'&&c.textContent.includes('自动处理的游戏')));
    assert.ok(automatic);
    assert.notEqual(automatic.open,true);
    const texts=node=>[node.textContent,...node.children.flatMap(texts)].join(' ');
    assert.equal(texts(part.element).includes('internal-cloud-id'),false);
    assert.ok(texts(automatic).includes('新增到云端'));
    assert.equal(calls.some(c=>c.name==='cloud_commit_import'),false);
    assert.equal(find('确认绑定并导入').hidden,true);
    const select=part.element.querySelectorAll('select').find(n=>n['aria-label']==='星标收藏 的导入处理');
    select.value='local';select.onchange();
    await find('按所选关联重新预览').click();await flush();
    assert.equal(find('确认绑定并导入').hidden,false);
    assert.deepEqual(calls.filter(c=>c.name==='cloud_prepare_import').at(-1).args.resolutions,[{key:'favorite:local',choice:'local'}]);
    const games=part.element.querySelectorAll('select').find(n=>n['aria-label']==='选择要归档的游戏');
    games.value='epic-game';await find('归档所选游戏').click();await flush();
    await find('恢复到游戏库').click();await flush();
    assert.deepEqual(calls.filter(c=>c.name==='cloud_set_archived').map(c=>c.args),[{gameId:'epic-game',archived:true},{gameId:'steam-game',archived:false}]);
    const confirm=find('确认绑定并导入');
    const pending=confirm.click();await flush();
    assert.equal(confirm.textContent,'正在导入…');assert.equal(confirm.disabled,true);
    await confirm.click();
    assert.equal(calls.filter(c=>c.name==='cloud_commit_import').length,1);
    commitReject('本地数据已变化，请重新预览');await pending;
    assert.equal(confirm.hidden,true);
    const notice=()=>part.element.querySelectorAll('p').find(n=>n['aria-live']==='polite');
    assert.match(notice().textContent,/本地数据已变化/);
    await find('立即同步').click();await flush();
    assert.match(notice().textContent,/本地数据已变化/);
    await find('按所选关联重新预览').click();await flush();
    const completed=find('确认绑定并导入').click();await flush();commitResolve();await completed;
    assert.match(notice().textContent,/导入已完成/);
    part.dispose();
  }finally{delete globalThis.cloudUiTest;}
});

test('存档入队不能显示为校验完成，查询后才确认云端可用',async()=>{
  let content;
  let pending=true;
  const calls=[];
  globalThis.savesUiTest={
    el:(tag,_cls,text)=>new Node(tag,text),
    button:(label,_cls,click)=>Object.assign(new Node('button',label),{click}),
    command:async(name)=>{
      calls.push(name);
      if(name==='saves_list') return [{profile:{id:'profile',name:'主存档',root:'C:/Save',cloudId:'cloud'},snapshots:[{id:'snapshot',createdAt:'2026-10-21',bytes:100,uploadStarted:true,uploaded:!pending}]}];
      if(name==='saves_upload')return {pending};
      throw new Error('未预期操作：'+name);
    },
    modal:(_title,_subtitle,node)=>{content=node;return node;},
    closeModal:async()=>{},field:()=>{},submit:()=>{},chooseFile:()=>{},preview:false,
    checkbox:()=>{},automationSettings:()=>{},taskSummary:()=>"",restoreWithReview:async()=>true,listen:async()=>()=>{},
  };
  try{
    const source=(await readFile(new URL('../src/ui/detail-saves.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
    const module=await import('data:text/javascript;base64,'+Buffer.from('const {el,button,command,modal,closeModal,field,submit,chooseFile,preview,checkbox,automationSettings,taskSummary,restoreWithReview,listen}=globalThis.savesUiTest;\n'+source).toString('base64'));
    module.savesButton({id:'game',title:'游戏'}).click();await flush();
    const upload=()=>content.querySelectorAll('button').find(b=>b.textContent==='上传 / 查询状态');
    await upload().click();await flush();
    const status=()=>content.querySelectorAll('p').find(n=>n.role==='status').textContent;
    assert.match(status(),/排队校验/);
    assert.doesNotMatch(status(),/校验通过|已通过/);
    assert.equal(calls.filter(n=>n==='saves_upload').length,1);
    pending=false;
    await upload().click();await flush();
    assert.equal(status(),'待传快照已通过云端校验');
    assert.ok(content.querySelectorAll('p').some(n=>n.textContent.includes('已上传 / 已下载')));
  }finally{delete globalThis.savesUiTest;}
});
